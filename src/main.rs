//! Campfire — local multi-server manager.
//!
//! Targets egui/eframe 0.35: the app entry point is `App::ui(&mut Ui, ..)` and
//! panels use the unified `egui::Panel` type shown into a `&mut Ui`.

mod model;
mod port;
mod process;
mod store;
mod ui;

use eframe::egui;
use model::ServerConfig;
use process::running::{RunningProcess, Status};
use std::collections::HashMap;
use std::time::Duration;
use ui::editor::{EditorForm, EditorOutcome};
use ui::log_view::LogView;

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
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(CampfireApp::new()))
        }),
    )
}

/// Register a Korean font (Nanum Gothic, OFL) as a fallback so Hangul renders —
/// egui's default fonts cover only Latin and emoji.
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "nanum".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/NanumGothic-Regular.ttf"
        ))),
    );
    // Append as a fallback: Latin keeps the default font, Hangul resolves here.
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("nanum".to_owned());
    }
    ctx.set_fonts(fonts);
}

/// A user action captured during rendering, applied after the panels close so
/// that the UI closures never need a mutable borrow of `self`.
enum Action {
    Start(String),
    Stop(String),
    ClearLogs(String),
    OpenNew,
    OpenEdit(String),
}

/// Root application state.
struct CampfireApp {
    servers: Vec<ServerConfig>,
    /// Id of the selected server (stable across list edits), if any.
    selected: Option<String>,
    /// Live processes, keyed by `ServerConfig::id`.
    running: HashMap<String, RunningProcess>,
    /// Open add/edit form, if any.
    editor: Option<EditorForm>,
    /// Log pane state (search text, follow toggle).
    log_view: LogView,
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
        Self {
            servers,
            selected: None,
            running: HashMap::new(),
            editor: None,
            log_view: LogView::new(),
            notice: None,
        }
    }

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

    fn apply_action(&mut self, action: Action, ctx: egui::Context) {
        match action {
            Action::Start(id) => self.start_server(&id, ctx),
            Action::Stop(id) => {
                if let Some(proc) = self.running.get_mut(&id) {
                    proc.stop(Duration::from_secs(3));
                }
            }
            Action::ClearLogs(id) => {
                if let Some(proc) = self.running.get_mut(&id) {
                    proc.clear_logs();
                }
            }
            Action::OpenNew => self.editor = Some(EditorForm::new_server()),
            Action::OpenEdit(id) => {
                if let Some(server) = self.servers.iter().find(|s| s.id == id) {
                    self.editor = Some(EditorForm::from_config(server));
                }
            }
        }
    }

    fn apply_editor_outcome(&mut self, outcome: EditorOutcome) {
        match outcome {
            EditorOutcome::None => {}
            EditorOutcome::Cancel => self.editor = None,
            EditorOutcome::Save(config) => {
                match self.servers.iter_mut().find(|s| s.id == config.id) {
                    Some(existing) => *existing = config,
                    None => self.servers.push(config),
                }
                self.persist();
                self.editor = None;
            }
            EditorOutcome::Delete(id) => {
                self.running.remove(&id); // dropped -> Drop force-kills the group
                self.servers.retain(|s| s.id != id);
                if self.selected.as_deref() == Some(id.as_str()) {
                    self.selected = None;
                }
                self.persist();
                self.editor = None;
            }
        }
    }

    fn persist(&mut self) {
        let doc = store::ConfigDoc {
            servers: self.servers.clone(),
            ..store::ConfigDoc::default()
        };
        if let Err(err) = store::save(&doc) {
            self.notice = Some(format!("Failed to save config: {err}"));
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
                ui.horizontal(|ui| {
                    ui.heading("Servers");
                    if ui.button("+ Add").clicked() {
                        action = Some(Action::OpenNew);
                    }
                });
                ui.separator();
                if self.servers.is_empty() {
                    ui.weak("(등록된 서버 없음)");
                } else {
                    let mut clicked: Option<String> = None;
                    for server in &self.servers {
                        let is_selected = self.selected.as_deref() == Some(server.id.as_str());
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
                        if ui.selectable_label(is_selected, label).clicked() {
                            clicked = Some(server.id.clone());
                        }
                    }
                    if clicked.is_some() {
                        self.selected = clicked;
                    }
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let selected = self
                .selected
                .as_ref()
                .and_then(|id| self.servers.iter().find(|s| &s.id == id));
            let Some(server) = selected else {
                ui.centered_and_justified(|ui| {
                    ui.weak("좌측에서 서버를 선택하거나 + Add로 추가하세요");
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
                if ui.button("Edit").clicked() {
                    action = Some(Action::OpenEdit(server.id.clone()));
                }
            });

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
                Some(proc) => {
                    if ui::log_view::show(ui, &mut self.log_view, proc.logs()) {
                        action = Some(Action::ClearLogs(server.id.clone()));
                    }
                }
                None => {
                    ui.weak("no output yet");
                }
            }
        });

        if let Some(action) = action {
            self.apply_action(action, ui.ctx().clone());
        }

        if self.editor.is_some() {
            let mut outcome = EditorOutcome::None;
            if let Some(form) = &mut self.editor {
                let response = egui::Modal::new(egui::Id::new("server_editor"))
                    .show(ui.ctx(), |ui| ui::editor::show(ui, form));
                let dismissed = response.should_close();
                outcome = response.inner;
                if dismissed && matches!(outcome, EditorOutcome::None) {
                    outcome = EditorOutcome::Cancel;
                }
            }
            self.apply_editor_outcome(outcome);
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
