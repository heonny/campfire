//! The central panel: the selected server's header card (status, port, actions,
//! CPU/memory, port-conflict warnings, command) and the log view below it.

use super::{icon_button, icons, status_color, status_text, Action, View};
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
            ui.weak("좌측에서 프로젝트를 선택하거나 + 로 추가하세요");
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
        ui.heading(&server.name);
        if let Some(port) = server.port {
            ui.weak(format!(":{port}"));
        }
        status_badge(ui, status);
        // Lifecycle controls sit at the right edge; Edit (config) is set apart.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if active {
                if ui.add(icon_button(icons::stop())).on_hover_text("Stop").clicked() {
                    *action = Some(Action::Stop(server.id.clone()));
                }
                if ui.add(icon_button(icons::restart())).on_hover_text("Restart").clicked() {
                    *action = Some(Action::Restart(server.id.clone()));
                }
            } else if ui.add(icon_button(icons::start())).on_hover_text("Start").clicked() {
                *action = Some(Action::Start(server.id.clone()));
            }
            ui.add_space(6.0);
            if ui.add(icon_button(icons::edit())).on_hover_text("Edit").clicked() {
                *action = Some(Action::OpenEdit(server.id.clone()));
            }
        });
    });
}

/// A small status pill: the status text in its color, on a pale tint of it.
fn status_badge(ui: &mut egui::Ui, status: &Status) {
    let color = status_color(status);
    let tint = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 36);
    egui::Frame::new()
        .fill(tint)
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(status_text(status)).color(color).small());
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
    theme::inset_frame().show(ui, |ui| {
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
            let clear = theme::card_frame()
                .fill(egui::Color32::WHITE)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    log_view::show(ui, log_view_state, proc.logs())
                })
                .inner;
            if clear {
                *action = Some(Action::ClearLogs(server.id.clone()));
            }
        }
        None => {
            ui.weak("no output yet");
        }
    }
}
