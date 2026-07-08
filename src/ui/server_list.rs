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
                    let is_selected = view.selected == Some(server.id.as_str());
                    let running = view.running.get(&server.id);
                    let status = running
                        .map(|p| p.status().clone())
                        .unwrap_or(Status::Stopped);
                    let active = running.is_some_and(|p| !p.is_terminal());
                    // Guard on `active`: cached metrics linger up to a refresh
                    // interval after a server stops.
                    let metrics = if active {
                        view.metrics.get(&server.id)
                    } else {
                        None
                    };
                    let dup = server.port.is_some_and(|p| view.dup_ports.contains(&p));

                    if card(ui, server, &status, metrics, dup, is_selected) {
                        *action = Some(Action::Select(server.id.clone()));
                    }
                    ui.add_space(6.0);
                }
            });
    });
}

/// Render one server card. Returns `true` if it was clicked.
fn card(
    ui: &mut egui::Ui,
    server: &ServerConfig,
    status: &Status,
    metrics: Option<(f32, u64)>,
    dup: bool,
    selected: bool,
) -> bool {
    let (fill, border) = if selected {
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
            status_dot(ui, status);
            // Lay the right-aligned items out first, then give the name the
            // space that remains, truncated — so a narrow sidebar elides the
            // name instead of drawing it under the port.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if dup {
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, "⚠").on_hover_text("duplicate port");
                }
                if let Some(port) = server.port {
                    ui.weak(format!(":{port}"));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(&server.name).truncate());
                });
            });
        });
        metrics_row(ui, metrics);
    }
    let response = prepared.allocate_space(ui).interact(egui::Sense::click());
    if !selected && response.hovered() {
        prepared.frame.fill = theme::CARD_HOVER_FILL;
    }
    prepared.paint(ui);
    response.clicked()
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
