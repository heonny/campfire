//! The left panel: an Add button and the drag-reorderable list of servers. Each
//! server is a clickable card with a status dot, name, port, a duplicate-port
//! marker, and live CPU/memory while running. Cards stay stationary during a
//! drag; a line marks the insertion point for sidebar reordering. A
//! left click selects, a right click opens the context menu.

use super::{
    Action, SidebarDrag, View, icon_button, icons, port_link, status_dot, status_dot_fill,
    status_text,
};
use crate::model::ServerConfig;
use crate::process::running::Status;
use crate::theme;
use eframe::egui;

/// Render the sidebar. `dock_rect` is the workspace dock's rect from the LAST
/// frame (the sidebar renders first). Only releases inside the sidebar can
/// reorder projects; the dock handles its own drops.
/// Returns the in-flight card drag for the dock's drop preview.
pub fn show(
    ui: &mut egui::Ui,
    view: &View,
    action: &mut Option<Action>,
    dock_rect: Option<egui::Rect>,
) -> SidebarDrag {
    let mut drag = SidebarDrag::default();
    // The block's right inner margin is moved inside the scroll area (as
    // content_margin): the cards keep their width, but the floating scroll bar
    // now rides in that empty gutter instead of on top of the cards.
    let gutter = 12.0;
    theme::block_frame()
        .inner_margin(egui::Margin {
            left: 12,
            right: 0,
            top: 12,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Header: title + live count on the left; add / collapse / help on
            // the right (the app has no top bar — the window title names it).
            // Trailing items first (right-to-left), then the title takes the
            // remainder and truncates — so a narrow sidebar elides the title
            // instead of drawing the buttons over it.
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    // Re-inset the button by the margin the frame no longer has.
                    ui.add_space(gutter);
                    if ui
                        .add(icon_button(icons::add()))
                        .on_hover_text("Add project")
                        .clicked()
                    {
                        *action = Some(Action::OpenNew);
                    }
                    // Collapse lives with the list it collapses (the rail has
                    // the matching expand button).
                    if ui
                        .add(icon_button(icons::sidebar()))
                        .on_hover_text("Hide sidebar (Cmd/Ctrl+B)")
                        .clicked()
                    {
                        *action = Some(Action::ToggleSidebar);
                    }
                    if ui
                        .add(icon_button(icons::help()))
                        .on_hover_text("Help")
                        .clicked()
                    {
                        *action = Some(Action::OpenHelp);
                    }
                    ui.add_space(4.0);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let count = format!("{}/{}", view.active, view.servers.len());
                        let title =
                            egui::Label::new(egui::RichText::new("Projects").heading()).truncate();
                        ui.add(title);
                        ui.add(egui::Label::new(egui::RichText::new(count).weak()).truncate())
                            .on_hover_text(format!(
                                "{} of {} running",
                                view.active,
                                view.servers.len()
                            ));
                    });
                });
            });
            ui.add_space(8.0);

            // Slim the bar and pad it off the block edge so both its dormant
            // (2px) and hovered (6px) widths sit centered in the gutter.
            let scroll = &mut ui.spacing_mut().scroll;
            scroll.bar_width = 6.0;
            scroll.bar_outer_margin = 4.0;

            // The scroll area fills the remaining height, which also stretches
            // the block to the bottom of the panel.
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .content_margin(egui::Margin {
                    right: gutter as i8,
                    ..egui::Margin::ZERO
                })
                .show(ui, |ui| {
                    if view.servers.is_empty() {
                        ui.weak("No projects yet — press + to add one.");
                        return;
                    }
                    let mut rows = Vec::with_capacity(view.servers.len());
                    for server in view.servers {
                        let response = ui
                            .push_id(&server.id, |ui| render_card(ui, view, server, action))
                            .inner;
                        rows.push(response.rect);
                        if response.dragged() || response.drag_stopped() {
                            drag.server = Some(server.id.clone());
                            drag.finished = response.drag_stopped();
                        }
                    }
                    if let Some(server) = &drag.server
                        && let Some(pos) = ui.ctx().pointer_hover_pos()
                        && ui.clip_rect().contains(pos)
                        && !dock_rect.is_some_and(|rect| rect.contains(pos))
                    {
                        let from = view.servers.iter().position(|s| s.id == *server).unwrap();
                        let slot = rows.iter().take_while(|r| pos.y > r.center().y).count();
                        let to = if slot > from { slot - 1 } else { slot };
                        if from != to {
                            let y = if slot == rows.len() {
                                rows.last().unwrap().bottom() + 3.0
                            } else {
                                rows[slot].top() - 3.0
                            };
                            ui.painter().hline(
                                rows[0].x_range(),
                                y,
                                egui::Stroke::new(2.0, theme::ACCENT),
                            );
                            if drag.finished {
                                *action = Some(Action::Reorder { from, to });
                            }
                        }
                    }
                });
        });
    drag
}

/// Stable card geometry keeps drag ownership independent of its destination.
fn render_card(
    ui: &mut egui::Ui,
    view: &View,
    server: &ServerConfig,
    action: &mut Option<Action>,
) -> egui::Response {
    let running = view.running.get(&server.id);
    let active = running.is_some_and(|p| !p.is_terminal());
    let status = running
        .map(|p| p.status().clone())
        .unwrap_or(Status::Stopped);
    // Guard on `active`: cached metrics linger up to a refresh interval after a
    // server stops.
    let metrics = if active {
        view.metrics.get(&server.id)
    } else {
        None
    };
    let dup = server.port.is_some_and(|p| view.dup_ports.contains(&p));
    let focused = view.focused == Some(server.id.as_str());

    // The focused card reads through a whisper of accent tint alone — no
    // stripes or accent borders; the hairline stays neutral everywhere. An
    // open-but-unfocused pane is shown by its tab/pane, not marked here.
    let fill = if focused {
        theme::ACCENT_TINT
    } else {
        theme::CARD_FILL
    };

    let response = ui
        .scope(|ui| {
            ui.style_mut().interaction.selectable_labels = false;
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
                                port_link(ui, port);
                            }
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add(egui::Label::new(&server.name).truncate());
                                },
                            );
                        });
                    });
                    metrics_row(ui, metrics, &status, &server.command);
                })
                .response
        })
        .inner;
    let response = ui
        .interact(
            response.rect,
            ui.id().with("card_drag"),
            egui::Sense::click_and_drag(),
        )
        .on_hover_cursor(egui::CursorIcon::Grab);

    card_context_menu(&response, server, active, action);
    // A drag ends as a release, not a click, so this fires only on a genuine
    // short click. Clicking shows the log in place — focus if open, else swap
    // into the focused pane (or open the first); splitting is the drag gesture.
    if response.clicked() {
        *action = Some(Action::ShowLog(server.id.clone()));
    }
    ui.add_space(6.0);
    response
}

/// The right-click menu: lifecycle actions (Start, or Stop/Restart while
/// running), OS hand-offs (browser, folder, clipboard), then management
/// (Open log, Duplicate, Edit), then Delete — set apart and
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
        if let Some(port) = server.port
            && ui.button("Open in browser").clicked()
        {
            crate::system::open_url(&crate::system::localhost_url(port));
            ui.close();
        }
        if ui.button("Reveal working dir").clicked() {
            crate::system::reveal_dir(&server.cwd);
            ui.close();
        }
        if ui.button("Copy command").clicked() {
            ui.ctx().copy_text(server.command.clone());
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
            if rail_chip(ui, &server.name, &status, focused)
                .on_hover_text(&server.name)
                .clicked()
            {
                *action = Some(Action::ShowLog(server.id.clone()));
            }
            ui.add_space(8.0);
        }
    });
}

/// One clickable project chip for the rail: the name's monogram on a
/// rounded-square backing (filled when selected or hovered, like the icon
/// buttons), with a small status dot at the bottom-right corner. A bare dot
/// per project left nine identical greys with nothing to tell them apart.
fn rail_chip(ui: &mut egui::Ui, name: &str, status: &Status, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    let painter = ui.painter();
    if selected || response.hovered() {
        painter.rect_filled(rect, egui::CornerRadius::same(6), theme::BUTTON_HOVER_FILL);
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        monogram(name),
        egui::TextStyle::Small.resolve(ui.style()),
        ui.visuals().text_color(),
    );
    // Status dot in the corner, ringed in the backing color so it stays
    // crisp over the letter.
    let dot = rect.right_bottom() + egui::vec2(-6.0, -6.0);
    painter.circle_filled(dot, 4.0, egui::Color32::WHITE);
    painter.circle_filled(dot, 3.0, status_dot_fill(status));
    response
}

/// A two-letter monogram: the first letter of the first word and of the last
/// word (`admin-web` → AW, `admin-api clean` → AC, `producer` → P). Names in a
/// list tend to share a prefix, so a single initial told nine chips apart as
/// "A A A A A A P P P".
fn monogram(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| c == '-' || c == '_' || c == '.' || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .collect();
    let first = words.first().and_then(|w| w.chars().next());
    let last = words.last().and_then(|w| w.chars().next());
    match (first, last) {
        (Some(a), Some(b)) if words.len() > 1 => format!("{a}{b}"),
        (Some(a), _) => a.to_string(),
        _ => String::new(),
    }
    .to_uppercase()
}

/// The card's second line: CPU/memory while running, the exit code after a
/// crash, otherwise the command — so the row is never blank and card heights
/// stay put as servers start and stop.
fn metrics_row(ui: &mut egui::Ui, metrics: Option<(f32, u64)>, status: &Status, command: &str) {
    ui.horizontal(|ui| {
        ui.add_space(20.0); // status dot (12) + item gap (8): align with the name
        let text = match (metrics, status) {
            (Some((cpu, mem)), _) => {
                let mem_mb = mem as f64 / 1_048_576.0;
                egui::RichText::new(format!("CPU {cpu:.0}% · {mem_mb:.0} MB")).weak()
            }
            (None, Status::Crashed { .. }) => {
                egui::RichText::new(status_text(status)).color(ui.visuals().error_fg_color)
            }
            // The command is information, not decoration: small but not weak.
            (None, _) => egui::RichText::new(command).small(),
        };
        ui.add(egui::Label::new(text.small()).truncate());
    });
}

#[cfg(test)]
mod tests {
    use super::monogram;

    #[test]
    fn monogram_takes_first_and_last_word_initials() {
        assert_eq!(monogram("admin-web"), "AW");
        assert_eq!(monogram("admin-api clean"), "AC");
        assert_eq!(monogram("producer"), "P");
        assert_eq!(monogram("my_service.v2"), "MV");
        assert_eq!(monogram("  "), "");
    }
}

#[cfg(test)]
#[path = "server_list_drag_tests.rs"]
mod drag_tests;
