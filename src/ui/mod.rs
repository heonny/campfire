//! egui rendering, split by area. Kept out of `main.rs` so each view stays
//! small and focused. Panels render from a read-only [`View`] and report user
//! intent through [`Action`]; the app applies actions after the panels close.

use crate::metrics::Metrics;
use crate::model::ServerConfig;
use crate::process::running::{RunningProcess, Status};
use eframe::egui;
use std::collections::{BTreeSet, HashMap};

pub mod confirm;
pub mod editor;
pub mod help;
pub mod icons;
pub mod log_view;
pub mod server_list;
pub mod workspaces;

/// A user action captured during rendering, applied after the panels close so
/// the render code never needs a mutable borrow of the app state.
pub enum Action {
    Start(String),
    Stop(String),
    Restart(String),
    Duplicate(String),
    Delete(String),
    ClearLogs(String),
    /// Open this server's log in the active workspace at an automatic position
    /// (the accessible non-drag path: the card context menu).
    OpenLog(String),
    /// Show this server's log in the active workspace without changing the
    /// layout (card click): focus it if open, else swap it into the focused
    /// pane, or open it as the first pane of an empty workspace.
    ShowLog(String),
    /// Move the server at index `from` to index `to` in the list (drag reorder).
    /// Indices are into the same rendered `View::servers`, resolved this frame.
    Reorder {
        from: usize,
        to: usize,
    },
    OpenNew,
    OpenEdit(String),
    OpenHelp,
    /// Collapse or expand the sidebar (project list).
    ToggleSidebar,
}

/// A sidebar card mid-drag, reported by the server list each frame so the
/// workspace dock can preview and accept the drop.
#[derive(Default)]
pub struct SidebarDrag {
    /// The server id being dragged, if a card drag is in flight.
    pub server: Option<String>,
    /// The drag was released this frame (the drop moment).
    pub finished: bool,
}

/// Read-only view of the app state that the panels render from.
pub struct View<'a> {
    /// How many servers are live (non-terminal), for the sidebar's count.
    pub active: usize,
    pub servers: &'a [ServerConfig],
    pub running: &'a HashMap<String, RunningProcess>,
    pub dup_ports: &'a BTreeSet<u16>,
    /// The ACTIVE workspace's focused pane, for the sidebar highlight.
    pub focused: Option<&'a str>,
    pub metrics: &'a Metrics,
}

/// Fill for the status dot: vivid green running / amber starting or stopping
/// / red crashed / light cool grey stopped. Running and crashed are both
/// lighter than the stopped grey, so red-green color-blind users still get a
/// brightness cue.
pub(crate) fn status_dot_fill(status: &Status) -> egui::Color32 {
    match status {
        Status::Running => egui::Color32::from_rgb(0x22, 0xC5, 0x5E),
        Status::Starting | Status::Stopping => egui::Color32::from_rgb(0xF5, 0x9E, 0x0B),
        Status::Crashed { .. } => egui::Color32::from_rgb(0xEF, 0x44, 0x44),
        // A light cool grey: quiet, and clearly "off" next to the vivid states.
        Status::Stopped => egui::Color32::from_rgb(0xB4, 0xBB, 0xC7),
    }
}

/// Paint a small filled status circle inline (no font glyph dependency).
/// Returns the response so callers can hang a tooltip off it.
pub fn status_dot(ui: &mut egui::Ui, status: &Status) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 4.0, status_dot_fill(status));
    response
}

pub fn status_text(status: &Status) -> String {
    match status {
        Status::Stopped => "stopped".to_string(),
        Status::Starting => "starting (waiting for the port)".to_string(),
        Status::Running => "running".to_string(),
        Status::Stopping => "stopping (press Stop again to force-quit)".to_string(),
        Status::Crashed { code: Some(code) } => format!("crashed (exit {code})"),
        Status::Crashed { code: None } => "crashed".to_string(),
    }
}

// Button constructors, kept as the single place buttons are made so styling
// stays consistent. Borderlessness and the hover fill ramp come from the theme
// (interactive `bg_stroke` is zeroed there); these just pick the content shape.

/// Padding around an icon glyph: equal on all sides so the hover box is a
/// square hugging the icon, instead of the wide pill the text-button padding
/// would make (10×5 around a 15px glyph reads as 35×25).
const ICON_PADDING: f32 = 5.0;

/// An icon-only button, rendered inside a scope with square [`ICON_PADDING`].
/// Wraps [`egui::Button`] so it still goes through `ui.add` / `ui.add_enabled`.
pub struct IconButton<'a> {
    button: egui::Button<'a>,
}

impl<'a> IconButton<'a> {
    pub fn min_size(mut self, size: egui::Vec2) -> Self {
        self.button = self.button.min_size(size);
        self
    }
}

impl egui::Widget for IconButton<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.scope(|ui| {
            ui.spacing_mut().button_padding = egui::Vec2::splat(ICON_PADDING);
            ui.add(self.button)
        })
        .inner
    }
}

/// An icon-only button: no chrome at rest — just the glyph — with the hover
/// fill appearing on interaction. `frame_when_inactive(false)` keeps the same
/// inner margin in every state, so the layout doesn't shift on hover.
pub fn icon_button<'a>(icon: egui::Image<'a>) -> IconButton<'a> {
    IconButton {
        button: egui::Button::image(icon).frame_when_inactive(false),
    }
}

/// An icon button that stays visibly "pressed" — a soft grey box — while `on`,
/// used for the log view's follow toggle. Chromeless at rest when off (like
/// [`icon_button`]); when on, the same hover fill is shown at rest so the active
/// state reads without color. `selected(on)` also announces on/off to assistive
/// tech. The caller flips the bound flag when the button is clicked.
pub fn icon_toggle_button<'a>(icon: egui::Image<'a>, on: bool) -> IconButton<'a> {
    let button = egui::Button::image(icon)
        .selected(on)
        .frame_when_inactive(on);
    let button = if on {
        button.fill(crate::theme::BUTTON_HOVER_FILL)
    } else {
        button
    };
    IconButton { button }
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

/// A weak `:port` label that opens `http://localhost:port` in the browser on
/// click — the most common thing to do with a local dev server's port.
pub fn port_link(ui: &mut egui::Ui, port: u16) -> egui::Response {
    let url = crate::system::localhost_url(port);
    let response = ui
        .add(
            egui::Label::new(egui::RichText::new(format!(":{port}")).weak())
                .sense(egui::Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("Open {url}"));
    if response.clicked() {
        crate::system::open_url(&url);
    }
    response
}

/// A single-line text input drawn as a bordered, padded box. Fields are
/// otherwise borderless (the theme zeroes widget outlines for the flat buttons),
/// so the box is a wrapping [`egui::Frame`]; the inner [`egui::TextEdit`] is
/// frameless and transparent. Returns the edit response.
pub fn text_input(ui: &mut egui::Ui, text: &mut String, hint: &str, width: f32) -> egui::Response {
    text_input_frame(false)
        .show(ui, |ui| ui.add(frameless_edit(text, hint, width)))
        .inner
}

/// The bordered box every text field sits in. `error` swaps the hairline for
/// the danger red (an invalid regex, a bad port).
pub fn text_input_frame(error: bool) -> egui::Frame {
    let stroke = if error {
        crate::theme::DANGER
    } else {
        crate::theme::CARD_BORDER
    };
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 5))
}

/// The transparent single-line editor that goes inside [`text_input_frame`].
pub fn frameless_edit<'a>(text: &'a mut String, hint: &str, width: f32) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .frame(egui::Frame::NONE)
        .background_color(egui::Color32::TRANSPARENT)
        .hint_text(hint)
        .desired_width(width)
}
