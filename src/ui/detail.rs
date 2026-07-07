//! The central panel: the selected server's header card (status, port, actions,
//! CPU/memory, port-conflict warnings, command) and the log view below it.

use super::{status_dot, status_text, Action, View};
use crate::model::ServerConfig;
use crate::process::running::{RunningProcess, Status};
use crate::theme;
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

    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        header_row(ui, server, &status, active, action);
        meta_rows(ui, view, server, active);
        command_block(ui, server);
    });

    ui.add_space(8.0);
    log_section(ui, server, proc, log_view_state, action);
}

/// The top row of the header card: status dot, name, port, status text, and the
/// right-aligned lifecycle actions.
fn header_row(
    ui: &mut egui::Ui,
    server: &ServerConfig,
    status: &Status,
    active: bool,
    action: &mut Option<Action>,
) {
    ui.horizontal(|ui| {
        status_dot(ui, status);
        ui.heading(&server.name);
        if let Some(port) = server.port {
            ui.weak(format!(":{port}"));
        }
        ui.weak(status_text(status));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Edit").clicked() {
                *action = Some(Action::OpenEdit(server.id.clone()));
            }
            if active {
                if ui.button("Restart").clicked() {
                    *action = Some(Action::Restart(server.id.clone()));
                }
                if ui.button("Stop").clicked() {
                    *action = Some(Action::Stop(server.id.clone()));
                }
            } else if ui.button("Start").clicked() {
                *action = Some(Action::Start(server.id.clone()));
            }
        });
    });
}

/// CPU/memory (when running) and any port-conflict warnings.
fn meta_rows(ui: &mut egui::Ui, view: &View, server: &ServerConfig, active: bool) {
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
}

/// The run command, shown in a subtle inset "code block".
fn command_block(ui: &mut egui::Ui, server: &ServerConfig) {
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(egui::Stroke::new(1.0, theme::CARD_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(egui::RichText::new(&server.command).monospace()).selectable(true));
        });
}

/// The log view (or a placeholder when the server has never run).
fn log_section(
    ui: &mut egui::Ui,
    server: &ServerConfig,
    proc: Option<&RunningProcess>,
    log_view_state: &mut LogView,
    action: &mut Option<Action>,
) {
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
