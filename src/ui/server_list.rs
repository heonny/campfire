//! The left panel: an Add button and the list of servers, each rendered as a
//! clickable card with a status dot, name, port, a duplicate-port marker, and
//! live CPU/memory while running.

use super::{Action, View, icon_button, icons, status_dot};
use crate::model::ServerConfig;
use crate::process::running::Status;
use crate::theme;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, view: &View, action: &mut Option<Action>) {
    theme::block_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.heading("Projects");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(icon_button(icons::add()))
                    .on_hover_text("Add project")
                    .clicked()
                {
                    *action = Some(Action::OpenNew);
                }
            });
        });
        ui.add_space(8.0);

        // The scroll area fills the remaining height, which also stretches the
        // block to the bottom of the panel.
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if view.servers.is_empty() {
                    ui.weak("등록된 프로젝트가 없습니다");
                    return;
                }
                for server in view.servers {
                    let running = view.running.get(&server.id);
                    let active = running.is_some_and(|p| !p.is_terminal());
                    Card {
                        server,
                        status: running
                            .map(|p| p.status().clone())
                            .unwrap_or(Status::Stopped),
                        // Guard on `active`: cached metrics linger up to a
                        // refresh interval after a server stops.
                        metrics: if active {
                            view.metrics.get(&server.id)
                        } else {
                            None
                        },
                        dup: server.port.is_some_and(|p| view.dup_ports.contains(&p)),
                        selected: view.selected == Some(server.id.as_str()),
                        active,
                    }
                    .show(ui, action);
                    ui.add_space(6.0);
                }
            });
    });
}

/// The per-card render inputs, bundled so [`Card::show`] keeps a small
/// signature (the fields all derive from the same server + runtime lookup).
struct Card<'a> {
    server: &'a ServerConfig,
    status: Status,
    metrics: Option<(f32, u64)>,
    dup: bool,
    selected: bool,
    active: bool,
}

impl Card<'_> {
    /// Render the card. A left click selects it; a right click opens the
    /// context menu. User intent is reported through `action`.
    fn show(&self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        let (fill, border) = if self.selected {
            (theme::ACCENT_WEAK, theme::ACCENT)
        } else {
            (theme::CARD_FILL, theme::CARD_BORDER)
        };
        let mut prepared = theme::card_frame()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, border))
            .begin(ui);
        {
            let ui = &mut prepared.content_ui;
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                status_dot(ui, &self.status);
                // Lay the right-aligned items out first, then give the name the
                // space that remains, truncated — so a narrow sidebar elides the
                // name instead of drawing it under the port.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.dup {
                        let warn = ui.visuals().warn_fg_color;
                        ui.colored_label(warn, "⚠").on_hover_text("duplicate port");
                    }
                    if let Some(port) = self.server.port {
                        ui.weak(format!(":{port}"));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(&self.server.name).truncate());
                    });
                });
            });
            metrics_row(ui, self.metrics);
        }
        let response = prepared.allocate_space(ui).interact(egui::Sense::click());
        if !self.selected && response.hovered() {
            prepared.frame.fill = theme::CARD_HOVER_FILL;
        }
        prepared.paint(ui);
        self.context_menu(&response, action);
        if response.clicked() {
            *action = Some(Action::Select(self.server.id.clone()));
        }
    }

    /// The right-click menu: lifecycle actions (Start, or Stop/Restart while
    /// running), then management (Duplicate, Edit), then Delete — set apart and
    /// error-colored, like the editor's Delete button, as the one destructive
    /// item. egui already styles its context menus so items are full-width and
    /// flat at rest, so the only styling here is roomier padding and a min width
    /// than that dense default, with groups set apart by a gap rather than a
    /// divider line (matching the app's spacing-over-separators layout).
    ///
    /// Labels are English to match the app's other action labels (the detail
    /// panel's Start/Stop/Restart/Edit); localization is a later, app-wide pass.
    fn context_menu(&self, response: &egui::Response, action: &mut Option<Action>) {
        response.context_menu(|ui| {
            ui.set_min_width(150.0);
            ui.spacing_mut().button_padding = egui::vec2(8.0, 5.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            let id = &self.server.id;
            if self.active {
                if ui.button("Stop").clicked() {
                    *action = Some(Action::Stop(id.clone()));
                    ui.close();
                }
                if ui.button("Restart").clicked() {
                    *action = Some(Action::Restart(id.clone()));
                    ui.close();
                }
            } else if ui.button("Start").clicked() {
                *action = Some(Action::Start(id.clone()));
                ui.close();
            }
            ui.add_space(4.0);
            if ui.button("Duplicate").clicked() {
                *action = Some(Action::Duplicate(id.clone()));
                ui.close();
            }
            if ui.button("Edit").clicked() {
                *action = Some(Action::OpenEdit(id.clone()));
                ui.close();
            }
            ui.add_space(4.0);
            let delete =
                egui::Button::new(egui::RichText::new("Delete").color(ui.visuals().error_fg_color));
            if ui.add(delete).clicked() {
                *action = Some(Action::Delete(id.clone()));
                ui.close();
            }
        });
    }
}

/// CPU/memory under the name while running. The row's space is reserved even
/// when idle so card heights don't change as servers start and stop.
fn metrics_row(ui: &mut egui::Ui, metrics: Option<(f32, u64)>) {
    ui.horizontal(|ui| {
        ui.add_space(20.0); // status dot (12) + item gap (8): align with the name
        match metrics {
            Some((cpu, mem)) => {
                let mem_mb = mem as f64 / 1_048_576.0;
                ui.label(
                    egui::RichText::new(format!("CPU {cpu:.0}% · {mem_mb:.0} MB"))
                        .small()
                        .weak(),
                );
            }
            None => {
                let height = ui.text_style_height(&egui::TextStyle::Small);
                ui.allocate_exact_size(egui::vec2(1.0, height), egui::Sense::hover());
            }
        }
    });
}
