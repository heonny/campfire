//! Campfire — local multi-server manager.
//!
//! Targets egui/eframe 0.35: the app entry point is `App::ui(&mut Ui, ..)` and
//! panels use the unified `egui::Panel` type shown into a `&mut Ui`. Rendering
//! lives in the `ui` module; this file owns state and applies actions.

mod ansi;
mod metrics;
mod model;
mod port;
mod process;
mod store;
mod theme;
mod ui;

use eframe::egui;
use model::ServerConfig;
use process::running::RunningProcess;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use ui::editor::{EditorForm, EditorOutcome};
use ui::log_view::LogView;
use ui::Action;

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
            theme::setup(&cc.egui_ctx);
            Ok(Box::new(CampfireApp::new()))
        }),
    )
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
    /// Servers awaiting a restart once their current process has terminated.
    restart_pending: HashSet<String>,
    /// Cached per-server CPU/memory usage.
    metrics: metrics::Metrics,
    /// Whether the help modal is open.
    show_help: bool,
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
            restart_pending: HashSet::new(),
            metrics: metrics::Metrics::new(),
            show_help: false,
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
            Action::Restart(id) => {
                let active = self.running.get(&id).is_some_and(|p| !p.is_terminal());
                if active {
                    if let Some(proc) = self.running.get_mut(&id) {
                        proc.stop(Duration::from_secs(3));
                    }
                    self.restart_pending.insert(id); // relaunched once terminated
                } else {
                    self.start_server(&id, ctx);
                }
            }
            Action::ClearLogs(id) => {
                if let Some(proc) = self.running.get_mut(&id) {
                    proc.clear_logs();
                }
            }
            Action::Select(id) => self.selected = Some(id),
            Action::OpenNew => self.editor = Some(EditorForm::new_server()),
            Action::OpenEdit(id) => {
                if let Some(server) = self.servers.iter().find(|s| s.id == id) {
                    self.editor = Some(EditorForm::from_config(server));
                }
            }
            Action::OpenHelp => self.show_help = true,
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
        self.metrics.refresh(&self.running);
        if self.running.values().any(|p| !p.is_terminal()) {
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }

        // Complete restarts whose old process has terminated (frees the port).
        if !self.restart_pending.is_empty() {
            let ready: Vec<String> = self
                .restart_pending
                .iter()
                .filter(|&id| self.running.get(id).is_none_or(|p| p.is_terminal()))
                .cloned()
                .collect();
            let ctx = ui.ctx().clone();
            for id in ready {
                self.restart_pending.remove(&id);
                self.running.remove(&id);
                self.start_server(&id, ctx.clone());
            }
        }

        let dup_ports = port::duplicate_config_ports(&self.servers);
        let mut action: Option<Action> = None;

        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Campfire");
                ui.separator();
                let active = self.running.values().filter(|p| !p.is_terminal()).count();
                ui.label(format!("running {active}/{}", self.servers.len()));
                if ui.button("Help").clicked() {
                    action = Some(Action::OpenHelp);
                }
                if let Some(notice) = &self.notice {
                    ui.separator();
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, notice);
                }
            });
        });

        let view = ui::View {
            servers: &self.servers,
            running: &self.running,
            dup_ports: &dup_ports,
            selected: self.selected.as_deref(),
            metrics: &self.metrics,
        };
        egui::Panel::left("server_list")
            .default_size(220.0)
            .show(ui, |ui| ui::server_list::show(ui, &view, &mut action));
        egui::CentralPanel::default()
            .show(ui, |ui| ui::detail::show(ui, &view, &mut self.log_view, &mut action));

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

        if self.show_help {
            let response = egui::Modal::new(egui::Id::new("help")).show(ui.ctx(), ui::help::show);
            if response.should_close() || response.inner {
                self.show_help = false;
            }
        }
    }
}
