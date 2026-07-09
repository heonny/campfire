//! The log view's toolbars: the (optional) find/grep search rows shown above the
//! lines, and the follow/clear/scroll control bar pinned to the bottom edge.

use super::{Events, LogView, ScrollTo, SearchInfo};
use crate::ui::{icon_button, icon_toggle_button, icons, text_input};
use eframe::egui;

/// Bad-regex marker color (the app's crash red).
const ERROR_RED: egui::Color32 = egui::Color32::from_rgb(0xC0, 0x39, 0x2B);

/// Shared width of the find and grep inputs, so the two toolbar rows line up.
const INPUT_WIDTH: f32 = 150.0;

/// Two rows — find on top, grep below — laid out identically: a leading icon, an
/// equal-width input, the shared case/word/regex toggles, and a status
/// indicator. find adds match navigation. `focus_find` requests keyboard focus
/// on the find field this frame (set when the box was just opened). Edits `state`
/// in place and records the nav clicks `show` acts on once it knows the match set.
pub(super) fn search_rows(
    ui: &mut egui::Ui,
    state: &mut LogView,
    info: SearchInfo,
    events: &mut Events,
    focus_find: bool,
) {
    // Find row: highlights matches in place and steps between them.
    ui.horizontal(|ui| {
        let input = labeled_input(ui, icons::search(), &mut state.find, "find in logs");
        if focus_find {
            input.request_focus();
        }
        search_toggles(
            ui,
            &mut state.find_case,
            &mut state.find_word,
            &mut state.find_regex,
        );
        find_indicator(
            ui,
            info.find_error,
            state.find.trim(),
            state.active,
            info.total,
        );
        find_nav(ui, info.total, events);
    });
    // Grep row: the same options, but filters the visible lines instead.
    ui.horizontal(|ui| {
        labeled_input(ui, icons::filter(), &mut state.grep, "filter lines");
        search_toggles(
            ui,
            &mut state.grep_case,
            &mut state.grep_word,
            &mut state.grep_regex,
        );
        grep_indicator(ui, info.grep_error, state.grep.trim(), info.grep_count);
    });
}

/// A leading field glyph (search / filter) followed by its input, with the glyph
/// vertically centered on the input box. The glyph's row slot is reserved first
/// (so it sits to the left) but painted after the input is laid out, at the
/// input's exact vertical center — otherwise, as the shorter leading item, egui
/// top-aligns the glyph and it rides visibly high next to the taller input.
/// Returns the input's response so the caller can request focus on it.
fn labeled_input(
    ui: &mut egui::Ui,
    icon: egui::Image<'_>,
    text: &mut String,
    hint: &str,
) -> egui::Response {
    let (slot, _) = ui.allocate_exact_size(egui::vec2(15.0, 1.0), egui::Sense::hover());
    let input = text_input(ui, text, hint, INPUT_WIDTH);
    let center = egui::pos2(slot.center().x, input.rect.center().y);
    icon.paint_at(
        ui,
        egui::Rect::from_center_size(center, egui::Vec2::splat(15.0)),
    );
    input
}

/// The shared case / whole-word / regex toggle trio (preceded by a faint
/// divider), so the find and grep rows read identically.
fn search_toggles(ui: &mut egui::Ui, case: &mut bool, word: &mut bool, regex: &mut bool) {
    dim_divider(ui);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        toggle_chip(ui, case, "Cc", "match case");
        toggle_chip(ui, word, "W", "whole word");
        toggle_chip(ui, regex, ".*", "regular expression");
    });
}

/// Find's match position (`n/total`) or a bad-regex marker, after the toggles.
fn find_indicator(
    ui: &mut egui::Ui,
    error: Option<&str>,
    query: &str,
    active: usize,
    total: usize,
) {
    if let Some(err) = error {
        ui.colored_label(ERROR_RED, "!")
            .on_hover_text(format!("invalid regex: {err}"));
    } else if !query.is_empty() {
        let current = if total == 0 { 0 } else { active + 1 };
        ui.weak(format!("{current}/{total}"));
    }
}

/// Grep's matched-line count or a bad-regex marker — the filter row's parallel
/// of the find match count.
fn grep_indicator(ui: &mut egui::Ui, error: Option<&str>, query: &str, lines: Option<usize>) {
    if let Some(err) = error {
        ui.colored_label(ERROR_RED, "!")
            .on_hover_text(format!("invalid regex: {err}"));
    } else if !query.is_empty() {
        ui.weak(format!("{} lines", lines.unwrap_or(0)));
    }
}

/// The previous/next find-match steppers (disabled when there are no matches).
fn find_nav(ui: &mut egui::Ui, total: usize, events: &mut Events) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if ui
            .add_enabled(total > 0, icon_button(icons::chevron_up()))
            .on_hover_text("previous match")
            .clicked()
        {
            events.nav = -1;
        }
        if ui
            .add_enabled(total > 0, icon_button(icons::chevron_down()))
            .on_hover_text("next match")
            .clicked()
        {
            events.nav = 1;
        }
    });
}

/// The bottom control bar, right-aligned: follow (a pressed-state icon toggle)
/// set apart from the one-shot clear and scroll-to-bottom/top. Laid out
/// right-to-left, so these read as scroll↑ scroll↓ clear · follow.
pub(super) fn bottom_bar(ui: &mut egui::Ui, state: &mut LogView, events: &mut Events) {
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
