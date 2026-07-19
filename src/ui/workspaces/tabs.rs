//! The workspace tab strip: one chip per workspace (click = switch,
//! double-click = rename inline, middle-click / × = close), plus a trailing
//! add button. Sits directly on the canvas above the dock, scrolling
//! horizontally when the chips overflow.
//!
//! Every strip item — active chip, inactive tab buttons, the rename editor,
//! the + button — is laid out at the same fixed [`TAB_HEIGHT`] inside an
//! explicit centered row. Mixed intrinsic heights in an implicit layout are
//! what made labels and buttons sit crooked against each other.

use super::{MAX_WORKSPACES, Workspace, Workspaces};
use crate::theme;
use crate::ui::{View, icon_button, icons};
use eframe::egui;

/// One height for everything in the strip, so baselines line up across the
/// active chip, inactive tabs, the rename editor, and the add button. Must be
/// at least a Button's natural height (text + 2×button_padding.y ≈ 27): egui
/// centers row items against a running height estimate, so items only align
/// with each other when they are exactly the same size.
const TAB_HEIGHT: f32 = 28.0;

/// A tab holding a single log wears that server's name; only tabs bundling two
/// or more (or empty ones) go by their workspace name.
fn tab_title(ws: &Workspace, view: &View) -> String {
    let open = ws.open_ids();
    if let [only] = open.as_slice()
        && let Some(server) = view.servers.iter().find(|s| s.id == *only)
    {
        return server.name.clone();
    }
    ws.name.clone()
}

pub(super) fn strip(ui: &mut egui::Ui, wss: &mut Workspaces, view: &View) {
    // Deferred mutations: applied after the loop so indices stay stable.
    let mut select: Option<usize> = None;
    let mut close: Option<usize> = None;
    let mut start_rename: Option<usize> = None;
    let mut commit_rename = false;

    egui::ScrollArea::horizontal()
        .id_salt("workspace_tabs")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.set_min_height(TAB_HEIGHT);
                // Once content names the tab, the reveal flag has done its job;
                // clearing it re-hides the tab if the workspace empties again.
                if !wss.active().open_ids().is_empty() {
                    wss.solo_revealed = false;
                }
                // A lone empty workspace shows no tab at all — just the + —
                // unless + explicitly revealed it.
                let hide = wss.solo_hidden();
                for index in 0..wss.list.len() {
                    if hide {
                        break;
                    }
                    match &mut wss.renaming {
                        Some((i, buffer)) if *i == index => {
                            if rename_field(ui, buffer) {
                                commit_rename = true;
                            }
                        }
                        _ => {
                            chip(
                                ui,
                                index,
                                wss,
                                view,
                                &mut select,
                                &mut close,
                                &mut start_rename,
                            );
                        }
                    }
                }
                let can_add = wss.list.len() < MAX_WORKSPACES;
                let add = icon_button(icons::add()).min_size(egui::vec2(TAB_HEIGHT, TAB_HEIGHT));
                if ui
                    .add_enabled(can_add, add)
                    .on_hover_text(if can_add {
                        "New workspace"
                    } else {
                        "Workspace limit reached (100)"
                    })
                    .clicked()
                {
                    wss.add_or_reveal();
                }
            });
        });

    if commit_rename
        && let Some((index, buffer)) = wss.renaming.take()
    {
        let name = buffer.trim();
        if !name.is_empty()
            && let Some(ws) = wss.list.get_mut(index)
        {
            ws.name = name.to_owned();
        }
    }
    if let Some(index) = start_rename {
        wss.renaming = Some((index, wss.list[index].name.clone()));
    }
    if let Some(index) = select {
        wss.switch_to(index);
    }
    if let Some(index) = close {
        wss.renaming = None; // indices shift; drop any in-progress rename
        wss.close(index);
    }
}

/// One workspace chip, quiet like a modern tab strip. The active tab is a
/// white card with the usual hairline and its × close; inactive tabs are
/// chromeless buttons (weak text, the standard hover fill) that switch on
/// click. Double-click renames, middle-click closes — closing the last
/// workspace just leaves a fresh empty one.
#[allow(clippy::too_many_arguments)] // straight-line render inputs
fn chip(
    ui: &mut egui::Ui,
    index: usize,
    wss: &Workspaces,
    view: &View,
    select: &mut Option<usize>,
    close: &mut Option<usize>,
    start_rename: &mut Option<usize>,
) {
    let ws = &wss.list[index];
    let title = tab_title(ws, view);
    if index != wss.active {
        let button = egui::Button::new(egui::RichText::new(&title).weak())
            .frame_when_inactive(false)
            .min_size(egui::vec2(0.0, TAB_HEIGHT));
        let response = ui.add(button);
        if response.middle_clicked() {
            *close = Some(index);
        } else if response.double_clicked() {
            *start_rename = Some(index);
        } else if response.clicked() {
            *select = Some(index);
        }
        return;
    }
    // Vertical size comes from the fixed-height row inside; the frame only
    // adds the horizontal padding, so the chip matches the buttons exactly.
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, theme::CARD_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(TAB_HEIGHT);
                ui.spacing_mut().item_spacing.x = 6.0;
                // Compact padding for the × so it stays a small square target.
                ui.spacing_mut().button_padding = egui::vec2(2.0, 2.0);
                let label = ui.add(
                    egui::Label::new(egui::RichText::new(&title).strong())
                        .selectable(false)
                        .sense(egui::Sense::click()),
                );
                if label.middle_clicked() {
                    *close = Some(index);
                } else if label.double_clicked() {
                    *start_rename = Some(index);
                }
                if ui
                    .add(icon_button(icons::close()))
                    .on_hover_text("Close workspace")
                    .clicked()
                {
                    *close = Some(index);
                }
            });
        });
}

/// The inline rename editor, centered at the shared strip height. Returns
/// `true` when the edit should be committed (Enter or focus loss); Escape
/// reverts by committing the untouched original.
fn rename_field(ui: &mut egui::Ui, buffer: &mut String) -> bool {
    let (_, rect) = ui.allocate_space(egui::vec2(130.0, TAB_HEIGHT));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let response = child.add(
        egui::TextEdit::singleline(buffer)
            .desired_width(120.0)
            .font(egui::TextStyle::Body),
    );
    response.request_focus();
    response.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter))
}
