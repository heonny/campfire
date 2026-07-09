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
                let mut reorder: Option<(usize, usize)> = None;
                for (idx, server) in view.servers.iter().enumerate() {
                    let running = view.running.get(&server.id);
                    let active = running.is_some_and(|p| !p.is_terminal());
                    let response = Card {
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
                    .show(ui, idx, action);
                    insertion_marker(ui, &response, idx, &mut reorder);
                    ui.add_space(6.0);
                }
                // Emitted once the whole list has laid out, so the insertion line
                // is painted this frame and the order changes on the next.
                if let Some((from, to)) = reorder {
                    *action = Some(Action::Reorder { from, to });
                }
            });
    });
}

/// Drag-and-drop payload for a card: its index in the current list. A newtype
/// because egui keys the drag payload by its Rust type — a bare `usize` would
/// collide with any other `usize` payload added elsewhere later.
#[derive(Clone, Copy)]
struct ServerDragIdx(usize);

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
    /// Render the card. A left click selects it; a drag reorders it; a right
    /// click opens the context menu. User intent is reported through `action`;
    /// the interaction [`egui::Response`] is returned so the caller can draw the
    /// drop-insertion line and detect the drop. `idx` is this card's position in
    /// the list — the payload carried while dragging.
    fn show(&self, ui: &mut egui::Ui, idx: usize, action: &mut Option<Action>) -> egui::Response {
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
        // `click_and_drag` so a short press still selects while a drag reorders.
        let response = prepared
            .allocate_space(ui)
            .interact(egui::Sense::click_and_drag());
        // Read the drag state off the response's OWN id: `interact` reuses the
        // auto id `allocate_space` assigned, so a hand-built id never matches
        // `is_being_dragged`. Set the payload (this card's index) on drag start;
        // egui holds it until the drop, where `insertion_marker` reads it back.
        let dragging = response.dragged();
        response.dnd_set_drag_payload(ServerDragIdx(idx));
        // Highlight the dragged card; otherwise the usual hover fill. The
        // selection tint already won above.
        if dragging {
            prepared.frame.fill = theme::ACCENT_WEAK;
        } else if !self.selected && response.hovered() {
            prepared.frame.fill = theme::CARD_HOVER_FILL;
        }
        prepared.paint(ui);
        let response = response.on_hover_cursor(if dragging {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
        self.context_menu(&response, action);
        // A drag ends as a release, not a click, so this only fires on a genuine
        // click — a reorder never also selects.
        if response.clicked() {
            *action = Some(Action::Select(self.server.id.clone()));
        }
        response
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

/// While a card is dragged over row `idx`, paint an insertion line at the
/// nearest edge and, on release, record the move as `(from, to)`. `to` is this
/// row (pointer in the top half) or the next (bottom half) — the slot the line
/// marks. Dropping a card onto itself yields `to == idx`, which `move_in_place`
/// treats as a no-op, so no line is drawn there.
fn insertion_marker(
    ui: &egui::Ui,
    response: &egui::Response,
    idx: usize,
    reorder: &mut Option<(usize, usize)>,
) {
    let Some(hovered) = response.dnd_hover_payload::<ServerDragIdx>() else {
        return;
    };
    let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) else {
        return;
    };
    let rect = response.rect;
    let to = if hovered.0 == idx {
        idx // hovering its own card — no marker, drop is a no-op
    } else {
        let stroke = egui::Stroke::new(2.0, theme::ACCENT);
        if pointer.y < rect.center().y {
            ui.painter().hline(rect.x_range(), rect.top(), stroke);
            idx
        } else {
            ui.painter().hline(rect.x_range(), rect.bottom(), stroke);
            idx + 1
        }
    };
    if let Some(dragged) = response.dnd_release_payload::<ServerDragIdx>() {
        *reorder = Some((dragged.0, to));
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
