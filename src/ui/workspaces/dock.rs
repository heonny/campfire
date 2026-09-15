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
use crate::ui::{Action, View, icon_button, icons, port_link, status_dot, status_text};
use eframe::egui;
use egui_tiles::{Behavior, ResizeState, TileId, UiResponse};
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
                ui.weak("Select a project to view its log. Use Open log or drag to compare up to 4 logs.");
            });
        });
        return;
    }

    // Focus only needs marking when there is a choice: with a single pane the
    // accent border would just be noise.
    let multi = ws.open_ids().len() > 1;
    let compact = ws.uses_compact_view(ui.available_size());
    if compact {
        ui.horizontal_wrapped(|ui| {
            for id in ws.open_ids() {
                let name = view
                    .servers
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.name.as_str())
                    .unwrap_or(&id);
                if ui
                    .selectable_label(ws.focused() == Some(&id), name)
                    .clicked()
                {
                    ws.focus(&id);
                }
            }
        });
        ui.weak("Compact view · expand the window to compare logs");
    }
    let compact_pane = if compact {
        ws.focused()
            .and_then(|id| ws.find_pane(id).map(|tile| (tile, id.to_owned())))
    } else {
        None
    };
    let Workspace {
        id,
        tree,
        views,
        focused,
        automatic_layout,
        ..
    } = ws;
    let mut behavior = DockBehavior {
        view,
        ws_id: *id,
        views,
        focused,
        action,
        close: Vec::new(),
        multi,
        automatic_layout,
    };
    if let Some((tile, mut server)) = compact_pane {
        let _ = behavior.pane_ui(ui, tile, &mut server);
    } else {
        tree.ui(&mut behavior, ui);
    }

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
    /// Two or more panes are open, so the focused one is worth marking.
    multi: bool,
    automatic_layout: &'a mut bool,
}

impl Behavior<String> for DockBehavior<'_> {
    fn on_edit(&mut self, edit: egui_tiles::EditAction) {
        if matches!(
            edit,
            egui_tiles::EditAction::TileDropped | egui_tiles::EditAction::TileResized
        ) {
            *self.automatic_layout = false;
        }
    }
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
    // app's blocks-on-canvas layout (6 keeps it snug — logs want the space).
    fn gap_width(&self, _style: &egui::Style) -> f32 {
        6.0
    }

    // egui_tiles fills the idle gap with a gap-wide darkened band by default;
    // sections here separate by surface contrast, not lines, so the gap stays
    // bare canvas and the handle only shows as a slim accent line on
    // hover/drag — same language as the sidebar's resize indicator.
    fn resize_stroke(&self, _style: &egui::Style, resize_state: ResizeState) -> egui::Stroke {
        match resize_state {
            ResizeState::Idle => egui::Stroke::NONE,
            ResizeState::Hovering => egui::Stroke::new(1.0, theme::ACCENT),
            ResizeState::Dragging => egui::Stroke::new(1.5, theme::ACCENT),
        }
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
        let uptime = proc
            .filter(|p| !p.is_terminal())
            .and_then(|p| p.started_at().elapsed().ok());

        // With several panes, the focused one wears a softened accent border
        // so the sidebar highlight and the pane it refers to read as one;
        // focus also owns the Cmd/Ctrl+F shortcut below.
        let is_focused = self.focused.as_deref() == Some(pane.as_str());
        let frame = if is_focused && self.multi {
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
                uptime,
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
            if logs.is_empty() && !active {
                ui.add_space(12.0);
                ui.label(if matches!(status, Status::Crashed { .. }) {
                    "Project exited. Run it again to collect new output."
                } else {
                    "Not running. Use Run to start this command."
                });
                ui.add(egui::Label::new(egui::RichText::new(&server.command).monospace()).wrap());
                return;
            }
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

/// Compact uptime: `<1m`, `12m`, `3h 05m`, `2d 4h` — never seconds, which
/// would tick visibly in the header.
fn format_uptime(up: std::time::Duration) -> String {
    let s = up.as_secs();
    match s {
        0..60 => "<1m".to_owned(),
        60..3600 => format!("{}m", s / 60),
        3600..86_400 => format!("{}h {:02}m", s / 3600, (s % 3600) / 60),
        _ => format!("{}d {}h", s / 86_400, (s % 86_400) / 3600),
    }
}

/// Keep process controls on a separate row so the project name retains its width.
#[allow(clippy::too_many_arguments)]
fn pane_header(
    ui: &mut egui::Ui,
    server: &ServerConfig,
    tile_id: TileId,
    status: &Status,
    active: bool,
    recovered: bool,
    uptime: Option<std::time::Duration>,
    action: &mut Option<Action>,
    close: &mut Vec<TileId>,
) -> egui::Response {
    let title = ui
        .horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button = ui
                    .add(icon_button(icons::close()))
                    .on_hover_text("Close this log view");
                button.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("Close {} log view", server.name),
                    )
                });
                if button.clicked() {
                    close.push(tile_id);
                }
                ui.menu_button("More", |ui| {
                    if active && ui.button("Restart project").clicked() {
                        *action = Some(Action::Restart(server.id.clone()));
                        ui.close();
                    }
                    if ui.button("Edit project…").clicked() {
                        *action = Some(Action::OpenEdit(server.id.clone()));
                        ui.close();
                    }
                    if let Some(up) = uptime {
                        ui.weak(format!("Uptime: {}", format_uptime(up)));
                    }
                    if recovered {
                        ui.weak("Recovered session");
                    }
                });
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(&server.name).strong())
                            .truncate()
                            .selectable(false)
                            .sense(egui::Sense::click_and_drag()),
                    )
                    .on_hover_text(&server.name)
                    .on_hover_cursor(egui::CursorIcon::Grab)
                })
                .inner
            })
            .inner
        })
        .inner;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if matches!(status, Status::Stopping) {
                "Force stop"
            } else if active {
                "Stop"
            } else {
                "Run"
            };
            if ui
                .button(label)
                .on_hover_text(format!("{label} {}", server.name))
                .clicked()
            {
                *action = Some(if active {
                    Action::Stop(server.id.clone())
                } else {
                    Action::Start(server.id.clone())
                });
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                status_dot(ui, status).on_hover_text(status_text(status));
                let label = match status {
                    Status::Stopped => "Stopped",
                    Status::Starting => "Starting",
                    Status::Running => "Running",
                    Status::Stopping => "Stopping",
                    Status::Crashed { .. } => "Crashed",
                };
                ui.label(egui::RichText::new(label).small());
                if ui.available_width() > 60.0
                    && let Some(port) = server.port
                {
                    port_link(ui, port);
                }
            });
        });
    });
    title
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn four_logs_render_in_two_rows_and_survive_compact_view() {
        let ctx = egui::Context::default();
        crate::theme::setup(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut wss = Workspaces::new();
        let servers: Vec<_> = (0..4)
            .map(|i| {
                let mut server =
                    ServerConfig::from_preset("Project", ".", crate::model::Preset::Custom);
                server.id = format!("project-{i}");
                server.name = format!("Project {i}");
                wss.active_mut().open_auto(&server.id);
                server
            })
            .collect();
        let metrics = crate::metrics::Metrics::new();
        let view = View {
            active: 0,
            servers: &servers,
            running: &HashMap::new(),
            dup_ports: &Default::default(),
            focused: None,
            metrics: &metrics,
        };
        for width in [760.0, 440.0, 760.0] {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 560.0));
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| {
                    show_active(ui, &mut wss, &view, &mut None);
                },
            );
            assert_eq!(wss.active().open_ids().len(), 4);
            assert_eq!(wss.active().focused(), Some("project-3"));
            if width > 640.0 {
                let ws = wss.active();
                let rects: Vec<_> = ws
                    .pane_tiles()
                    .iter()
                    .map(|(id, _)| ws.tree.tiles.rect(*id).unwrap())
                    .collect();
                assert!(
                    rects
                        .iter()
                        .all(|r| r.width() > 300.0 && r.height() > 200.0)
                );
                assert!(rects[0].right() <= rects[1].left());
                assert!(rects[0].bottom() <= rects[2].top());
                assert!(rects[2].right() <= rects[3].left());
            }
        }
    }

    #[test]
    fn uptime_picks_the_coarsest_useful_unit() {
        assert_eq!(format_uptime(Duration::from_secs(42)), "<1m");
        assert_eq!(format_uptime(Duration::from_secs(750)), "12m");
        assert_eq!(format_uptime(Duration::from_secs(3 * 3600 + 300)), "3h 05m");
        assert_eq!(
            format_uptime(Duration::from_secs(2 * 86_400 + 4 * 3600)),
            "2d 4h"
        );
    }
}
