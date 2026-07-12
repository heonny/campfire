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

pub fn start() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/play.svg"))
}

pub fn stop() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/square.svg"))
}

pub fn restart() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/rotate-cw.svg"))
}

pub fn edit() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/pencil.svg"))
}

/// Leading affordance for the find field.
pub fn search() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/search.svg"))
}

/// Leading affordance for the grep (filter) field.
pub fn filter() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/filter.svg"))
}

/// Follow the tail — auto-scroll as new output arrives.
pub fn follow() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/chevrons-down.svg"))
}

pub fn scroll_top() -> egui::Image<'static> {
    sized(egui::include_image!(
        "../../assets/icons/arrow-up-to-line.svg"
    ))
}

pub fn scroll_bottom() -> egui::Image<'static> {
    sized(egui::include_image!(
        "../../assets/icons/arrow-down-to-line.svg"
    ))
}

pub fn clear() -> egui::Image<'static> {
    sized(egui::include_image!("../../assets/icons/eraser.svg"))
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
