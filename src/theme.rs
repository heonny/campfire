//! Light theme: the Pretendard font (Latin + Korean) plus a warm amber accent,
//! so the app reads as a clean dev tool rather than raw egui defaults.

use eframe::egui;
use egui::{vec2, Color32, CornerRadius, Margin, Stroke, Visuals};

/// Campfire accent — a warm ember orange — and its pale selection tint.
pub const ACCENT: Color32 = Color32::from_rgb(0xC2, 0x41, 0x0C);
pub const ACCENT_WEAK: Color32 = Color32::from_rgb(0xFB, 0xE3, 0xCC);

/// Card surface palette, tuned to the Claude Code app: a muted grey card on a
/// near-white background, delineated by a hairline border, with a slightly
/// deeper grey on hover. `INSET_FILL` is the recessed surface for code blocks.
pub const CARD_FILL: Color32 = Color32::from_rgb(0xF3, 0xF3, 0xF2);
pub const CARD_BORDER: Color32 = Color32::from_rgb(0xE3, 0xE3, 0xE2);
pub const CARD_HOVER_FILL: Color32 = Color32::from_rgb(0xEB, 0xEB, 0xEA);
pub const INSET_FILL: Color32 = Color32::from_rgb(0xE9, 0xE9, 0xE8);

/// A recessed inset surface for code / command text (a touch darker than the
/// card, hairline border, rounded, snug padding).
pub fn inset_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(INSET_FILL)
        .stroke(Stroke::new(1.0, CARD_BORDER))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(8, 6))
}

/// The dialog surface: white, generously padded, rounded, with a soft drop
/// shadow so it reads as elevated above the dimmed backdrop.
pub fn modal_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, CARD_BORDER))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(22))
        .shadow(egui::Shadow {
            offset: [0, 8],
            blur: 32,
            spread: 0,
            color: Color32::from_black_alpha(38),
        })
}

/// A base card frame: muted grey surface, hairline border, rounded, padded —
/// delineated by fill + border, not elevation (no shadow, since the card sits
/// darker than the background). Callers may override `.fill`/`.stroke` (e.g. for
/// the selected/hover states).
pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD_FILL)
        .stroke(Stroke::new(1.0, CARD_BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 10))
}

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

    // App background — the Claude Code near-white; cards sit slightly darker on top.
    visuals.panel_fill = Color32::from_rgb(0xFD, 0xFD, 0xFC);
    visuals.window_fill = Color32::WHITE;
    visuals.faint_bg_color = Color32::from_rgb(0xF2, 0xF1, 0xEE);
    visuals.extreme_bg_color = Color32::WHITE;

    visuals.selection.bg_fill = ACCENT_WEAK;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;

    // Buttons are borderless — their only hover/press feedback is a gentle fill
    // darkening (below). egui's light theme gives inactive widgets a 0-width
    // border but hovered/active a 1px one; that per-state width mismatch made
    // buttons resize by 1px on hover, so drop the interactive borders entirely.
    // Text fields keep their focus ring — that uses `selection.stroke`, untouched.
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.bg_stroke = Stroke::NONE;
    }
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xEA, 0xEA, 0xE9);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xE1, 0xE1, 0xE0);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0xD8, 0xD8, 0xD7);

    let radius = CornerRadius::same(6);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = radius;
        // Don't grow widgets on hover/press — the size change makes the toolbar
        // layout jitter. Feedback comes from the fill color instead.
        widget.expansion = 0.0;
    }
    visuals.window_corner_radius = CornerRadius::same(10);

    visuals
}
