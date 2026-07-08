//! egui rendering, split by area. Kept out of `main.rs` so each view stays
//! small and focused. Panels render from a read-only [`View`] and report user
//! intent through [`Action`]; the app applies actions after the panels close.

use crate::metrics::Metrics;
use crate::model::ServerConfig;
use crate::process::running::{RunningProcess, Status};
use eframe::egui;
use std::collections::{BTreeSet, HashMap};

pub mod detail;
pub mod editor;
pub mod help;
pub mod icons;
pub mod log_view;
pub mod server_list;

/// A user action captured during rendering, applied after the panels close so
/// the render code never needs a mutable borrow of the app state.
pub enum Action {
    Start(String),
    Stop(String),
    Restart(String),
    ClearLogs(String),
    Select(String),
    OpenNew,
    OpenEdit(String),
    OpenHelp,
}

/// Read-only view of the app state that the panels render from.
pub struct View<'a> {
    pub servers: &'a [ServerConfig],
    pub running: &'a HashMap<String, RunningProcess>,
    pub dup_ports: &'a BTreeSet<u16>,
    pub selected: Option<&'a str>,
    pub metrics: &'a Metrics,
}

/// Status indicator color: green running / amber starting / red crashed / gray
/// stopped.
pub fn status_color(status: &Status) -> egui::Color32 {
    match status {
        Status::Running => egui::Color32::from_rgb(0x2E, 0x7D, 0x32),
        Status::Starting => egui::Color32::from_rgb(0xC2, 0x88, 0x1F),
        Status::Crashed { .. } => egui::Color32::from_rgb(0xC0, 0x39, 0x2B),
        Status::Stopped => egui::Color32::from_rgb(0x6E, 0x6E, 0x6E),
    }
}

/// Fill for the status **dot**. A dot is a tiny filled circle with no text to
/// read, so it wants a vivid, bright color; `status_color` is instead tuned for
/// badge-text legibility on white (WCAG AA), which forces dark, muted hues that
/// read as grey at dot size and even sit *darker* than the stopped grey.
/// Running and Crashed diverge here — a brighter, more saturated green / red
/// (both lighter than the stopped grey, so red-green color-blind users get a
/// brightness cue) make a live or crashed server unmistakable next to a stopped
/// one; Starting and Stopped reuse `status_color`.
fn status_dot_fill(status: &Status) -> egui::Color32 {
    match status {
        Status::Running => egui::Color32::from_rgb(0x22, 0xC5, 0x5E),
        Status::Crashed { .. } => egui::Color32::from_rgb(0xEF, 0x44, 0x44),
        other => status_color(other),
    }
}

/// Paint a small filled status circle inline (no font glyph dependency).
pub fn status_dot(ui: &mut egui::Ui, status: &Status) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 4.0, status_dot_fill(status));
}

pub fn status_text(status: &Status) -> String {
    match status {
        Status::Stopped => "stopped".to_string(),
        Status::Starting => "starting".to_string(),
        Status::Running => "running".to_string(),
        Status::Crashed { code: Some(code) } => format!("crashed (exit {code})"),
        Status::Crashed { code: None } => "crashed".to_string(),
    }
}

// Button constructors, kept as the single place buttons are made so styling
// stays consistent. Borderlessness and the hover fill ramp come from the theme
// (interactive `bg_stroke` is zeroed there); these just pick the content shape.

/// An icon-only button: no chrome at rest — just the glyph — with the hover
/// fill appearing on interaction. `frame_when_inactive(false)` keeps the same
/// inner margin in every state, so the layout doesn't shift on hover.
pub fn icon_button<'a>(icon: egui::Image<'a>) -> egui::Button<'a> {
    egui::Button::image(icon).frame_when_inactive(false)
}

/// A text-only button.
pub fn text_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(label)
}

/// A filled accent button for the primary action (e.g. Save).
pub fn primary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(label).color(egui::Color32::WHITE))
        .fill(crate::theme::ACCENT)
}

/// A single-line text input drawn as a bordered, padded box. Fields are
/// otherwise borderless (the theme zeroes widget outlines for the flat buttons),
/// so the box is a wrapping [`egui::Frame`]; the inner [`egui::TextEdit`] is
/// frameless and transparent. Returns the edit response.
pub fn text_input(ui: &mut egui::Ui, text: &mut String, hint: &str, width: f32) -> egui::Response {
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, crate::theme::CARD_BORDER))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .frame(egui::Frame::NONE)
                    .background_color(egui::Color32::TRANSPARENT)
                    .hint_text(hint)
                    .desired_width(width),
            )
        })
        .inner
}

/// A small on/off switch (à la shadcn's Switch): an amber pill when on, grey
/// when off, with a sliding white knob. Flips `*on` when clicked. `label` is the
/// accessible name (assistive tech announces it with the on/off state).
pub fn toggle(ui: &mut egui::Ui, on: &mut bool, label: &str) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(30.0, 17.0), egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *on, label));
    let how_on = ui.ctx().animate_bool(response.id, *on);
    let radius = 0.5 * rect.height();
    let track = if *on {
        crate::theme::ACCENT
    } else {
        egui::Color32::from_gray(0xC8)
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(radius.round() as u8), track);
    let knob_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
    ui.painter().circle_filled(
        egui::pos2(knob_x, rect.center().y),
        radius - 2.0,
        egui::Color32::WHITE,
    );
    response
}
