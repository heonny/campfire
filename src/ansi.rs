//! Minimal ANSI SGR (color) handling for the log view: turn a line containing
//! `ESC[..m` escape sequences into a colored [`LayoutJob`], and strip the codes
//! for plain-text search. Only foreground colors + reset are interpreted;
//! other codes (bold, background, cursor moves) are consumed and ignored.

use eframe::egui;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

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

/// Build a colored [`LayoutJob`] from a line that may contain ANSI SGR codes.
pub fn to_job(text: &str, font: FontId, base: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut color = base;
    let mut rest = text;
    while let Some(idx) = rest.find('\x1b') {
        if idx > 0 {
            push(&mut job, &rest[..idx], &font, color);
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
        push(&mut job, rest, &font, color);
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

fn push(job: &mut LayoutJob, text: &str, font: &FontId, color: Color32) {
    job.append(
        text,
        0.0,
        TextFormat { font_id: font.clone(), color, ..Default::default() },
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
        let job = to_job("\x1b[31mred\x1b[0m tail", FontId::default(), base);
        assert_eq!(job.text, "red tail");
        assert_eq!(job.sections.len(), 2);
        assert_eq!(job.sections[0].format.color, STANDARD[1]); // red
        assert_eq!(job.sections[1].format.color, base); // reset to base
    }

    #[test]
    fn to_job_plain_text_is_single_run() {
        let job = to_job("no colors here", FontId::default(), Color32::BLACK);
        assert_eq!(job.sections.len(), 1);
        assert_eq!(job.text, "no colors here");
    }
}
