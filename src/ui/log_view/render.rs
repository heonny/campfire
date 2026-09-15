//! Rendering the log lines: the (plain, variable-height) scroll area, per-line
//! find highlighting, and one wrapping selectable label per line.

use super::{Displayed, FindMatch, ScrollTo};
use crate::process::log_buffer::LogBuffer;
use crate::search::Matcher;
use eframe::egui;
use std::ops::Range;

/// The scrollable line list. Lines **wrap** (no sideways scroll), so rows have
/// variable height — which rules out egui's fixed-height `show_rows`. Plain
/// `show` lays every line out at its real height instead, so `stick_to_bottom`
/// follows the true content bottom and no longer oscillates ("trembles") the way
/// it did when wrapped rows broke `show_rows`' uniform-height assumption.
///
/// The trade-off: every displayed line is built each frame (egui caches the
/// galleys, so a steady tail stays cheap). If a very large scrollback ever makes
/// this heavy, swap in a variable-height virtual list.
// Straight-line render pipeline inputs; bundling them into a struct would just
// move the same fields behind an extra name.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_body(
    ui: &mut egui::Ui,
    salt: egui::Id,
    logs: &LogBuffer,
    displayed: &Displayed<'_>,
    find: Option<&Matcher>,
    active: Option<&FindMatch>,
    follow: bool,
    wrap: bool,
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
    // Wrapped lines only scroll vertically; extended lines add a sideways axis.
    let mut area = if wrap {
        egui::ScrollArea::vertical()
    } else {
        egui::ScrollArea::both()
    }
    .auto_shrink([false, false])
    .id_salt(salt);
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
            let resp = render_line(ui, &line.text, &font, base, &ranges, active_local, wrap);
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

/// Render one log line as a selectable label (wrapped or extended); returns its response so
/// the caller can scroll a find match into view.
fn render_line(
    ui: &mut egui::Ui,
    text: &str,
    font: &egui::FontId,
    base: egui::Color32,
    ranges: &[Range<usize>],
    active: Option<usize>,
    wrap: bool,
) -> egui::Response {
    let job = crate::ansi::to_job(text, font.clone(), base, ranges, active);
    // Wrap (egui's default in a vertical layout) folds a long line onto the next
    // visual row; extend keeps it on one row under the horizontal scroll.
    let label = egui::Label::new(job).selectable(true);
    let label = if wrap { label.wrap() } else { label.extend() };
    ui.add(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matcher(query: &str) -> Matcher {
        Matcher::new(query, false, false, false).unwrap().unwrap()
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
}
