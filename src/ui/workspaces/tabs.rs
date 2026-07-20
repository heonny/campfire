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
/// Chip metrics: horizontal text padding, the × hit box, and its gap.
const CHIP_PAD_X: f32 = 10.0;
const CLOSE_SIZE: f32 = 16.0;
const CLOSE_GAP: f32 = 6.0;

/// One workspace chip, drawn by hand so BOTH states share the exact same text
/// pipeline: one rect of [`TAB_HEIGHT`], one galley, manually centered.
/// Composing different widgets (Button vs Frame+Label) gave each its own
/// vertical text placement, so the grey and white tab titles never sat on one
/// baseline.
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
    let selected = index == wss.active;

    let font = egui::TextStyle::Body.resolve(ui.style());
    let color = if selected {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let galley = ui.painter().layout_no_wrap(title, font, color);

    let close_extra = if selected { CLOSE_GAP + CLOSE_SIZE } else { 0.0 };
    let width = CHIP_PAD_X + galley.size().x + close_extra + CHIP_PAD_X;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, TAB_HEIGHT), egui::Sense::click());

    // Background: white card for the active tab, hover fill for inactive ones.
    let painter = ui.painter();
    if selected {
        painter.rect(
            rect,
            egui::CornerRadius::same(6),
            egui::Color32::WHITE,
            egui::Stroke::new(1.0, theme::CARD_BORDER),
            egui::StrokeKind::Inside,
        );
    } else if response.hovered() {
        painter.rect_filled(rect, egui::CornerRadius::same(6), theme::BUTTON_HOVER_FILL);
    }

    // The title, vertically centered by its real galley height.
    let text_pos = egui::pos2(
        rect.left() + CHIP_PAD_X,
        rect.center().y - galley.size().y / 2.0,
    );
    painter.galley(text_pos, galley, color);

    // The active tab's ×: its own interact rect inside the chip (registered
    // after the chip response, so it wins the pointer over that area).
    if selected {
        let close_rect = egui::Rect::from_center_size(
            egui::pos2(rect.right() - CHIP_PAD_X - CLOSE_SIZE / 2.0, rect.center().y),
            egui::vec2(CLOSE_SIZE, CLOSE_SIZE),
        );
        let close_response = ui
            .interact(
                close_rect,
                response.id.with("close"),
                egui::Sense::click(),
            )
            .on_hover_text("Close workspace");
        if close_response.hovered() {
            ui.painter().rect_filled(
                close_rect.expand(2.0),
                egui::CornerRadius::same(4),
                theme::BUTTON_HOVER_FILL,
            );
        }
        icons::close().paint_at(ui, close_rect);
        if close_response.clicked() {
            *close = Some(index);
            return;
        }
    }

    if response.middle_clicked() {
        *close = Some(index);
    } else if response.double_clicked() {
        *start_rename = Some(index);
    } else if response.clicked() && !selected {
        *select = Some(index);
    }
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
