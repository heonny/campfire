//! The log view's toolbars: the (optional) find/grep search rows shown above the
//! lines, and the follow/clear/scroll control bar pinned to the bottom edge.

use super::{Events, LogView, ScrollTo, SearchInfo};
use crate::ui::{frameless_edit, icon_button, icon_toggle_button, icons, text_input_frame};
use eframe::egui;

/// Bad-regex marker color (the app's crash red).
const ERROR_RED: egui::Color32 = egui::Color32::from_rgb(0xC0, 0x39, 0x2B);

/// What a search field shows at its trailing edge: the live result count, or
/// why the query is invalid.
enum FieldStatus<'a> {
    None,
    Count(String),
    Error(&'a str),
}

/// Two rows — find on top, grep below — in the shape of a browser find bar.
/// Each row: a leading glyph, an input that takes all the width the trailing
/// controls leave (its result count sits inside the box, at the right edge,
/// where the eye already is), then the compact case/word/regex toggles. The
/// find row adds ↑ ↓ match steppers; Enter / Shift+Enter step too. No × here:
/// the pane's own × sits right above it, and two stacked closes misread — the
/// box closes with Esc, Cmd/Ctrl+F, or the bottom bar's search toggle. The
/// grep row pads its trailing edge so both inputs and both toggle groups
/// share the same x. `focus_find` requests keyboard focus on the find field
/// this frame (set when the box was just opened). Edits `state` in place and
/// records the nav clicks `show` acts on once it knows the match set.
pub(super) fn search_rows(
    ui: &mut egui::Ui,
    state: &mut LogView,
    info: SearchInfo,
    events: &mut Events,
    focus_find: bool,
) {
    ui.spacing_mut().item_spacing.y = 4.0;

    // Find row, laid out right-to-left so the input can fill what's left.
    let mut toggles_right = 0.0;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            find_nav(ui, info.total, events);
            toggles_right = search_toggles(
                ui,
                &mut state.find_case,
                &mut state.find_word,
                &mut state.find_regex,
            )
            .right();
            let status = match info.find_error {
                Some(err) => FieldStatus::Error(err),
                None if state.find.trim().is_empty() => FieldStatus::None,
                None => {
                    let current = if info.total == 0 { 0 } else { state.active + 1 };
                    FieldStatus::Count(format!("{current}/{}", info.total))
                }
            };
            let input = search_field(ui, icons::search(), &mut state.find, "Find", status);
            if focus_find {
                input.request_focus();
            }
            // Enter steps to the next match, Shift+Enter to the previous; the
            // field keeps focus (a single-line edit would otherwise drop it).
            if input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                events.nav = if ui.input(|i| i.modifiers.shift) {
                    -1
                } else {
                    1
                };
                input.request_focus();
            }
        });
    });

    // Grep row: same shape, no steppers — padded so the toggles line up.
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let pad = ui.cursor().right() - toggles_right;
            if pad > 0.0 {
                ui.add_space(pad);
            }
            search_toggles(
                ui,
                &mut state.grep_case,
                &mut state.grep_word,
                &mut state.grep_regex,
            );
            let status = match info.grep_error {
                Some(err) => FieldStatus::Error(err),
                None if state.grep.trim().is_empty() => FieldStatus::None,
                None => FieldStatus::Count(format!("{} lines", info.grep_count.unwrap_or(0))),
            };
            search_field(ui, icons::filter(), &mut state.grep, "Filter lines", status);
        });
    });
}

/// A leading glyph and a bordered input filling the remaining row width, with
/// `status` (count or error) drawn inside the box at its trailing edge. Called
/// from a right-to-left row, so "remaining" is what the trailing controls left.
/// The glyph is painted after layout at the box's exact vertical center — as
/// the shorter item egui would otherwise top-align it. Returns the edit's
/// response so the caller can manage focus.
fn search_field(
    ui: &mut egui::Ui,
    icon: egui::Image<'_>,
    text: &mut String,
    hint: &str,
    status: FieldStatus<'_>,
) -> egui::Response {
    const GLYPH: f32 = 15.0;
    let is_error = matches!(status, FieldStatus::Error(_));
    // Box width: the row's remainder minus the glyph slot and its gap.
    let box_width = (ui.available_width() - GLYPH - ui.spacing().item_spacing.x).max(60.0);
    let mut edit = None;
    let frame = text_input_frame(is_error);
    // The frame adds its margin AND its stroke around the content (egui 0.35
    // `total_margin`); sizing the content by the margin alone left the box 2px
    // too wide, so each row overflowed the pane by 2px and pushed the pane's
    // own frame past the clip (its left border vanished).
    let content_width = box_width - frame.total_margin().sum().x;
    let frame = frame.show(ui, |ui| {
        ui.set_width(content_width);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            match status {
                FieldStatus::None => {}
                FieldStatus::Count(count) => {
                    ui.label(egui::RichText::new(count).small().weak());
                }
                FieldStatus::Error(err) => {
                    ui.label(
                        egui::RichText::new("invalid regex")
                            .small()
                            .color(ERROR_RED),
                    )
                    .on_hover_text(err);
                }
            }
            edit = Some(ui.add(frameless_edit(text, hint, ui.available_width())));
        });
    });
    let (slot, _) = ui.allocate_exact_size(egui::vec2(GLYPH, 1.0), egui::Sense::hover());
    let center = egui::pos2(slot.center().x, frame.response.rect.center().y);
    icon.paint_at(
        ui,
        egui::Rect::from_center_size(center, egui::Vec2::splat(GLYPH)),
    );
    edit.expect("edit rendered inside the frame")
}

/// The case / whole-word / regex toggles as one compact segment (browser
/// find-bar labels: `Aa`, `W`, `.*`). Returns the group's rect so the grep row
/// can align its own group to it.
fn search_toggles(
    ui: &mut egui::Ui,
    case: &mut bool,
    word: &mut bool,
    regex: &mut bool,
) -> egui::Rect {
    // One hairline box around the three: chromeless chips alone read as plain
    // text; the box says "a group of options".
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, crate::theme::CARD_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
            // Laid out right-to-left by the row, so list them reversed.
            toggle_chip(ui, regex, ".*", "regular expression");
            toggle_chip(ui, word, "W", "whole word");
            toggle_chip(ui, case, "Aa", "match case");
        })
        .response
        .rect
}

/// The previous/next find-match steppers (disabled when there are no matches).
/// Right-to-left row: next is laid out first so it ends up on the right.
fn find_nav(ui: &mut egui::Ui, total: usize, events: &mut Events) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if ui
            .add_enabled(total > 0, icon_button(icons::chevron_down()))
            .on_hover_text("next match (Enter)")
            .clicked()
        {
            events.nav = 1;
        }
        if ui
            .add_enabled(total > 0, icon_button(icons::chevron_up()))
            .on_hover_text("previous match (Shift+Enter)")
            .clicked()
        {
            events.nav = -1;
        }
    });
}

/// The bottom control bar, right-aligned: follow (a pressed-state icon toggle)
/// set apart from the one-shot clear and scroll-to-bottom/top, then the search
/// toggle (the visible way into Cmd/Ctrl+F) and wrap. Laid out right-to-left,
/// so these read as [new] · search · wrap scroll↑ scroll↓ clear · follow.
pub(super) fn bottom_bar(ui: &mut egui::Ui, state: &mut LogView, unseen: u64, events: &mut Events) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui
            .add(icon_toggle_button(icons::follow(), state.follow))
            .on_hover_text("follow new output")
            .clicked()
        {
            state.follow = !state.follow;
        }
        dim_divider(ui);
        if ui
            .add(icon_button(icons::clear()))
            .on_hover_text("clear log")
            .clicked()
        {
            events.clear = true;
        }
        if ui
            .add(icon_button(icons::scroll_bottom()))
            .on_hover_text("scroll to bottom")
            .clicked()
        {
            state.scroll_to = Some(ScrollTo::Bottom);
        }
        if ui
            .add(icon_button(icons::scroll_top()))
            .on_hover_text("scroll to top")
            .clicked()
        {
            state.scroll_to = Some(ScrollTo::Top);
        }
        if ui
            .add(icon_toggle_button(icons::wrap(), state.wrap))
            .on_hover_text("wrap long lines")
            .clicked()
        {
            state.wrap = !state.wrap;
        }
        dim_divider(ui);
        if ui
            .add(icon_toggle_button(icons::search(), state.search_open))
            .on_hover_text("find / filter (Cmd/Ctrl+F)")
            .clicked()
        {
            events.toggle_search = true;
        }
        // Output arrived while not following: an accent chip that jumps to the
        // bottom and resumes tailing.
        if unseen > 0 {
            dim_divider(ui);
            let chip = egui::Button::new(
                egui::RichText::new(format!("↓ {unseen} new"))
                    .small()
                    .color(crate::theme::ACCENT),
            )
            .fill(crate::theme::ACCENT_TINT);
            if ui
                .add(chip)
                .on_hover_text("scroll to bottom and follow")
                .clicked()
            {
                events.catch_up = true;
            }
        }
    });
}

/// A small on/off text toggle (case / word / regex).
fn toggle_chip(ui: &mut egui::Ui, on: &mut bool, label: &str, tip: &str) {
    if ui.selectable_label(*on, label).on_hover_text(tip).clicked() {
        *on = !*on;
    }
}

/// A full-width hairline under the search box, in the card-border grey so it
/// reads as part of the surface system rather than a heavy rule. The 1px line is
/// centered in a slightly taller slot, which carries the breathing room above
/// and below (plus the layout's own item spacing).
pub(super) fn row_divider(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 11.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0, crate::theme::CARD_BORDER),
    );
}

/// A short, faint vertical divider between toolbar groups — lighter and shorter
/// than `ui.separator()` (which draws a full-height bar that reads as heavy).
fn dim_divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(11.0, 16.0), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        (rect.center().y - 7.0)..=(rect.center().y + 7.0),
        egui::Stroke::new(1.0, egui::Color32::from_gray(0xCF)),
    );
}
