//! The left panel: an Add button and the list of servers, each rendered as a
//! clickable card with a status dot, name, port, and a duplicate-port marker.

use super::{action_button, icons, status_dot, Action, View};
use crate::model::ServerConfig;
use crate::process::running::Status;
use crate::theme;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, view: &View, action: &mut Option<Action>) {
    ui.horizontal(|ui| {
        ui.heading("Servers");
        if ui.add(action_button(icons::add(), "Add")).clicked() {
            *action = Some(Action::OpenNew);
        }
    });
    ui.add_space(8.0);

    if view.servers.is_empty() {
        ui.weak("(등록된 서버 없음)");
        return;
    }

    for server in view.servers {
        let is_selected = view.selected == Some(server.id.as_str());
        let status = view
            .running
            .get(&server.id)
            .map(|p| p.status().clone())
            .unwrap_or(Status::Stopped);
        let dup = server.port.is_some_and(|p| view.dup_ports.contains(&p));

        if card(ui, server, &status, dup, is_selected) {
            *action = Some(Action::Select(server.id.clone()));
        }
        ui.add_space(6.0);
    }
}

/// Render one server card. Returns `true` if it was clicked.
fn card(
    ui: &mut egui::Ui,
    server: &ServerConfig,
    status: &Status,
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
            ui.label(&server.name);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if dup {
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, "⚠").on_hover_text("duplicate port");
                }
                if let Some(port) = server.port {
                    ui.weak(format!(":{port}"));
                }
            });
        });
    }
    let response = prepared.allocate_space(ui).interact(egui::Sense::click());
    if !selected && response.hovered() {
        prepared.frame.fill = theme::CARD_HOVER_FILL;
    }
    prepared.paint(ui);
    response.clicked()
}
