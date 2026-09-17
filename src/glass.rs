//! Static glass materials, isolated from server state and process management.

use eframe::egui::{self, Color32, Margin};
use egui_glass::{Glass, GlassStyle};

const BACKDROP_SIZE: usize = 256;

pub fn init(cc: &eframe::CreationContext<'_>) -> Result<(), std::io::Error> {
    let render_state = cc
        .wgpu_render_state
        .as_ref()
        .ok_or_else(|| std::io::Error::other("Campfire glass requires the wgpu renderer"))?;
    egui_glass::init(render_state, 1);
    egui_glass::set_backdrop(&cc.egui_ctx, render_state, &backdrop());
    Ok(())
}

fn backdrop() -> egui::ColorImage {
    let mut image = egui::ColorImage::filled([BACKDROP_SIZE; 2], crate::theme::CANVAS_FILL);
    for y in 0..BACKDROP_SIZE {
        for x in 0..BACKDROP_SIZE {
            let u = x as f32 / (BACKDROP_SIZE - 1) as f32;
            let v = y as f32 / (BACKDROP_SIZE - 1) as f32;
            let ember = (-((u - 0.08).powi(2) + (v - 0.2).powi(2)) / 0.12).exp();
            let blue = (-((u - 0.8).powi(2) + (v - 0.85).powi(2)) / 0.25).exp();
            image[(x, y)] = Color32::from_rgb(
                (243.0 + 10.0 * ember - 23.0 * blue) as u8,
                (245.0 - 20.0 * ember - 12.0 * blue) as u8,
                (249.0 - 34.0 * ember + 3.0 * blue) as u8,
            );
        }
    }
    image
}

pub fn show_backdrop(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    egui_glass::show_backdrop_mapped(ui, rect, rect, crate::theme::CANVAS_FILL);
}

pub struct Surface {
    style: GlassStyle,
    frame: egui::Frame,
}

impl Surface {
    pub fn inner_margin(mut self, margin: Margin) -> Self {
        self.frame.inner_margin = margin;
        self
    }

    pub fn show<R>(
        self,
        ui: &mut egui::Ui,
        contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        // Headless UI tests have no registered GPU backdrop.
        if egui_glass::backdrop_rect(ui.ctx()).is_none() {
            return self.frame.show(ui, contents);
        }
        let style = ui.style().clone();
        Glass::new(self.style)
            .inner_margin(self.frame.inner_margin)
            .show(ui, |ui| {
                // Keep Campfire's selection, focus rings and action colours.
                ui.set_style(style);
                contents(ui)
            })
    }
}

pub fn sidebar() -> Surface {
    Surface {
        style: GlassStyle {
            corner_radius: 10.0,
            tint: Color32::from_rgba_unmultiplied(255, 255, 255, 115),
            ..GlassStyle::panel()
        },
        frame: crate::theme::block_frame(),
    }
}

pub fn modal() -> Surface {
    Surface {
        style: GlassStyle {
            corner_radius: 12.0,
            tint: Color32::from_rgba_unmultiplied(255, 255, 255, 205),
            shadow: 0.12,
            shadow_radius: 24.0,
            ..GlassStyle::panel()
        },
        frame: crate::theme::modal_frame(),
    }
}

pub fn banner() -> Surface {
    Surface {
        style: GlassStyle {
            chromatic: 0.0,
            refraction: 6.0,
            tint: Color32::from_rgba_unmultiplied(255, 255, 255, 175),
            ..GlassStyle::regular()
        },
        frame: crate::theme::modal_frame().inner_margin(Margin::symmetric(14, 12)),
    }
}
