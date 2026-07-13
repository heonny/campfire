//! Campfire — local multi-server manager.
//!
//! Targets egui/eframe 0.35: the app entry point is `App::ui(&mut Ui, ..)` and
//! panels use the unified `egui::Panel` type shown into a `&mut Ui`. Rendering
//! lives in the `ui` module; this file owns state and applies actions.

mod ansi;
mod fs_util;
mod gradle;
mod metrics;
mod model;
mod port;
mod process;
mod project;
mod search;
mod store;
mod theme;
mod ui;

use eframe::egui;
use model::ServerConfig;
use process::kill_tree;
use process::running::RunningProcess;
use process::runtime_state::{self, RuntimeEntry};
use process::shutdown;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;
use ui::Action;
use ui::editor::{EditorForm, EditorOutcome};
use ui::workspaces::Workspaces;

fn main() -> eframe::Result<()> {
    // Relay signal-based termination (SIGTERM/SIGINT/…) to our server groups,
    // since a signal skips the Drop that normally kills them on window close.
    shutdown::install_handler();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Campfire")
        .with_inner_size([1024.0, 640.0])
        .with_min_inner_size([720.0, 480.0]);
    if let Ok(icon) =
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/images/logo-mac.png"))
    {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Campfire",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            theme::setup(&cc.egui_ctx);
            Ok(Box::new(CampfireApp::new(cc.egui_ctx.clone())))
        }),
    )
}

/// How long a graceful stop waits for a server to exit on its own before the
/// group is force-killed. Long enough for a framework's shutdown hooks to run —
/// draining connections, closing pools — and print their logs (Spring Boot's
/// graceful shutdown, etc.), which a shorter window would cut off. This bounds
/// only a UI-initiated Stop/Restart; closing the app force-kills at once via
/// `Drop`, and pressing Stop again during the wait escalates immediately.
const STOP_GRACE: Duration = Duration::from_secs(10);

/// Sidebar layout: `egui::Panel::show_switched` cross-fades between the full
/// card list (`SIDEBAR_DEFAULT_WIDTH`, resizable down to `SIDEBAR_MIN_WIDTH`) and
/// a `SIDEBAR_RAIL_WIDTH` rail of per-server status dots. Dragging the resize bar
/// in past the rail's width collapses it; pulling the rail's edge back out
/// expands it. egui shares one resize handle across both, so a single drag can
/// do either, and it drives the `is_expanded` flag for us.
const SIDEBAR_DEFAULT_WIDTH: f32 = 240.0;
const SIDEBAR_MIN_WIDTH: f32 = 150.0;
const SIDEBAR_RAIL_WIDTH: f32 = 44.0;
/// How long the sidebar collapse/expand slide takes. Longer than egui's 0.2 s
/// default so this wide panel eases smoothly instead of snapping; scoped to the
/// panel so hover and other feedback keep the snappy default.
const SIDEBAR_SLIDE_TIME: f32 = 0.35;

/// Root application state.
struct CampfireApp {
    servers: Vec<ServerConfig>,
    /// Live processes, keyed by `ServerConfig::id`.
    running: HashMap<String, RunningProcess>,
    /// Open add/edit form, if any.
    editor: Option<EditorForm>,
    /// The workspace tabs and their log-pane splits (mutable UI state; process
    /// data stays in `running`).
    workspaces: Workspaces,
    /// Transient one-line notice (e.g. a port conflict that blocked a start).
    notice: Option<String>,
    /// Servers awaiting a restart once their current process has terminated.
    restart_pending: HashSet<String>,
    /// Cached per-server CPU/memory usage.
    metrics: metrics::Metrics,
    /// Whether the help modal is open.
    show_help: bool,
    /// Id of the server awaiting delete confirmation while the confirm dialog is
    /// open. Both the sidebar context menu and the editor's Delete button set
    /// this; the actual removal happens only on explicit confirm.
    pending_delete: Option<String>,
    /// Lazily-loaded top-bar logo texture.
    logo: Option<egui::TextureHandle>,
    /// Servers currently running, mirrored to disk so a force-killed instance
    /// can reconcile orphaned processes on the next launch. Keyed by server id.
    runtime: HashMap<String, RuntimeEntry>,
    /// Cached path of the runtime-state file (`running.json`), if resolvable.
    runtime_path: Option<PathBuf>,
    /// Whether the sidebar (project list) is collapsed to give the log area the
    /// full width. Session-only; not persisted.
    sidebar_collapsed: bool,
    /// The workspace dock's screen rect from the previous frame. The sidebar
    /// renders first each frame and needs it to tell a card drag released over
    /// the dock (a drop) apart from a reorder within the list.
    dock_rect: Option<egui::Rect>,
}

impl CampfireApp {
    fn new(ctx: egui::Context) -> Self {
        let servers = match store::load() {
            Ok(doc) => doc.servers,
            Err(err) => {
                eprintln!("campfire: could not load config, starting empty ({err})");
                Vec::new()
            }
        };

        // Reconcile processes a previous instance left running without getting to
        // run Drop (force-kill / crash). A server whose config still exists is
        // adopted as a recovered "running" card (stop/restart, but no live logs);
        // one whose config was deleted is simply stopped to free its port.
        let runtime_path = runtime_state::state_path();
        let mut running: HashMap<String, RunningProcess> = HashMap::new();
        let mut runtime: HashMap<String, RuntimeEntry> = HashMap::new();
        let mut notice = None;

        if let Some(path) = &runtime_path {
            let orphans = runtime_state::confirmed_orphans(&runtime_state::load_from(path));
            let mut stopped = 0usize;
            for entry in orphans {
                if servers.iter().any(|s| s.id == entry.server_id) {
                    let ctx = ctx.clone();
                    let proc = RunningProcess::adopt(&entry, move || ctx.request_repaint());
                    shutdown::register(entry.pid);
                    running.insert(entry.server_id.clone(), proc);
                    runtime.insert(entry.server_id.clone(), entry);
                } else {
                    // Config was deleted, so the user wants this gone: SIGKILL
                    // frees the port for certain (no grace is owed, and a trapped
                    // graceful signal would leak it) — and a dead PID needs no
                    // re-tracking.
                    kill_tree::tree_kill(entry.pid, kill_tree::Signal::Kill);
                    stopped += 1;
                }
            }
            // Persist the reconciled set: adopted entries kept, stopped ones gone.
            let entries: Vec<RuntimeEntry> = runtime.values().cloned().collect();
            let _ = runtime_state::save_to(path, &entries);
            notice = reconcile_notice(running.len(), stopped);
        }

        Self {
            servers,
            running,
            editor: None,
            pending_delete: None,
            workspaces: Workspaces::new(),
            notice,
            restart_pending: HashSet::new(),
            metrics: metrics::Metrics::new(),
            show_help: false,
            logo: None,
            runtime,
            runtime_path,
            sidebar_collapsed: false,
            dock_rect: None,
        }
    }

    fn start_server(&mut self, id: &str, ctx: egui::Context) {
        let Some(server) = self.servers.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        if let Some(port) = server.port
            && !port::is_port_free(port)
        {
            self.notice = Some(format!(
                "Can't start '{}': port {port} is already in use.",
                server.name
            ));
            return;
        }
        let wake = move || ctx.request_repaint();
        match RunningProcess::spawn(&server, wake) {
            Ok(proc) => {
                self.notice = None;
                self.track_running(&server, proc.pid());
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
                    proc.stop(STOP_GRACE);
                }
            }
            Action::Restart(id) => {
                let active = self.running.get(&id).is_some_and(|p| !p.is_terminal());
                if active {
                    if let Some(proc) = self.running.get_mut(&id) {
                        proc.stop(STOP_GRACE);
                    }
                    self.restart_pending.insert(id); // relaunched once terminated
                } else {
                    self.start_server(&id, ctx);
                }
            }
            Action::Duplicate(id) => {
                // Insert the copy without changing the selection, so a
                // right-click-duplicate leaves the current view in place.
                if let Some(src) = self.servers.iter().find(|s| s.id == id) {
                    let copy = src.duplicate();
                    self.servers.push(copy);
                    self.persist();
                }
            }
            Action::Delete(id) => self.pending_delete = Some(id),
            Action::ClearLogs(id) => {
                if let Some(proc) = self.running.get_mut(&id) {
                    proc.clear_logs();
                }
            }
            Action::OpenLog(id) => {
                if let Some(notice) = self.workspaces.active_mut().open_auto(&id) {
                    self.notice = Some(notice.to_owned());
                }
            }
            Action::FocusLog(id) => self.workspaces.active_mut().focus(&id),
            Action::Reorder { from, to } => self.reorder_servers(from, to),
            Action::OpenNew => self.editor = Some(EditorForm::new_server()),
            Action::OpenEdit(id) => {
                if let Some(server) = self.servers.iter().find(|s| s.id == id) {
                    self.editor = Some(EditorForm::from_config(server));
                }
            }
            Action::OpenHelp => self.show_help = true,
            Action::ToggleSidebar => self.sidebar_collapsed = !self.sidebar_collapsed,
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
                // Defer to the shared confirm dialog; close the editor first so
                // the confirmation isn't stacked on top of it.
                self.pending_delete = Some(id);
                self.editor = None;
            }
        }
    }

    /// Remove a server: drop its process (`Drop` force-kills the group), clear
    /// runtime + signal-registry tracking, remove it from config, deselect it
    /// if it was selected, and persist. Shared by the editor's Delete button
    /// and the sidebar context menu's Delete.
    fn delete_server(&mut self, id: &str) {
        self.running.remove(id); // dropped -> Drop force-kills the group
        self.untrack_running(id); // clear runtime + signal registry now
        self.servers.retain(|s| s.id != id);
        self.workspaces.close_server_everywhere(id); // drop its panes everywhere
        self.persist();
    }

    /// Render the delete-confirmation modal when a delete is pending, and apply
    /// the choice. The server is removed only on explicit confirm; click-away or
    /// Esc cancels. If the pending server vanished meanwhile, the state is
    /// silently dropped.
    fn render_delete_confirm(&mut self, ctx: &egui::Context) {
        let Some(id) = self.pending_delete.clone() else {
            return;
        };
        let Some(name) = self
            .servers
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone())
        else {
            self.pending_delete = None;
            return;
        };
        let response = egui::Modal::new(egui::Id::new("confirm_delete"))
            .frame(theme::modal_frame())
            .show(ctx, |ui| ui::confirm::show_delete(ui, &name));
        let outcome = if response.should_close() {
            ui::confirm::ConfirmOutcome::Cancel // click-away / Esc = cancel
        } else {
            response.inner
        };
        match outcome {
            ui::confirm::ConfirmOutcome::None => {}
            ui::confirm::ConfirmOutcome::Cancel => self.pending_delete = None,
            ui::confirm::ConfirmOutcome::Confirm => {
                self.delete_server(&id);
                self.pending_delete = None;
            }
        }
    }

    /// Apply a drag reorder and persist the new order. Persistence is the list's
    /// existing save path, so the on-disk `servers` order is the display order —
    /// no separate ordering field is needed. A no-op move skips the write.
    fn reorder_servers(&mut self, from: usize, to: usize) {
        if move_in_place(&mut self.servers, from, to) {
            self.persist();
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

    /// Record a freshly-spawned server in the runtime state (and persist it), so
    /// it can be recovered if this instance is force-killed. Skips recording if
    /// the process vanished before we could read its start time.
    fn track_running(&mut self, server: &ServerConfig, pid: u32) {
        let Some(start_time) = runtime_state::process_start_time(pid) else {
            // The process exited before we could read its start time (e.g. a
            // command that fails instantly). Nothing to recover; poll() will
            // still surface it as terminal in the UI.
            eprintln!(
                "campfire: '{}' exited before it could be tracked",
                server.name
            );
            return;
        };
        self.runtime.insert(
            server.id.clone(),
            RuntimeEntry {
                server_id: server.id.clone(),
                name: server.name.clone(),
                pid,
                start_time,
                port: server.port,
            },
        );
        shutdown::register(pid);
        self.persist_runtime();
    }

    /// Drop a server from the runtime state (and persist) once its process has
    /// ended — it is no longer an orphan to recover.
    fn untrack_running(&mut self, id: &str) {
        if let Some(entry) = self.runtime.remove(id) {
            shutdown::unregister(entry.pid);
            self.persist_runtime();
        }
    }

    /// Rewrite `running.json` from the current in-memory set. Best-effort: a
    /// failure only risks a stale reconcile next launch, so it is logged, not
    /// surfaced as a user-facing error.
    fn persist_runtime(&self) {
        let Some(path) = &self.runtime_path else {
            return;
        };
        let entries: Vec<RuntimeEntry> = self.runtime.values().cloned().collect();
        if let Err(err) = runtime_state::save_to(path, &entries) {
            eprintln!("campfire: could not persist runtime state ({err})");
        }
    }
}

/// Move the element at `from` to insertion index `to`, where `to` is the drop
/// position computed **before** removal (as the drag UI reports it), and adjust
/// for the earlier removal when moving down. Returns whether the order actually
/// changed — an out-of-range or in-place move leaves the vec untouched, so the
/// caller can skip persisting.
fn move_in_place<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= items.len() {
        return false;
    }
    // Dropping just above or just below yourself lands in the same slot; after
    // accounting for the removal shift, that is `insert == from` — a no-op.
    let insert = (if from < to { to.saturating_sub(1) } else { to }).min(items.len() - 1);
    if insert == from {
        return false;
    }
    let item = items.remove(from);
    items.insert(insert, item);
    true
}

/// One-line summary of what startup reconcile did, or `None` if nothing was
/// recovered or stopped. `recovered` orphans became running cards; `stopped`
/// ones (whose config was gone) were killed to free their ports.
fn reconcile_notice(recovered: usize, stopped: usize) -> Option<String> {
    match (recovered, stopped) {
        (0, 0) => None,
        (r, 0) => Some(format!(
            "Recovered {r} running server(s) from a previous session."
        )),
        (0, s) => Some(format!(
            "Stopped {s} orphaned server(s) from a previous session."
        )),
        (r, s) => Some(format!(
            "Recovered {r} running server(s) and stopped {s} orphan(s) from a previous session."
        )),
    }
}

impl eframe::App for CampfireApp {
    // eframe's default clear color is near-black; a panel resize drag can leave
    // a sliver of the surface uncovered for a frame, which then flashes black.
    // Clearing to the canvas color makes any such gap invisible.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::CANVAS_FILL.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.logo.is_none()
            && let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!(
                "../assets/images/logo-mark-64.png"
            ))
        {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            );
            self.logo = Some(
                ui.ctx()
                    .load_texture("logo", image, egui::TextureOptions::LINEAR),
            );
        }

        // Drive live processes: drain logs, detect exit, escalate shutdown.
        for proc in self.running.values_mut() {
            proc.poll();
        }

        // Keep the persisted runtime state accurate: drop entries whose process
        // has ended, so the orphan set recovered next launch stays correct.
        if !self.runtime.is_empty() {
            let ended: Vec<String> = self
                .runtime
                .keys()
                .filter(|id| self.running.get(*id).is_none_or(|p| p.is_terminal()))
                .cloned()
                .collect();
            for id in ended {
                self.untrack_running(&id);
            }
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

        // Every section is a white rounded block on the grey canvas: the panel
        // frames carry the canvas fill plus the outer margins (12 at the window
        // edge, 4 + 4 = 8 between blocks), and their divider lines are off.
        egui::Panel::top("top_bar")
            .frame(theme::canvas_frame(egui::Margin {
                left: 12,
                right: 12,
                top: 12,
                bottom: 4,
            }))
            .show_separator_line(false)
            .show(ui, |ui| {
                theme::block_frame()
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            if let Some(logo) = &self.logo {
                                ui.add(egui::Image::from_texture(logo).max_height(22.0));
                            }
                            ui.heading("Campfire");
                            let active = self.running.values().filter(|p| !p.is_terminal()).count();
                            ui.weak(format!("running {active}/{}", self.servers.len()));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(ui::icon_button(ui::icons::help()))
                                        .on_hover_text("Help")
                                        .clicked()
                                    {
                                        action = Some(Action::OpenHelp);
                                    }
                                    if let Some(notice) = &self.notice {
                                        let warn = ui.visuals().warn_fg_color;
                                        ui.colored_label(warn, notice);
                                    }
                                },
                            );
                        });
                    });
            });

        // Snapshot the active workspace's focus/open set into locals, so `View`
        // doesn't borrow `workspaces` — it must stay free for the dock's
        // mutable render below.
        let focused = self.workspaces.active().focused().map(str::to_owned);
        let open_logs = self.workspaces.active().open_ids();
        let view = ui::View {
            servers: &self.servers,
            running: &self.running,
            dup_ports: &dup_ports,
            focused: focused.as_deref(),
            open_logs: &open_logs,
            metrics: &self.metrics,
        };
        // Sidebar: `show_switched` cross-fades between the full card list and a
        // slim rail of per-server status dots, and drives both directions by
        // dragging the shared resize bar — squeeze it past the rail's width to
        // collapse, pull the rail's edge back out to expand. The rail's reopen
        // button is an additional way in, via `Action::ToggleSidebar`.
        let was_collapsed = self.sidebar_collapsed;
        let mut expanded = !was_collapsed;
        let margin = egui::Margin {
            left: 12,
            right: 4,
            top: 4,
            bottom: 12,
        };
        let collapsed_panel = egui::Panel::left("sidebar_rail")
            .resizable(true)
            .exact_size(SIDEBAR_RAIL_WIDTH)
            .frame(theme::canvas_frame(margin))
            .show_separator_line(false);
        let expanded_panel = egui::Panel::left("server_list")
            .resizable(true)
            .default_size(SIDEBAR_DEFAULT_WIDTH)
            .size_range(SIDEBAR_MIN_WIDTH..=420.0)
            .frame(theme::canvas_frame(margin))
            .show_separator_line(false);
        let mut drag = ui::SidebarDrag::default();
        let last_dock_rect = self.dock_rect;
        theme::with_accent_resize_indicator(ui, |ui| {
            // `show_switched` slides via the global `animation_time`; bump it just
            // for this call (restored right after) so the collapse/expand eases
            // smoothly, while hover and other feedback keep the snappy default.
            let ctx = ui.ctx().clone();
            let saved_anim = ctx.global_style().animation_time;
            ctx.all_styles_mut(|s| s.animation_time = SIDEBAR_SLIDE_TIME);
            egui::Panel::show_switched(
                ui,
                &mut expanded,
                collapsed_panel,
                expanded_panel,
                |ui, is_expanded| {
                    if is_expanded {
                        drag = ui::server_list::show(ui, &view, &mut action, last_dock_rect);
                    } else {
                        ui::server_list::rail(ui, &view, &mut action);
                    }
                },
            );
            ctx.all_styles_mut(|s| s.animation_time = saved_anim);
        });
        let central = egui::CentralPanel::default()
            .frame(theme::canvas_frame(egui::Margin {
                left: 4,
                right: 12,
                top: 4,
                bottom: 12,
            }))
            .show(ui, |ui| self.workspaces.show(ui, &view, &mut action, &drag));
        let (dock_rect, drop_notice) = central.inner;
        self.dock_rect = Some(dock_rect);
        if let Some(notice) = drop_notice {
            self.notice = Some(notice.to_owned());
        }

        if let Some(action) = action {
            self.apply_action(action, ui.ctx().clone());
        }
        // A resize-bar drag flips `expanded` directly; honor that over the
        // pre-frame state. A flip means `expanded` now equals the old collapsed
        // flag. The rail's reopen button instead flips state through
        // `Action::ToggleSidebar` above, and reaches here as a no-op (no flip).
        if expanded == was_collapsed {
            self.sidebar_collapsed = !expanded;
        }

        if self.editor.is_some() {
            let mut outcome = EditorOutcome::None;
            if let Some(form) = &mut self.editor {
                let response = egui::Modal::new(egui::Id::new("server_editor"))
                    .frame(theme::modal_frame())
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
            let response = egui::Modal::new(egui::Id::new("help"))
                .frame(theme::modal_frame())
                .show(ui.ctx(), ui::help::show);
            if response.should_close() || response.inner {
                self.show_help = false;
            }
        }

        self.render_delete_confirm(ui.ctx());
    }
}

#[cfg(test)]
mod tests {
    use super::move_in_place;

    #[test]
    fn move_down_lands_before_the_drop_target() {
        let mut v = vec![0, 1, 2, 3];
        assert!(move_in_place(&mut v, 0, 2));
        assert_eq!(v, vec![1, 0, 2, 3]);
    }

    #[test]
    fn move_to_end() {
        let mut v = vec![0, 1, 2, 3];
        assert!(move_in_place(&mut v, 0, 4));
        assert_eq!(v, vec![1, 2, 3, 0]);
    }

    #[test]
    fn move_up_to_front() {
        let mut v = vec![0, 1, 2, 3];
        assert!(move_in_place(&mut v, 3, 0));
        assert_eq!(v, vec![3, 0, 1, 2]);
    }

    #[test]
    fn dropping_onto_self_is_a_noop() {
        let mut v = vec![0, 1, 2, 3];
        assert!(!move_in_place(&mut v, 1, 1));
        assert_eq!(v, vec![0, 1, 2, 3]);
    }

    #[test]
    fn dropping_just_below_self_is_a_noop() {
        // `to = from + 1` is the slot right after the item — the same position.
        let mut v = vec![0, 1, 2, 3];
        assert!(!move_in_place(&mut v, 1, 2));
        assert_eq!(v, vec![0, 1, 2, 3]);
    }

    #[test]
    fn out_of_range_from_is_ignored() {
        let mut v = vec![0, 1, 2];
        assert!(!move_in_place(&mut v, 5, 0));
        assert_eq!(v, vec![0, 1, 2]);
    }
}
