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
//!
//! The module is split by concern: this file owns the view state and the
//! per-frame orchestration (`show`); [`cache`] rebuilds the filter + match set,
//! [`toolbar`] renders the search rows and control bar, and [`render`] draws the
//! lines.

mod cache;
mod render;
mod toolbar;

use crate::process::log_buffer::LogBuffer;
use eframe::egui;
use std::ops::Range;

use cache::{Cache, ensure_cache};
use render::render_body;
use toolbar::{bottom_bar, row_divider, search_rows};

/// A one-shot scroll request applied on the next frame.
enum ScrollTo {
    Top,
    Bottom,
    Match(usize), // display row holding the focused find match
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

/// Render the (optional) find/grep box, the virtualized log lines, and the
/// bottom control bar. Returns `true` if the user clicked "clear".
///
/// `salt` uniquely identifies this log pane, so its internal widget ids (the
/// bottom control panel, the scroll area) don't clash when several log views
/// are on screen at once (egui flags duplicate ids with red warnings).
/// `has_focus` gates the global Cmd/Ctrl+F shortcut: with several panes
/// visible, only the focused one may consume it (consumption is
/// first-caller-wins, which would otherwise go to whichever pane renders
/// first, not the one the user is looking at).
pub fn show(
    ui: &mut egui::Ui,
    salt: egui::Id,
    has_focus: bool,
    state: &mut LogView,
    logs: &LogBuffer,
) -> bool {
    // Cmd/Ctrl+F toggles the find/grep box (focusing it on open); Escape closes it.
    let open_key = has_focus
        && ui.input_mut(|i| {
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
    egui::Panel::bottom(salt.with("log_controls"))
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
    render_body(ui, salt, logs, &displayed, find, active, follow, scroll_to);
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

#[cfg(test)]
mod tests {
    use super::*;

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
