//! The left panel: an Add button and the list of servers, each with a status
//! dot, name/port, and a duplicate-port marker.

use super::{port_suffix, status_dot, Action, View};
use crate::process::running::Status;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, view: &View, action: &mut Option<Action>) {
    ui.horizontal(|ui| {
        ui.heading("Servers");
        if ui.button("+ Add").clicked() {
            *action = Some(Action::OpenNew);
        }
    });
    ui.separator();

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

        let response = ui
            .horizontal(|ui| {
                status_dot(ui, &status);
                let label = format!("{}{}", server.name, port_suffix(server));
                let response = ui.selectable_label(is_selected, label);
                if dup {
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, "dup");
                }
                response
            })
            .inner;
        if response.clicked() {
            *action = Some(Action::Select(server.id.clone()));
        }
    }
}
