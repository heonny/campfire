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

/// Paint a small filled status circle inline (no font glyph dependency).
pub fn status_dot(ui: &mut egui::Ui, status: &Status) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, status_color(status));
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
