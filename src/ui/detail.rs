//! The central panel: the selected server's header (status, port, actions),
//! CPU/memory, port-conflict warnings, its command, and the log view.

use super::{status_dot, status_text, Action, View};
use crate::process::running::Status;
use crate::ui::log_view::{self, LogView};
use eframe::egui;

pub fn show(ui: &mut egui::Ui, view: &View, log_view_state: &mut LogView, action: &mut Option<Action>) {
    let selected = view
        .selected
        .and_then(|id| view.servers.iter().find(|s| s.id == id));
    let Some(server) = selected else {
        ui.centered_and_justified(|ui| {
            ui.weak("좌측에서 서버를 선택하거나 + Add로 추가하세요");
        });
        return;
    };

    let proc = view.running.get(&server.id);
    let status = proc.map(|p| p.status().clone()).unwrap_or(Status::Stopped);
    let active = proc.map(|p| !p.is_terminal()).unwrap_or(false);

    ui.horizontal(|ui| {
        status_dot(ui, &status);
        ui.heading(&server.name);
        if let Some(port) = server.port {
            ui.weak(format!(":{port}"));
        }
        ui.label(status_text(&status));
        if active {
            if ui.button("Stop").clicked() {
                *action = Some(Action::Stop(server.id.clone()));
            }
            if ui.button("Restart").clicked() {
                *action = Some(Action::Restart(server.id.clone()));
            }
        } else if ui.button("Start").clicked() {
            *action = Some(Action::Start(server.id.clone()));
        }
        if ui.button("Edit").clicked() {
            *action = Some(Action::OpenEdit(server.id.clone()));
        }
    });

    if active
        && let Some((cpu, mem)) = view.metrics.get(&server.id)
    {
        let mem_mb = mem as f64 / 1_048_576.0;
        ui.weak(format!("CPU {cpu:.0}%   ·   {mem_mb:.0} MB"));
    }

    if let Some(assigned) = server.port {
        let warn = ui.visuals().warn_fg_color;
        if view.dup_ports.contains(&assigned) {
            ui.colored_label(
                warn,
                format!("port {assigned} is also assigned to another server in config"),
            );
        } else if !active && !crate::port::is_port_free(assigned) {
            ui.colored_label(warn, format!("port {assigned} is already in use"));
        }
    }

    ui.monospace(&server.command);
    ui.separator();

    match proc {
        Some(proc) => {
            if log_view::show(ui, log_view_state, proc.logs()) {
                *action = Some(Action::ClearLogs(server.id.clone()));
            }
        }
        None => {
            ui.weak("no output yet");
        }
    }
}
