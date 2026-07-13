//! The active workspace's dock: an egui_tiles tree whose panes are server logs.
//! Each pane is a white block with a header (status dot, name, status pill,
//! lifecycle buttons, close) over the shared log view. Dragging a pane's title
//! area rearranges panes (egui_tiles built-in preview + drop); the splits are
//! resizable. Pane close and focus mutate the workspace directly; process
//! operations go out through [`Action`]s.

use super::{Workspace, Workspaces};
use crate::model::ServerConfig;
use crate::process::log_buffer::LogBuffer;
use crate::process::running::Status;
use crate::theme;
use crate::ui::log_view::{self, LogView};
use crate::ui::{Action, View, icon_button, icons, status_dot, status_text};
use eframe::egui;
use egui_tiles::{Behavior, TileId, UiResponse};
use std::collections::HashMap;

pub(super) fn show_active(
    ui: &mut egui::Ui,
    wss: &mut Workspaces,
    view: &View,
    action: &mut Option<Action>,
) {
    let ws = &mut wss.list[wss.active];
    if ws.tree.is_empty() {
        theme::block_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(ui.available_height());
            ui.centered_and_justified(|ui| {
                ui.weak("좌측의 프로젝트 카드를 이곳으로 드래그하면 로그가 열립니다 (최대 4개)");
            });
        });
        return;
    }

    let Workspace {
        id,
        tree,
        views,
        focused,
        ..
    } = ws;
    let mut behavior = DockBehavior {
        view,
        ws_id: *id,
        views,
        focused,
        action,
        close: Vec::new(),
    };
    tree.ui(&mut behavior, ui);

    // Apply pane closes after the tree finished rendering (the behavior only
    // records them — mutating the tree mid-render would fight egui_tiles).
    let close = behavior.close;
    for tile in close {
        if let Some(egui_tiles::Tile::Pane(server)) = tree.tiles.get(tile) {
            views.remove(&server.clone());
        }
        tree.remove_recursively(tile);
    }
    ws.fix_focus_and_reset();
}

/// Per-frame render context for the tiles: the read-only app view, the mutable
/// per-server log-view state, focus, and collected requests.
struct DockBehavior<'a> {
    view: &'a View<'a>,
    ws_id: u64,
    views: &'a mut HashMap<String, LogView>,
    focused: &'a mut Option<String>,
    action: &'a mut Option<Action>,
    /// Panes whose × was clicked this frame; removed after `Tree::ui` returns.
    close: Vec<TileId>,
}

impl Behavior<String> for DockBehavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &String) -> egui::WidgetText {
        self.view
            .servers
            .iter()
            .find(|s| s.id == *pane)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| pane.clone())
            .into()
    }

    // Widen the default 1px gap so the canvas shows between panes, matching the
    // app's blocks-on-canvas layout.
    fn gap_width(&self, _style: &egui::Style) -> f32 {
        8.0
    }

    fn pane_ui(&mut self, ui: &mut egui::Ui, tile_id: TileId, pane: &mut String) -> UiResponse {
        let Some(server) = self.view.servers.iter().find(|s| s.id == *pane) else {
            // Server was deleted; drop its pane after this frame.
            self.close.push(tile_id);
            return UiResponse::None;
        };
        let proc = self.view.running.get(&server.id);
        let status = proc.map(|p| p.status().clone()).unwrap_or(Status::Stopped);
        let active = proc.is_some_and(|p| !p.is_terminal());
        let recovered = proc.is_some_and(|p| p.is_recovered());

        // The focused pane gets a softened accent border so the sidebar
        // highlight and the pane it refers to read as one, without shouting;
        // focus also owns the Cmd/Ctrl+F shortcut below.
        let is_focused = self.focused.as_deref() == Some(pane.as_str());
        let frame = if is_focused {
            theme::block_frame().stroke(egui::Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.55)))
        } else {
            theme::block_frame()
        };

        let mut drag_started = false;
        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(ui.available_height());
            let title = pane_header(
                ui,
                server,
                tile_id,
                &status,
                active,
                recovered,
                self.action,
                &mut self.close,
            );
            if title.drag_started() {
                drag_started = true;
            }
            if title.clicked() || title.drag_started() {
                *self.focused = Some(pane.clone());
            }
            ui.add_space(6.0);
            if recovered {
                ui.weak(
                    "Recovered from a previous session — live logs aren't available. \
                     Restart to stream logs.",
                );
                return;
            }
            let empty = LogBuffer::default();
            let logs = proc.map(|p| p.logs()).unwrap_or(&empty);
            let salt = egui::Id::new(("ws_log", self.ws_id, pane.as_str()));
            let view_state = self.views.entry(pane.clone()).or_default();
            if log_view::show(ui, salt, is_focused, view_state, logs) {
                *self.action = Some(Action::ClearLogs(pane.clone()));
            }
        });

        // A drag on the title area hands the pane to egui_tiles' built-in
        // rearrange (preview + drop).
        if drag_started {
            UiResponse::DragStarted
        } else {
            UiResponse::None
        }
    }
}

/// One pane's header, kept slim: a status dot (state in its tooltip) and the
/// truncating name on the left; start/stop/restart and × on the right. Only
/// the name truncates and nothing renders after it, so a narrow pane elides
/// the title instead of overlapping the buttons. The title strip doubles as
/// the pane's drag handle and focus click target — its response is returned.
#[allow(clippy::too_many_arguments)] // straight-line render inputs, same shape as log_view::render_body
fn pane_header(
    ui: &mut egui::Ui,
    server: &ServerConfig,
    tile_id: TileId,
    status: &Status,
    active: bool,
    recovered: bool,
    action: &mut Option<Action>,
    close: &mut Vec<TileId>,
) -> egui::Response {
    let mut title = None;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(icon_button(icons::close()))
                .on_hover_text("Close log")
                .clicked()
            {
                close.push(tile_id);
            }
            ui.add_space(2.0);
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
            let inner = ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let mut state = status_text(status);
                if recovered {
                    state.push_str(" · recovered from a previous session — restart for live logs");
                }
                status_dot(ui, status).on_hover_text(state);
                ui.add(
                    egui::Label::new(egui::RichText::new(&server.name).strong())
                        .truncate()
                        .selectable(false),
                );
            });
            // The title strip senses click (focus) and drag (rearrange).
            let response = ui
                .interact(
                    inner.response.rect,
                    egui::Id::new(("pane_title", tile_id)),
                    egui::Sense::click_and_drag(),
                )
                .on_hover_cursor(egui::CursorIcon::Grab);
            title = Some(response);
        });
    });
    title.expect("title strip was rendered above")
}
