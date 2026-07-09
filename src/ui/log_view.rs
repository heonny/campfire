//! The log viewer: an (optional) find/grep box over a virtualized tailing list
//! with ANSI colors, match highlighting, selectable lines, and viewport controls
//! pinned to the bottom edge.
//!
//! The find/grep box is hidden by default and toggled with Cmd/Ctrl+F (Escape
//! closes it); while closed the full buffer is shown and the box keeps its query
//! text for next time. Two search modes, kept deliberately separate, each on its
//! own row with the same case / whole-word / regex toggles:
//! - **find** — highlights every match in place and steps between them (↑↓)
//!   without hiding anything.
//! - **grep** — a filter that keeps only matching lines (active whenever its
//!   input is non-empty). find then works within whatever grep leaves visible.
//!
//! Follow / clear / scroll live in a bar along the bottom edge, shown whether or
//! not the search box is open.
//!
//! Matching the whole buffer is O(lines), so the filter + match set are cached
//! and only recomputed when the query, a toggle, or the line count changes —
//! not every frame.

use super::{icon_button, icon_toggle_button, icons, text_input};
use crate::process::log_buffer::LogBuffer;
use crate::search::Matcher;
use eframe::egui;
use std::ops::Range;

/// Bad-regex marker color (the app's crash red).
const ERROR_RED: egui::Color32 = egui::Color32::from_rgb(0xC0, 0x39, 0x2B);

/// Shared width of the find and grep inputs, so the two toolbar rows line up.
const INPUT_WIDTH: f32 = 150.0;

/// A one-shot scroll request applied on the next frame.
enum ScrollTo {
    Top,
    Bottom,
    Match(usize), // display row holding the focused find match
}

/// The inputs the cached match set depends on. When this is unchanged frame to
/// frame, the (whole-buffer) filter and match scan are skipped.
struct CacheKey {
    find: String,
    case: bool,
    word: bool,
    regex: bool,
    grep: String,
    grep_case: bool,
    grep_word: bool,
    grep_regex: bool,
    len: usize,
}

/// Everything derived from a [`CacheKey`]: the compiled find matcher (kept for
/// per-row highlight ranges), any regex error, the grep row set, and the flat
/// list of find matches.
#[derive(Default)]
struct Cache {
    key: Option<CacheKey>,
    find: Option<Matcher>,
    find_error: Option<String>,
    grep_error: Option<String>,
    filter: Option<Vec<usize>>, // None = no grep (all lines shown)
    matches: Vec<FindMatch>,
}

/// View state for the log pane.
pub struct LogView {
    /// Whether the find/grep box is shown. Hidden by default; toggled with
    /// Cmd/Ctrl+F. While hidden the search is inactive (the full buffer shows).
    search_open: bool,
    find: String,
    find_case: bool,
    find_word: bool,
    find_regex: bool,
    active: usize, // focused match among all find matches (clamped to range each frame)
    grep: String,
    grep_case: bool,
    grep_word: bool,
    grep_regex: bool,
    follow: bool,
    scroll_to: Option<ScrollTo>,
    cache: Cache,
}

impl Default for LogView {
    fn default() -> Self {
        Self {
            search_open: false,
            find: String::new(),
            find_case: false,
            find_word: false,
            find_regex: false,
            active: 0,
            grep: String::new(),
            grep_case: false,
            grep_word: false,
            grep_regex: false,
            follow: true,
            scroll_to: None,
            cache: Cache::default(),
        }
    }
}

impl LogView {
    pub fn new() -> Self {
        Self::default()
    }
}

/// A find match: the display row it sits on and its byte range in that line's
/// ANSI-stripped text.
struct FindMatch {
    row: usize,
    range: Range<usize>,
}

/// The lines currently shown — all of them, or the grep-filtered subset. A view
/// (it borrows the cached filter) so scanning and rendering share one path
/// without materializing `0..len` when there is no filter.
enum Displayed<'a> {
    All(usize),
    Filtered(&'a [usize]),
}

impl Displayed<'_> {
    fn len(&self) -> usize {
        match self {
            Displayed::All(n) => *n,
            Displayed::Filtered(rows) => rows.len(),
        }
    }

    /// The log-buffer index shown at display `row`, if any.
    fn line_index(&self, row: usize) -> Option<usize> {
        match self {
            Displayed::All(n) => (row < *n).then_some(row),
            Displayed::Filtered(rows) => rows.get(row).copied(),
        }
    }
}

/// Render the (optional) find/grep box, the virtualized log lines, and the
/// bottom control bar. Returns `true` if the user clicked "clear".
pub fn show(ui: &mut egui::Ui, state: &mut LogView, logs: &LogBuffer) -> bool {
    // Cmd/Ctrl+F toggles the find/grep box (focusing it on open); Escape closes it.
    let open_key = ui.input_mut(|i| {
        i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::F,
        ))
    });
    // Escape is read but deliberately *not* consumed (unlike the shortcut above):
    // the editor/help modals render after this panel and close on the same key,
    // so swallowing it here would trap them open. A stray double-handle (closing
    // the box behind a modal) is harmless; stealing Escape from a modal is not.
    let escape_key = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let (search_open, focus_find) = next_search_state(state.search_open, open_key, escape_key);
    state.search_open = search_open;

    // The filter + match set only apply while the box is open; a closed box shows
    // the full, unfiltered buffer but keeps its query text for the next opening.
    if state.search_open {
        ensure_cache(state, logs);
        let total = state.cache.matches.len();
        state.active = if total == 0 {
            0
        } else {
            state.active.min(total - 1)
        };
    }

    let mut events = Events::default();

    // Viewport controls (follow / clear / scroll) live in a bar pinned to the
    // bottom edge, shown whether or not the search box is open. Rendered first so
    // it reserves its space; the body then fills whatever remains above it.
    egui::Panel::bottom(egui::Id::new("log_controls"))
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::new().inner_margin(egui::Margin {
            left: 0,
            right: 0,
            top: 6,
            bottom: 0,
        }))
        .show(ui, |ui| bottom_bar(ui, state, &mut events));

    // The find/grep rows (when open) sit above the lines; both render into the
    // space the bottom bar left.
    if state.search_open {
        let find_error = state.cache.find_error.clone();
        let grep_error = state.cache.grep_error.clone();
        let info = SearchInfo {
            find_error: find_error.as_deref(),
            grep_error: grep_error.as_deref(),
            total: state.cache.matches.len(),
            grep_count: state.cache.filter.as_ref().map(|rows| rows.len()),
        };
        search_rows(ui, state, info, &mut events, focus_find);
        let total = state.cache.matches.len();
        if events.nav != 0 && total > 0 {
            state.active = (state.active as i32 + events.nav).rem_euclid(total as i32) as usize;
            state.follow = false; // stop tailing so the match stays in view
            state.scroll_to = Some(ScrollTo::Match(state.cache.matches[state.active].row));
        }
        row_divider(ui); // soft hairline setting the search box off from the lines
    }

    // Pull the render inputs out before borrowing the cache immutably. When the
    // box is closed the search is inactive: no highlight matcher, no filter.
    let scroll_to = state.scroll_to.take();
    let follow = state.follow;
    let (displayed, find, active) = if state.search_open {
        let displayed = match state.cache.filter.as_deref() {
            Some(rows) => Displayed::Filtered(rows),
            None => Displayed::All(logs.len()),
        };
        (
            displayed,
            state.cache.find.as_ref(),
            state.cache.matches.get(state.active),
        )
    } else {
        (Displayed::All(logs.len()), None, None)
    };
    render_body(ui, logs, &displayed, find, active, follow, scroll_to);
    events.clear
}

/// Next find/grep-box state from the current state and the two key events.
/// Cmd/Ctrl+F toggles the box — opening it requests focus, closing it does not;
/// Escape closes an open box. Returns the new open flag and whether the find
/// field should take focus this frame.
fn next_search_state(is_open: bool, open_key: bool, escape_key: bool) -> (bool, bool) {
    if open_key {
        // Toggle; focus the find field only on the frame it opens.
        let now_open = !is_open;
        (now_open, now_open)
    } else if is_open && escape_key {
        (false, false)
    } else {
        (is_open, false)
    }
}

/// Rebuild the filter + match cache when the query/toggles/line-count change;
/// otherwise reuse last frame's. A content change (anything but new lines
/// arriving) restarts find navigation at the first match.
fn ensure_cache(state: &mut LogView, logs: &LogBuffer) {
    let len = logs.len();
    if cache_is_current(&state.cache.key, state, len) {
        return;
    }
    let content_changed = state.cache.key.as_ref().is_none_or(|k| {
        k.find != state.find.trim()
            || k.case != state.find_case
            || k.word != state.find_word
            || k.regex != state.find_regex
            || k.grep != state.grep.trim()
            || k.grep_case != state.grep_case
            || k.grep_word != state.grep_word
            || k.grep_regex != state.grep_regex
    });
    if content_changed {
        state.active = 0;
    }

    let (find, find_error) = compile(
        state.find.trim(),
        state.find_case,
        state.find_word,
        state.find_regex,
    );
    // grep shares the same toggles as find; it just filters instead of stepping.
    let (grep, grep_error) = compile(
        state.grep.trim(),
        state.grep_case,
        state.grep_word,
        state.grep_regex,
    );

    let filter = grep.as_ref().map(|m| {
        (0..len)
            .filter(|&i| {
                logs.get(i)
                    .is_some_and(|l| m.is_match(&crate::ansi::strip(&l.text)))
            })
            .collect::<Vec<_>>()
    });
    let displayed = match filter.as_deref() {
        Some(rows) => Displayed::Filtered(rows),
        None => Displayed::All(len),
    };
    let matches = collect_matches(logs, &displayed, find.as_ref());

    state.cache = Cache {
        key: Some(CacheKey {
            find: state.find.trim().to_string(),
            case: state.find_case,
            word: state.find_word,
            regex: state.find_regex,
            grep: state.grep.trim().to_string(),
            grep_case: state.grep_case,
            grep_word: state.grep_word,
            grep_regex: state.grep_regex,
            len,
        }),
        find,
        find_error,
        grep_error,
        filter,
        matches,
    };
}

/// Whether the cache key still matches the live state (no allocation).
fn cache_is_current(key: &Option<CacheKey>, state: &LogView, len: usize) -> bool {
    matches!(key, Some(k)
        if k.find == state.find.trim()
        && k.case == state.find_case
        && k.word == state.find_word
        && k.regex == state.find_regex
        && k.grep == state.grep.trim()
        && k.grep_case == state.grep_case
        && k.grep_word == state.grep_word
        && k.grep_regex == state.grep_regex
        && k.len == len)
}

/// Compile `query` under the toggles, split into the matcher (if the query is
/// non-empty and valid) and an error message (if `regex` is on and the pattern
/// is bad). Shared by find and grep so both handle a bad pattern the same way.
fn compile(query: &str, case: bool, word: bool, regex: bool) -> (Option<Matcher>, Option<String>) {
    match Matcher::new(query, case, word, regex) {
        Ok(matcher) => (matcher, None),
        Err(err) => (None, Some(err)),
    }
}

/// Scan the displayed lines for every find match (empty when find is inactive).
fn collect_matches(
    logs: &LogBuffer,
    displayed: &Displayed<'_>,
    find: Option<&Matcher>,
) -> Vec<FindMatch> {
    let mut matches = Vec::new();
    let Some(matcher) = find else { return matches };
    for row in 0..displayed.len() {
        let Some(index) = displayed.line_index(row) else {
            continue;
        };
        let Some(line) = logs.get(index) else {
            continue;
        };
        let stripped = crate::ansi::strip(&line.text);
        for range in matcher.find_ranges(&stripped) {
            matches.push(FindMatch { row, range });
        }
    }
    matches
}

/// Click events the search rows and bottom bar report back to `show`.
#[derive(Default)]
struct Events {
    clear: bool,
    nav: i32, // -1 previous match, +1 next match, 0 none
}

/// The cache-derived values the search rows only display (match/line counts and
/// any bad-regex messages), gathered in `show` before the mutable row borrow.
#[derive(Clone, Copy)]
struct SearchInfo<'a> {
    find_error: Option<&'a str>,
    grep_error: Option<&'a str>,
    total: usize,
    grep_count: Option<usize>,
}

/// Two rows — find on top, grep below — laid out identically: a leading icon, an
/// equal-width input, the shared case/word/regex toggles, and a status
/// indicator. find adds match navigation. `focus_find` requests keyboard focus
/// on the find field this frame (set when the box was just opened). Edits `state`
/// in place and records the nav clicks `show` acts on once it knows the match set.
fn search_rows(
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
fn bottom_bar(ui: &mut egui::Ui, state: &mut LogView, events: &mut Events) {
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
fn row_divider(ui: &mut egui::Ui) {
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

/// The scrollable line list. Lines **wrap** (no sideways scroll), so rows have
/// variable height — which rules out egui's fixed-height `show_rows`. Plain
/// `show` lays every line out at its real height instead, so `stick_to_bottom`
/// follows the true content bottom and no longer oscillates ("trembles") the way
/// it did when wrapped rows broke `show_rows`' uniform-height assumption.
///
/// The trade-off: every displayed line is built each frame (egui caches the
/// galleys, so a steady tail stays cheap). If a very large scrollback ever makes
/// this heavy, swap in a variable-height virtual list.
fn render_body(
    ui: &mut egui::Ui,
    logs: &LogBuffer,
    displayed: &Displayed<'_>,
    find: Option<&Matcher>,
    active: Option<&FindMatch>,
    follow: bool,
    scroll_to: Option<ScrollTo>,
) {
    // Tight line spacing for a terminal-like density (the app default is roomier).
    ui.spacing_mut().item_spacing.y = 2.0;
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let base = ui.visuals().text_color();

    if logs.is_empty() {
        ui.weak("no output yet");
        return;
    }
    if displayed.len() == 0 {
        ui.weak("no matching lines");
        return;
    }

    // Top/Bottom jump to an edge; a find-nav target is scrolled into view as its
    // row renders; otherwise tail the bottom while following.
    let (offset, stick, match_row) = match scroll_to {
        Some(ScrollTo::Top) => (Some(0.0), false, None),
        Some(ScrollTo::Bottom) => (Some(f32::MAX), false, None),
        Some(ScrollTo::Match(row)) => (None, false, Some(row)),
        None => (None, follow, None),
    };
    let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
    area = match offset {
        Some(y) => area.vertical_scroll_offset(y),
        None => area.stick_to_bottom(stick),
    };
    area.show(ui, |ui| {
        ui.set_width(ui.available_width());
        for row in 0..displayed.len() {
            let Some(index) = displayed.line_index(row) else {
                continue;
            };
            let Some(line) = logs.get(index) else {
                continue;
            };
            let (ranges, active_local) = highlights(find, active, row, &line.text);
            let resp = render_line(ui, &line.text, &font, base, &ranges, active_local);
            if Some(row) == match_row {
                resp.scroll_to_me(Some(egui::Align::Center));
            }
        }
        ui.add_space(4.0); // a little slack below the last line, easier to select
    });
}

/// Find-match ranges for one line, plus the local index of the focused match if
/// it lands on this row (so `to_job` can paint that one span more strongly).
fn highlights(
    find: Option<&Matcher>,
    active: Option<&FindMatch>,
    row: usize,
    text: &str,
) -> (Vec<Range<usize>>, Option<usize>) {
    let Some(matcher) = find else {
        return (Vec::new(), None);
    };
    let ranges = matcher.find_ranges(&crate::ansi::strip(text));
    let active_local = active
        .filter(|a| a.row == row)
        .and_then(|a| ranges.iter().position(|r| *r == a.range));
    (ranges, active_local)
}

/// Render one log line as a wrapping, selectable label; returns its response so
/// the caller can scroll a find match into view.
fn render_line(
    ui: &mut egui::Ui,
    text: &str,
    font: &egui::FontId,
    base: egui::Color32,
    ranges: &[Range<usize>],
    active: Option<usize>,
) -> egui::Response {
    let job = crate::ansi::to_job(text, font.clone(), base, ranges, active);
    // Default wrap (egui wraps text in a vertical layout): a long line folds onto
    // the next visual row rather than running off the right edge.
    ui.add(egui::Label::new(job).selectable(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::log_buffer::Stream;

    fn buf(lines: &[&str]) -> LogBuffer {
        let mut b = LogBuffer::with_capacity(10_000);
        for line in lines {
            b.push(Stream::Stdout, *line);
        }
        b
    }

    fn matcher(query: &str) -> Matcher {
        Matcher::new(query, false, false, false).unwrap().unwrap()
    }

    #[test]
    fn displayed_all_is_identity_then_blank_row() {
        let d = Displayed::All(3);
        assert_eq!(d.len(), 3);
        assert_eq!(d.line_index(0), Some(0));
        assert_eq!(d.line_index(2), Some(2));
        assert_eq!(d.line_index(3), None); // the trailing blank row
    }

    #[test]
    fn displayed_filtered_maps_through_rows() {
        let rows = vec![1usize, 4, 9];
        let d = Displayed::Filtered(&rows);
        assert_eq!(d.len(), 3);
        assert_eq!(d.line_index(0), Some(1));
        assert_eq!(d.line_index(2), Some(9));
        assert_eq!(d.line_index(3), None);
    }

    #[test]
    fn collect_matches_empty_when_find_inactive() {
        let logs = buf(&["error here", "all good"]);
        let d = Displayed::All(logs.len());
        assert!(collect_matches(&logs, &d, None).is_empty());
    }

    #[test]
    fn collect_matches_records_row_and_ranges() {
        let logs = buf(&["nope", "hit and hit", "done"]);
        let d = Displayed::All(logs.len());
        let found = collect_matches(&logs, &d, Some(&matcher("hit")));
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].row, found[0].range.clone()), (1, 0..3));
        assert_eq!((found[1].row, found[1].range.clone()), (1, 8..11));
    }

    #[test]
    fn collect_matches_uses_display_rows_under_grep() {
        let logs = buf(&["skip", "keep hit", "skip", "keep hit"]);
        // grep left buffer rows 1 and 3, which become display rows 0 and 1.
        let rows = vec![1usize, 3];
        let d = Displayed::Filtered(&rows);
        let found = collect_matches(&logs, &d, Some(&matcher("hit")));
        assert_eq!(found.iter().map(|m| m.row).collect::<Vec<_>>(), vec![0, 1]);
    }

    #[test]
    fn highlights_marks_active_only_on_its_row() {
        let m = matcher("x");
        let active = FindMatch {
            row: 5,
            range: 2..3,
        };
        let (ranges, local) = highlights(Some(&m), Some(&active), 5, "x x x");
        assert_eq!(ranges, vec![0..1, 2..3, 4..5]);
        assert_eq!(local, Some(1)); // the 2..3 match is the second one on the row
        let (_, other_row) = highlights(Some(&m), Some(&active), 6, "x x x");
        assert_eq!(other_row, None);
    }

    #[test]
    fn highlights_empty_when_find_inactive() {
        let (ranges, local) = highlights(None, None, 0, "anything");
        assert!(ranges.is_empty());
        assert_eq!(local, None);
    }

    #[test]
    fn cmd_f_toggles_the_box() {
        // Closed -> open and focus; open -> closed, no focus.
        assert_eq!(next_search_state(false, true, false), (true, true));
        assert_eq!(next_search_state(true, true, false), (false, false));
    }

    #[test]
    fn escape_closes_only_an_open_box() {
        assert_eq!(next_search_state(true, false, true), (false, false));
        // Escape with the box already closed is a no-op (leaves other keys alone).
        assert_eq!(next_search_state(false, false, true), (false, false));
    }

    #[test]
    fn search_state_is_stable_without_keys() {
        assert_eq!(next_search_state(true, false, false), (true, false));
        assert_eq!(next_search_state(false, false, false), (false, false));
    }

    #[test]
    fn cmd_f_and_escape_together_resolve_to_toggle() {
        // The toggle branch takes precedence; from open, both keys agree on close.
        assert_eq!(next_search_state(true, true, true), (false, false));
        // From closed, Cmd/Ctrl+F opens even if Escape is also down.
        assert_eq!(next_search_state(false, true, true), (true, true));
    }
}
