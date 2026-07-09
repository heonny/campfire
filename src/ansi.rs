//! Minimal ANSI SGR (color) handling for the log view: turn a line containing
//! `ESC[..m` escape sequences into a colored [`LayoutJob`], optionally
//! highlighting a search substring, and strip the codes for plain-text search.
//! Only foreground colors + reset are interpreted; other codes (bold,
//! background, cursor moves) are consumed and ignored.

use eframe::egui;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};
use std::ops::Range;

// Foreground palette tuned for a light background (readable, not neon).
const STANDARD: [Color32; 8] = [
    Color32::from_rgb(0x2B, 0x2B, 0x2B), // 30 black
    Color32::from_rgb(0xC0, 0x39, 0x2B), // 31 red
    Color32::from_rgb(0x2E, 0x7D, 0x32), // 32 green
    Color32::from_rgb(0xB8, 0x86, 0x0B), // 33 yellow
    Color32::from_rgb(0x15, 0x65, 0xC0), // 34 blue
    Color32::from_rgb(0x8E, 0x24, 0xAA), // 35 magenta
    Color32::from_rgb(0x00, 0x83, 0x8F), // 36 cyan
    Color32::from_rgb(0x61, 0x61, 0x61), // 37 white -> gray
];
const BRIGHT: [Color32; 8] = [
    Color32::from_rgb(0x61, 0x61, 0x61), // 90 bright black
    Color32::from_rgb(0xD3, 0x2F, 0x2F), // 91
    Color32::from_rgb(0x38, 0x8E, 0x3C), // 92
    Color32::from_rgb(0xC7, 0x92, 0x00), // 93
    Color32::from_rgb(0x19, 0x76, 0xD2), // 94
    Color32::from_rgb(0x9C, 0x27, 0xB0), // 95
    Color32::from_rgb(0x00, 0x97, 0xA7), // 96
    Color32::from_rgb(0x42, 0x42, 0x42), // 97
];

/// Find-match highlight backgrounds: a soft amber behind every match, and a
/// stronger amber behind the one the user has stepped onto (find navigation).
const HIGHLIGHT_BG: Color32 = Color32::from_rgb(0xFF, 0xE0, 0x82);
const ACTIVE_BG: Color32 = Color32::from_rgb(0xFF, 0xB3, 0x00);

/// Build a colored [`LayoutJob`] from a line that may contain ANSI SGR codes.
/// `ranges` are find-match byte ranges in the ANSI-**stripped** text (as
/// returned by [`crate::search::Matcher::find_ranges`]); their spans get the
/// highlight background, and the one at index `active` gets the stronger
/// "current match" background. Pass an empty slice for no highlighting.
pub fn to_job(
    text: &str,
    font: FontId,
    base: Color32,
    ranges: &[Range<usize>],
    active: Option<usize>,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut color = base;
    let mut rest = text;
    let mut pos = 0; // byte offset into the stripped text, tracked so `ranges` map onto runs
    while let Some(idx) = rest.find('\x1b') {
        if idx > 0 {
            push_runs(&mut job, &rest[..idx], pos, &font, color, ranges, active);
            pos += idx;
        }
        match parse_csi(&rest[idx..], color, base) {
            Some((consumed, new_color)) => {
                color = new_color;
                rest = &rest[idx + consumed..];
            }
            None => rest = &rest[idx + 1..], // unrecognized escape: drop the ESC
        }
    }
    if !rest.is_empty() {
        push_runs(&mut job, rest, pos, &font, color, ranges, active);
    }
    job
}

/// Remove ANSI escape sequences, for plain-text search matching.
pub fn strip(text: &str) -> String {
    if !text.contains('\x1b') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(idx) = rest.find('\x1b') {
        out.push_str(&rest[..idx]);
        match parse_csi(&rest[idx..], Color32::BLACK, Color32::BLACK) {
            Some((consumed, _)) => rest = &rest[idx + consumed..],
            None => rest = &rest[idx + 1..],
        }
    }
    out.push_str(rest);
    out
}

/// Append one literal run (`chunk`, ANSI-free) in `color`, painting the parts
/// that fall inside `ranges` with the highlight background. `chunk_start` is the
/// chunk's byte offset in the stripped text, so the stripped-coordinate `ranges`
/// map straight onto it. The `active` range gets [`ACTIVE_BG`]; the rest get
/// [`HIGHLIGHT_BG`]. A match that straddles a color boundary spans two chunks —
/// each chunk paints the part that lands in it. `ranges` are sorted and
/// non-overlapping (as [`crate::search::Matcher::find_ranges`] returns them).
fn push_runs(
    job: &mut LayoutJob,
    chunk: &str,
    chunk_start: usize,
    font: &FontId,
    color: Color32,
    ranges: &[Range<usize>],
    active: Option<usize>,
) {
    let chunk_end = chunk_start + chunk.len();
    let mut cursor = chunk_start; // absolute stripped offset of the next unpainted byte
    for (i, r) in ranges.iter().enumerate() {
        if r.start >= chunk_end {
            break; // ranges are sorted; nothing after this overlaps the chunk
        }
        if r.end <= cursor {
            continue; // already painted past this match
        }
        let seg_start = r.start.max(chunk_start);
        let seg_end = r.end.min(chunk_end);
        let (lo, hi) = (seg_start - chunk_start, seg_end - chunk_start);
        if !chunk.is_char_boundary(lo) || !chunk.is_char_boundary(hi) {
            continue; // ranges align to chars; skip defensively if they don't
        }
        if seg_start > cursor {
            append(job, &chunk[cursor - chunk_start..lo], font, color, None);
        }
        let bg = if active == Some(i) {
            ACTIVE_BG
        } else {
            HIGHLIGHT_BG
        };
        append(job, &chunk[lo..hi], font, color, Some(bg));
        cursor = seg_end;
    }
    if cursor < chunk_end {
        append(job, &chunk[cursor - chunk_start..], font, color, None);
    }
}

fn append(
    job: &mut LayoutJob,
    text: &str,
    font: &FontId,
    color: Color32,
    background: Option<Color32>,
) {
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font.clone(),
            color,
            background: background.unwrap_or(Color32::TRANSPARENT),
            ..Default::default()
        },
    );
}

/// Parse a CSI escape starting at `s[0] == ESC`. Returns the bytes consumed and
/// the resulting color (SGR `m` sequences update it; other CSI leave it).
fn parse_csi(s: &str, current: Color32, base: Color32) -> Option<(usize, Color32)> {
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes[1] != b'[' {
        return None;
    }
    let mut i = 2;
    while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
        i += 1;
    }
    if i >= bytes.len() {
        return None; // no final byte
    }
    let final_byte = bytes[i];
    let params = &s[2..i];
    let consumed = i + 1;
    if final_byte != b'm' {
        return Some((consumed, current)); // e.g. cursor move — ignore
    }
    if params.is_empty() {
        return Some((consumed, base)); // ESC[m == reset
    }
    let mut color = current;
    for code in params.split(';') {
        match code.parse::<u16>() {
            Ok(0) | Ok(39) => color = base,
            Ok(n @ 30..=37) => color = STANDARD[(n - 30) as usize],
            Ok(n @ 90..=97) => color = BRIGHT[(n - 90) as usize],
            _ => {}
        }
    }
    Some((consumed, color))
}

#[cfg(test)]
mod tests {
    // The `&[a..b]` highlight inputs are deliberate single-element range slices.
    #![allow(clippy::single_range_in_vec_init)]
    use super::*;

    #[test]
    fn strip_removes_color_codes() {
        assert_eq!(strip("\x1b[39mDEBUG\x1b[0;39m done"), "DEBUG done");
    }

    #[test]
    fn strip_passes_through_plain_text() {
        assert_eq!(strip("plain line"), "plain line");
    }

    #[test]
    fn to_job_splits_into_colored_runs() {
        let base = Color32::BLACK;
        let job = to_job(
            "\x1b[31mred\x1b[0m tail",
            FontId::default(),
            base,
            &[],
            None,
        );
        assert_eq!(job.text, "red tail");
        assert_eq!(job.sections.len(), 2);
        assert_eq!(job.sections[0].format.color, STANDARD[1]); // red
        assert_eq!(job.sections[1].format.color, base); // reset to base
    }

    #[test]
    fn to_job_plain_text_is_single_run() {
        let job = to_job(
            "no colors here",
            FontId::default(),
            Color32::BLACK,
            &[],
            None,
        );
        assert_eq!(job.sections.len(), 1);
        assert_eq!(job.text, "no colors here");
    }

    #[test]
    fn to_job_highlights_given_range() {
        let job = to_job(
            "hello WORLD",
            FontId::default(),
            Color32::BLACK,
            &[6..11],
            None,
        );
        assert_eq!(job.text, "hello WORLD");
        assert_eq!(job.sections.len(), 2); // "hello " + highlighted "WORLD"
        assert_eq!(job.sections[0].format.background, Color32::TRANSPARENT);
        assert_eq!(job.sections[1].format.background, HIGHLIGHT_BG);
    }

    #[test]
    fn to_job_active_range_uses_stronger_bg() {
        let job = to_job("abcd", FontId::default(), Color32::BLACK, &[0..2], Some(0));
        assert_eq!(job.sections[0].format.background, ACTIVE_BG);
    }

    #[test]
    fn to_job_highlights_across_color_boundary() {
        // stripped = "abcd"; range 1..3 ("bc") straddles the red -> reset boundary.
        let base = Color32::BLACK;
        let job = to_job(
            "\x1b[31mab\x1b[0mcd",
            FontId::default(),
            base,
            &[1..3],
            None,
        );
        assert_eq!(job.text, "abcd");
        assert_eq!(job.sections.len(), 4); // a | b | c | d
        assert_eq!(job.sections[1].format.background, HIGHLIGHT_BG);
        assert_eq!(job.sections[1].format.color, STANDARD[1]); // "b" still red
        assert_eq!(job.sections[2].format.background, HIGHLIGHT_BG);
        assert_eq!(job.sections[2].format.color, base); // "c" after the reset
    }

    #[test]
    fn to_job_highlights_non_ascii_range() {
        // "빌드 " is 7 bytes, so "완료" spans 7..13.
        let job = to_job(
            "빌드 완료됨",
            FontId::default(),
            Color32::BLACK,
            &[7..13],
            None,
        );
        assert_eq!(job.text, "빌드 완료됨");
        assert_eq!(job.sections.len(), 3); // "빌드 " + "완료" + "됨"
        assert_eq!(job.sections[1].format.background, HIGHLIGHT_BG);
    }
}
