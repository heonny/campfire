//! The notification banner: an iOS-style card that slides down from the top
//! edge, holds, and slides back up. App icon on the left, a bold title over
//! the message, a quiet "now" on the right — a large radius, a hairline, and a
//! soft drop shadow. Purely presentational; the app owns the toast state and
//! its lifetime.

use crate::theme;
use eframe::egui;

const WIDTH: f32 = 380.0;
const ICON: f32 = 38.0;
/// Resting distance from the top edge.
const TOP: f32 = 14.0;
/// How long the slide in / out takes.
pub const SLIDE_TIME: f32 = 0.4;

/// Draw the banner. `progress` is 0 (fully hidden above the window) to 1
/// (resting); the caller animates it. `icon` is the app mark, if loaded.
pub fn show(
    ctx: &egui::Context,
    icon: Option<&egui::TextureHandle>,
    title: &str,
    text: &str,
    progress: f32,
) {
    // Ease-out: fast start, gentle settle — the iOS banner curve.
    let eased = 1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(3);
    // Slide from just above the window edge (its own height plus shadow) down
    // to the resting offset. The height is only known after layout, so use a
    // generous constant; the card fully clears the edge either way.
    let hidden_y = -(ICON + 40.0 + 30.0);
    let y = hidden_y + (TOP - hidden_y) * eased;

    egui::Area::new(egui::Id::new("banner"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, y))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(
                    0xFF, 0xFF, 0xFF, 0xF4,
                ))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_black_alpha(18)))
                .corner_radius(egui::CornerRadius::same(22))
                .inner_margin(egui::Margin::symmetric(14, 12))
                .shadow(egui::Shadow {
                    offset: [0, 10],
                    blur: 28,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(38),
                })
                .show(ui, |ui| {
                    // An Area is unbounded, so every row here must be given
                    // its width explicitly or it stretches to the window.
                    let inner = WIDTH - 28.0;
                    ui.set_width(inner);
                    ui.set_max_width(inner);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let column = inner - ICON - 12.0;
                        match icon {
                            Some(tex) => {
                                ui.add(
                                    egui::Image::from_texture(tex)
                                        .fit_to_exact_size(egui::Vec2::splat(ICON))
                                        .corner_radius(9.0),
                                );
                            }
                            None => {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::Vec2::splat(ICON),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    egui::CornerRadius::same(9),
                                    theme::ACCENT,
                                );
                            }
                        }
                        ui.vertical(|ui| {
                            ui.set_width(column);
                            ui.set_max_width(column);
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.horizontal(|ui| {
                                ui.set_max_width(column);
                                ui.add(
                                    egui::Label::new(egui::RichText::new(title).strong())
                                        .truncate(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(egui::RichText::new("now").small().weak());
                                    },
                                );
                            });
                            ui.add(egui::Label::new(text).wrap());
                        });
                    });
                });
        });
}
