//! The left panel: an Add button and the drag-reorderable list of servers. Each
//! server is a clickable card with a status dot, name, port, a duplicate-port
//! marker, and live CPU/memory while running. Reordering is handled by egui_dnd
//! (the dragged card floats to the cursor and the rest slide aside, animated); a
//! left click selects, a right click opens the context menu.

use super::{Action, SidebarDrag, View, icon_button, icons, status_dot, status_dot_fill};
use crate::model::ServerConfig;
use crate::process::running::Status;
use crate::theme;
use eframe::egui;
use egui_dnd::{DragDropItem, Handle, ItemState, dnd};

/// egui_dnd tracks each draggable by a stable [`egui::Id`]; key it off the
/// server's own id so it survives reordering. `&ServerConfig` is not `Hash`, so
/// it does NOT match egui_dnd's blanket `DragDropItem for T: AsId` (AsId = Hash +
/// Debug) — this manual impl is the one that applies, with no overlap.
impl DragDropItem for &ServerConfig {
    fn id(&self) -> egui::Id {
        egui::Id::new(("server_card", self.id.as_str()))
    }
}

/// Render the sidebar. `dock_rect` is the workspace dock's rect from the LAST
/// frame (the sidebar renders first): a card drag released inside it is a
/// drop-into-dock, so the reorder that egui_dnd still reports (it always snaps
/// to the closest list slot, however far the pointer is) must be swallowed.
/// Returns the in-flight card drag for the dock's drop preview.
pub fn show(
    ui: &mut egui::Ui,
    view: &View,
    action: &mut Option<Action>,
    dock_rect: Option<egui::Rect>,
) -> SidebarDrag {
    let mut drag = SidebarDrag::default();
    theme::block_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.heading("Projects");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(icon_button(icons::add()))
                    .on_hover_text("Add project")
                    .clicked()
                {
                    *action = Some(Action::OpenNew);
                }
            });
        });
        ui.add_space(8.0);

        // The scroll area fills the remaining height, which also stretches the
        // block to the bottom of the panel.
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if view.servers.is_empty() {
                    ui.weak("등록된 프로젝트가 없습니다");
                    return;
                }
                // egui_dnd renders each card, animates the reorder, and reports
                // the final move. We forward that as an Action so the app stays
                // the sole mutator — its `from`/`to` match `move_in_place` (egui_
                // dnd's `shift_vec` has the same semantics), so no translation.
                let response = dnd(ui, "server_reorder").show(
                    view.servers.iter(),
                    |ui, server, handle, state| {
                        render_card(ui, view, server, handle, state, action, &mut drag.server);
                    },
                );
                drag.finished = response.is_drag_finished();
                let over_dock = dock_rect
                    .zip(ui.ctx().pointer_hover_pos())
                    .is_some_and(|(rect, pos)| rect.contains(pos));
                if let Some(update) = response.final_update()
                    && !over_dock
                {
                    *action = Some(Action::Reorder {
                        from: update.from,
                        to: update.to,
                    });
                }
            });
    });
    drag
}

/// Render one server card inside its drag handle. The whole card is the handle
/// (with a click sense, so a short press still selects — egui_dnd only starts a
/// drag past a small move threshold); a right click opens the context menu.
fn render_card(
    ui: &mut egui::Ui,
    view: &View,
    server: &ServerConfig,
    handle: Handle<'_>,
    state: ItemState,
    action: &mut Option<Action>,
    dragging: &mut Option<String>,
) {
    // Report the in-flight drag so the dock can preview/accept a drop.
    if state.dragged {
        *dragging = Some(server.id.clone());
    }
    let running = view.running.get(&server.id);
    let active = running.is_some_and(|p| !p.is_terminal());
    let status = running.map(|p| p.status().clone()).unwrap_or(Status::Stopped);
    // Guard on `active`: cached metrics linger up to a refresh interval after a
    // server stops.
    let metrics = if active {
        view.metrics.get(&server.id)
    } else {
        None
    };
    let dup = server.port.is_some_and(|p| view.dup_ports.contains(&p));
    let focused = view.focused == Some(server.id.as_str());
    let open = view.open_logs.iter().any(|s| s == &server.id);

    // Open/focused read through a slim accent bar on the card's left edge —
    // focused additionally gets a whisper of tint — instead of loud accent
    // borders. Borders stay the neutral hairline everywhere.
    let fill = if focused {
        theme::ACCENT_TINT
    } else {
        theme::CARD_FILL
    };

    let response = handle.sense(egui::Sense::click()).ui(ui, |ui| {
        theme::card_frame()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, theme::CARD_BORDER))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    status_dot(ui, &status);
                    // Lay the right-aligned items out first, then give the name
                    // the remaining space, truncated — so a narrow sidebar elides
                    // the name instead of drawing it under the port.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if dup {
                            let warn = ui.visuals().warn_fg_color;
                            ui.colored_label(warn, "⚠").on_hover_text("duplicate port");
                        }
                        if let Some(port) = server.port {
                            ui.weak(format!(":{port}"));
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(&server.name).truncate());
                        });
                    });
                });
                metrics_row(ui, metrics);
            });
    });

    // The open/focused marker: a small rounded accent bar hugging the left
    // edge, inside the border radius.
    if open || focused {
        let rect = response.rect;
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 1.5, rect.top() + 8.0),
            egui::pos2(rect.left() + 4.5, rect.bottom() - 8.0),
        );
        ui.painter()
            .rect_filled(bar, egui::CornerRadius::same(2), theme::ACCENT);
    }

    card_context_menu(&response, server, active, action);
    // A drag ends as a release, not a click, so this fires only on a genuine
    // short click. Clicking shows the log in place — focus if open, else swap
    // into the focused pane (or open the first); splitting is the drag gesture.
    if response.clicked() {
        *action = Some(Action::ShowLog(server.id.clone()));
    }
    ui.add_space(6.0);
}

/// The right-click menu: lifecycle actions (Start, or Stop/Restart while
/// running), then management (Duplicate, Edit), then Delete — set apart and
/// error-colored, like the editor's Delete button, as the one destructive item.
/// egui already styles context menus full-width and flat at rest, so the only
/// styling here is roomier padding and a min width, with groups set apart by a
/// gap rather than a divider line (matching the app's spacing-over-separators
/// layout).
///
/// Labels are English to match the app's other action labels (the detail panel's
/// Start/Stop/Restart/Edit); localization is a later, app-wide pass.
fn card_context_menu(
    response: &egui::Response,
    server: &ServerConfig,
    active: bool,
    action: &mut Option<Action>,
) {
    response.context_menu(|ui| {
        ui.set_min_width(150.0);
        ui.spacing_mut().button_padding = egui::vec2(8.0, 5.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        let id = &server.id;
        if active {
            if ui.button("Stop").clicked() {
                *action = Some(Action::Stop(id.clone()));
                ui.close();
            }
            if ui.button("Restart").clicked() {
                *action = Some(Action::Restart(id.clone()));
                ui.close();
            }
        } else if ui.button("Start").clicked() {
            *action = Some(Action::Start(id.clone()));
            ui.close();
        }
        ui.add_space(4.0);
        // The non-drag way to open a log pane (auto-placed) in the workspace.
        if ui.button("Open log").clicked() {
            *action = Some(Action::OpenLog(id.clone()));
            ui.close();
        }
        if ui.button("Duplicate").clicked() {
            *action = Some(Action::Duplicate(id.clone()));
            ui.close();
        }
        if ui.button("Edit").clicked() {
            *action = Some(Action::OpenEdit(id.clone()));
            ui.close();
        }
        ui.add_space(4.0);
        let delete =
            egui::Button::new(egui::RichText::new("Delete").color(ui.visuals().error_fg_color));
        if ui.add(delete).clicked() {
            *action = Some(Action::Delete(id.clone()));
            ui.close();
        }
    });
}

/// The collapsed sidebar rail: a reopen button, then one clickable status dot
/// per server (color = state), so servers can be switched without expanding the
/// sidebar. Requested as "the server list as icons under the sidebar button".
pub fn rail(ui: &mut egui::Ui, view: &View, action: &mut Option<Action>) {
    ui.vertical_centered(|ui| {
        if ui
            .add(icon_button(icons::sidebar()))
            .on_hover_text("Show sidebar")
            .clicked()
        {
            *action = Some(Action::ToggleSidebar);
        }
        ui.add_space(10.0);
        for server in view.servers {
            let status = view
                .running
                .get(&server.id)
                .map(|p| p.status().clone())
                .unwrap_or(Status::Stopped);
            let focused = view.focused == Some(server.id.as_str());
            if rail_dot(ui, &status, focused)
                .on_hover_text(&server.name)
                .clicked()
            {
                *action = Some(Action::ShowLog(server.id.clone()));
            }
            ui.add_space(8.0);
        }
    });
}

/// One clickable server dot for the rail: a filled circle in the status color on
/// a rounded-square backing when selected or hovered — matching the log
/// toolbar's icon buttons, with no accent tint on the dot itself.
fn rail_dot(ui: &mut egui::Ui, status: &Status, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    if selected || response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(6), theme::BUTTON_HOVER_FILL);
    }
    ui.painter()
        .circle_filled(rect.center(), 5.0, status_dot_fill(status));
    response
}

/// CPU/memory under the name while running. The row's space is reserved even
/// when idle so card heights don't change as servers start and stop.
fn metrics_row(ui: &mut egui::Ui, metrics: Option<(f32, u64)>) {
    ui.horizontal(|ui| {
        ui.add_space(20.0); // status dot (12) + item gap (8): align with the name
        match metrics {
            Some((cpu, mem)) => {
                let mem_mb = mem as f64 / 1_048_576.0;
                ui.label(
                    egui::RichText::new(format!("CPU {cpu:.0}% · {mem_mb:.0} MB"))
                        .small()
                        .weak(),
                );
            }
            None => {
                let height = ui.text_style_height(&egui::TextStyle::Small);
                ui.allocate_exact_size(egui::vec2(1.0, height), egui::Sense::hover());
            }
        }
    });
}
