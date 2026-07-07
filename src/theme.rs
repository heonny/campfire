//! Light theme: the Pretendard font (Latin + Korean) plus a warm amber accent,
//! so the app reads as a clean dev tool rather than raw egui defaults.

use eframe::egui;
use egui::{vec2, Color32, CornerRadius, Margin, Stroke, Visuals};

/// Campfire accent — a warm ember orange — and its pale selection tint.
const ACCENT: Color32 = Color32::from_rgb(0xC2, 0x41, 0x0C);
const ACCENT_WEAK: Color32 = Color32::from_rgb(0xFB, 0xE3, 0xCC);

/// Install fonts and visuals. Call once from the eframe creation closure.
pub fn setup(ctx: &egui::Context) {
    install_fonts(ctx);
    install_visuals(ctx);
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "pretendard".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/Pretendard-Regular.otf"
        ))),
    );
    // Pretendard covers Latin + Hangul: make it the primary proportional font,
    // and a monospace fallback so Korean still resolves in the log view.
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        family.insert(0, "pretendard".to_owned());
    }
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        family.push("pretendard".to_owned());
    }
    ctx.set_fonts(fonts);
}

fn install_visuals(ctx: &egui::Context) {
    let visuals = build_visuals();
    // egui 0.35 keeps a Style per theme; overwrite both so our look applies
    // regardless of the system light/dark preference.
    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.button_padding = vec2(10.0, 5.0);
        style.spacing.window_margin = Margin::same(12);
    });
}

fn build_visuals() -> Visuals {
    let mut visuals = Visuals::light();

    visuals.panel_fill = Color32::from_rgb(0xFB, 0xFB, 0xFA);
    visuals.window_fill = Color32::WHITE;
    visuals.faint_bg_color = Color32::from_rgb(0xF2, 0xF1, 0xEE);
    visuals.extreme_bg_color = Color32::WHITE;

    visuals.selection.bg_fill = ACCENT_WEAK;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;

    let radius = CornerRadius::same(6);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = radius;
    }
    visuals.window_corner_radius = CornerRadius::same(10);

    visuals
}
