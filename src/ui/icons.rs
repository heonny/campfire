//! lucide icons (ISC-licensed SVGs under `assets/icons/`), rasterized at runtime
//! by the egui_extras SVG loader. They render in `currentColor` (dark) to match
//! the button text, and are sized to sit inline next to a label.

use eframe::egui;

/// Wrap an SVG source as an inline-sized icon image.
fn sized(source: egui::ImageSource<'static>) -> egui::Image<'static> {
    egui::Image::new(source).fit_to_exact_size(egui::vec2(15.0, 15.0))
}

pub fn add() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/plus.svg"))
}

/// Step to the previous find match.
pub fn chevron_up() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/chevron-up.svg"))
}

/// Step to the next find match.
pub fn chevron_down() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/chevron-down.svg"))
}

pub fn help() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/info.svg"))
}

/// Toggle the sidebar (project list) collapsed/expanded.
pub fn sidebar() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/panel-left.svg"))
}

/// Close a log pane or a workspace tab.
pub fn close() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/x.svg"))
}
