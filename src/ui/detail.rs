//! The central panel: the selected server's header card (status, port, actions,
//! port-conflict warnings, command) and the log view below it.

use super::{Action, View, icon_button, icons, status_color, status_text};
use crate::model::ServerConfig;
use crate::process::log_buffer::LogBuffer;
use crate::process::running::{RunningProcess, Status};
use crate::theme;
use crate::ui::log_view::{self, LogView};
use eframe::egui;

pub fn show(
    ui: &mut egui::Ui,
    view: &View,
    log_view_state: &mut LogView,
    action: &mut Option<Action>,
) {
    let selected = view
        .selected
        .and_then(|id| view.servers.iter().find(|s| s.id == id));
    let Some(server) = selected else {
        theme::block_frame().show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                ui.weak("좌측에서 프로젝트를 선택하거나 + 로 추가하세요");
            });
        });
        return;
    };

    let proc = view.running.get(&server.id);
    let status = proc.map(|p| p.status().clone()).unwrap_or(Status::Stopped);
    let active = proc.map(|p| !p.is_terminal()).unwrap_or(false);
    let recovered = proc.map(|p| p.is_recovered()).unwrap_or(false);

    theme::block_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        header_row(ui, server, &status, active, recovered, action);
        port_warnings(ui, view, server, active);
        command_block(ui, server);
    });

    ui.add_space(10.0);
    log_section(ui, server, proc, recovered, log_view_state, action);
}

/// The top row of the header card: status dot, name, port, status text, and the
/// right-aligned lifecycle actions.
fn header_row(
    ui: &mut egui::Ui,
    server: &ServerConfig,
    status: &Status,
    active: bool,
    recovered: bool,
    action: &mut Option<Action>,
) {
    ui.horizontal(|ui| {
        // Lifecycle controls sit at the right edge; Edit (config) is set apart.
        // They are laid out first so the name can truncate into what remains
        // instead of running under them when the pane gets narrow.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if active {
                if ui
                    .add(icon_button(icons::stop()))
                    .on_hover_text("Stop")
                    .clicked()
                {
                    *action = Some(Action::Stop(server.id.clone()));
                }
                if ui
                    .add(icon_button(icons::restart()))
                    .on_hover_text("Restart")
                    .clicked()
                {
                    *action = Some(Action::Restart(server.id.clone()));
                }
            } else if ui
                .add(icon_button(icons::start()))
                .on_hover_text("Start")
                .clicked()
            {
                *action = Some(Action::Start(server.id.clone()));
            }
            ui.add_space(6.0);
            if ui
                .add(icon_button(icons::edit()))
                .on_hover_text("Edit")
                .clicked()
            {
                *action = Some(Action::OpenEdit(server.id.clone()));
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(egui::RichText::new(&server.name).heading()).truncate());
                if let Some(port) = server.port {
                    ui.weak(format!(":{port}"));
                }
                status_badge(ui, status);
                if recovered {
                    ui.weak("recovered").on_hover_text(
                        "Running since before this app started — restart for live logs",
                    );
                }
            });
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
            ui.label(
                egui::RichText::new(status_text(status))
                    .color(color)
                    .small(),
            );
        });
}

/// Any port-conflict warnings. (CPU/memory lives on the sidebar cards.)
fn port_warnings(ui: &mut egui::Ui, view: &View, server: &ServerConfig, active: bool) {
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

/// The log view. It renders the same whether or not the server is running — when
/// there is no process it renders over an empty buffer, so the panel never
/// collapses to a bare placeholder (the bottom control bar keeps it grounded).
/// The one exception is an adopted (recovered) process: it has no live pipe to
/// stream or clear, so it explains that instead of offering controls that can't
/// do anything.
fn log_section(
    ui: &mut egui::Ui,
    server: &ServerConfig,
    proc: Option<&RunningProcess>,
    recovered: bool,
    log_view_state: &mut LogView,
    action: &mut Option<Action>,
) {
    if recovered {
        theme::block_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(ui.available_height());
            ui.weak(
                "Recovered from a previous session — live logs aren't available. \
                 Restart to stream logs.",
            );
        });
        return;
    }

    let empty = LogBuffer::default();
    let logs = proc.map(|p| p.logs()).unwrap_or(&empty);
    let clear = theme::block_frame()
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(ui.available_height());
            log_view::show(ui, log_view_state, logs)
        })
        .inner;
    if clear {
        *action = Some(Action::ClearLogs(server.id.clone()));
    }
}
