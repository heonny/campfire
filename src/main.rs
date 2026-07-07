//! Campfire — local multi-server manager.
//!
//! Targets egui/eframe 0.35: the app entry point is `App::ui(&mut Ui, ..)` and
//! panels use the unified `egui::Panel` type shown into a `&mut Ui`.

mod model;
mod port;
mod process;
mod store;

use eframe::egui;
use model::ServerConfig;
use process::log_buffer::LogBuffer;
use process::running::{RunningProcess, Status};
use std::collections::HashMap;
use std::time::Duration;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Campfire")
            .with_inner_size([1024.0, 640.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Campfire",
        options,
        Box::new(|_cc| Ok(Box::new(CampfireApp::new()))),
    )
}

/// A user action captured during rendering, applied after the panels close so
/// that the UI closures never need a mutable borrow of `self`.
enum Action {
    Start(String),
    Stop(String),
}

/// Root application state.
struct CampfireApp {
    servers: Vec<ServerConfig>,
    selected: Option<usize>,
    /// Live processes, keyed by `ServerConfig::id`.
    running: HashMap<String, RunningProcess>,
    /// Transient one-line notice (e.g. a port conflict that blocked a start).
    notice: Option<String>,
}

impl CampfireApp {
    fn new() -> Self {
        let servers = match store::load() {
            Ok(doc) => doc.servers,
            Err(err) => {
                eprintln!("campfire: could not load config, starting empty ({err})");
                Vec::new()
            }
        };
        Self { servers, selected: None, running: HashMap::new(), notice: None }
    }

    /// Spawn the server with `id`, refusing if its port is already taken.
    fn start_server(&mut self, id: &str, ctx: egui::Context) {
        let Some(server) = self.servers.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        if let Some(port) = server.port
            && !port::is_port_free(port)
        {
            self.notice =
                Some(format!("Can't start '{}': port {port} is already in use.", server.name));
            return;
        }
        let wake = move || ctx.request_repaint();
        match RunningProcess::spawn(&server, wake) {
            Ok(proc) => {
                self.notice = None;
                self.running.insert(server.id, proc);
            }
            Err(err) => {
                self.notice = Some(format!("Failed to start '{}': {err}", server.name));
            }
        }
    }
}

impl eframe::App for CampfireApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Drive live processes: drain logs, detect exit, escalate shutdown.
        for proc in self.running.values_mut() {
            proc.poll();
        }
        if self.running.values().any(|p| !p.is_terminal()) {
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }

        let dup_ports = port::duplicate_config_ports(&self.servers);
        let mut action: Option<Action> = None;

        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Campfire");
                ui.separator();
                let active = self.running.values().filter(|p| !p.is_terminal()).count();
                ui.label(format!("running {active}/{}", self.servers.len()));
                if let Some(notice) = &self.notice {
                    ui.separator();
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, notice);
                }
            });
        });

        egui::Panel::left("server_list")
            .default_size(220.0)
            .show(ui, |ui| {
                ui.heading("Servers");
                ui.separator();
                if self.servers.is_empty() {
                    ui.weak("(등록된 서버 없음)");
                } else {
                    let mut clicked = None;
                    for (index, server) in self.servers.iter().enumerate() {
                        let state_tag = match self.running.get(&server.id).map(|p| p.status()) {
                            Some(Status::Running | Status::Starting) => "  · running",
                            Some(Status::Crashed { .. }) => "  · crashed",
                            _ => "",
                        };
                        let dup_tag = match server.port {
                            Some(p) if dup_ports.contains(&p) => "  [dup port]",
                            _ => "",
                        };
                        let label =
                            format!("{}{}{state_tag}{dup_tag}", server.name, port_suffix(server));
                        if ui.selectable_label(self.selected == Some(index), label).clicked() {
                            clicked = Some(index);
                        }
                    }
                    if clicked.is_some() {
                        self.selected = clicked;
                    }
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(server) = self.selected.and_then(|i| self.servers.get(i)) else {
                ui.centered_and_justified(|ui| {
                    ui.weak("좌측에서 서버를 선택하거나 새로 추가하세요");
                });
                return;
            };
            let proc = self.running.get(&server.id);
            let status = proc.map(|p| p.status().clone()).unwrap_or(Status::Stopped);
            let active = proc.map(|p| !p.is_terminal()).unwrap_or(false);

            ui.horizontal(|ui| {
                ui.heading(&server.name);
                ui.label(status_text(&status));
                if active {
                    if ui.button("Stop").clicked() {
                        action = Some(Action::Stop(server.id.clone()));
                    }
                } else if ui.button("Start").clicked() {
                    action = Some(Action::Start(server.id.clone()));
                }
            });

            // Port-conflict warnings (requirement: surface port clashes).
            if let Some(assigned) = server.port {
                let warn = ui.visuals().warn_fg_color;
                if dup_ports.contains(&assigned) {
                    ui.colored_label(
                        warn,
                        format!("port {assigned} is also assigned to another server in config"),
                    );
                } else if !active && !port::is_port_free(assigned) {
                    ui.colored_label(warn, format!("port {assigned} is already in use"));
                }
            }

            ui.monospace(&server.command);
            ui.separator();

            match proc {
                Some(proc) => render_logs(ui, proc.logs()),
                None => {
                    ui.weak("no output yet");
                }
            }
        });

        match action {
            Some(Action::Start(id)) => self.start_server(&id, ui.ctx().clone()),
            Some(Action::Stop(id)) => {
                if let Some(proc) = self.running.get_mut(&id) {
                    proc.stop(Duration::from_secs(3));
                }
            }
            None => {}
        }
    }
}

/// `"   :8080"` when a port is set, else empty.
fn port_suffix(server: &ServerConfig) -> String {
    match server.port {
        Some(port) => format!("   :{port}"),
        None => String::new(),
    }
}

fn status_text(status: &Status) -> String {
    match status {
        Status::Stopped => "stopped".to_string(),
        Status::Starting => "starting".to_string(),
        Status::Running => "running".to_string(),
        Status::Crashed { code: Some(code) } => format!("crashed (exit {code})"),
        Status::Crashed { code: None } => "crashed".to_string(),
    }
}

/// Minimal log view (step 3): the last lines, tailing. The full search/filter
/// viewer lands in step 5.
fn render_logs(ui: &mut egui::Ui, logs: &LogBuffer) {
    if logs.is_empty() {
        ui.weak("no output yet");
        return;
    }
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let start = logs.len().saturating_sub(1000);
            for index in start..logs.len() {
                if let Some(line) = logs.get(index) {
                    ui.monospace(&line.text);
                }
            }
        });
}
