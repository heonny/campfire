//! The workspace tab strip: one chip per workspace (click = switch,
//! double-click = rename inline, × = close), plus a trailing add button.
//! Sits directly on the canvas above the dock, scrolling horizontally when the
//! chips overflow.

use super::{MAX_WORKSPACES, Workspaces};
use crate::theme;
use crate::ui::{icon_button, icons};
use eframe::egui;

pub(super) fn strip(ui: &mut egui::Ui, wss: &mut Workspaces) {
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
                for index in 0..wss.list.len() {
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
                                &mut select,
                                &mut close,
                                &mut start_rename,
                            );
                        }
                    }
                }
                let can_add = wss.list.len() < MAX_WORKSPACES;
                if ui
                    .add_enabled(can_add, icon_button(icons::add()))
                    .on_hover_text(if can_add {
                        "New workspace"
                    } else {
                        "Workspace limit reached (100)"
                    })
                    .clicked()
                {
                    wss.add();
                }
            });
        });

    if commit_rename
        && let Some((index, buffer)) = wss.renaming.take()
    {
        let name = buffer.trim();
        if !name.is_empty()
            && let Some(ws) = wss.list.get_mut(index)
            && ws.name != name
        {
            ws.name = name.to_owned();
            wss.dirty = true;
        }
    }
    if let Some(index) = start_rename {
        wss.renaming = Some((index, wss.list[index].name.clone()));
    }
    if let Some(index) = select
        && index != wss.active
    {
        wss.active = index;
        // Loaded (non-active) workspaces start with no focus; give the newly
        // active one a sensible focused pane so click-to-swap and the log
        // search shortcut have a target.
        wss.list[index].fix_focus_and_reset();
        wss.dirty = true;
    }
    if let Some(index) = close {
        wss.renaming = None; // indices shift; drop any in-progress rename
        wss.close(index);
    }
}

/// One workspace chip, quiet like a modern tab strip. The active tab is a
/// white card with the usual hairline and its × close; inactive tabs are
/// chromeless buttons (weak text, the standard hover fill) that switch on
/// click. Double-click renames either.
fn chip(
    ui: &mut egui::Ui,
    index: usize,
    wss: &Workspaces,
    select: &mut Option<usize>,
    close: &mut Option<usize>,
    start_rename: &mut Option<usize>,
) {
    let ws = &wss.list[index];
    if index != wss.active {
        let button = egui::Button::new(egui::RichText::new(&ws.name).weak())
            .frame_when_inactive(false);
        let response = ui.add(button);
        if response.double_clicked() {
            *start_rename = Some(index);
        } else if response.clicked() {
            *select = Some(index);
        }
        return;
    }
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, theme::CARD_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let label = ui.add(
                egui::Label::new(egui::RichText::new(&ws.name).strong())
                    .selectable(false)
                    .sense(egui::Sense::click()),
            );
            if label.double_clicked() {
                *start_rename = Some(index);
            }
            if wss.list.len() > 1
                && ui
                    .add(icon_button(icons::close()))
                    .on_hover_text("Close workspace")
                    .clicked()
            {
                *close = Some(index);
            }
        });
}

/// The inline rename editor. Returns `true` when the edit should be committed
/// (Enter or focus loss); Escape reverts by committing the untouched original.
fn rename_field(ui: &mut egui::Ui, buffer: &mut String) -> bool {
    let response = ui.add(
        egui::TextEdit::singleline(buffer)
            .desired_width(120.0)
            .font(egui::TextStyle::Body),
    );
    response.request_focus();
    response.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter))
}
