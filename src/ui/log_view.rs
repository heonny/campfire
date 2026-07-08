//! The log viewer: a search box, top/bottom/follow/clear controls, and a
//! virtualized tailing list with ANSI colors, search highlighting, selectable
//! lines, and a blank bottom row.

use super::{icon_button, icons, text_input, toggle};
use crate::process::log_buffer::LogBuffer;
use eframe::egui;

/// A one-shot scroll request from the top/bottom buttons.
enum ScrollTo {
    Top,
    Bottom,
}

/// View state for the log pane.
pub struct LogView {
    search: String,
    follow: bool,
    scroll_to: Option<ScrollTo>,
}

impl Default for LogView {
    fn default() -> Self {
        Self { search: String::new(), follow: true, scroll_to: None }
    }
}

impl LogView {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Render the toolbar and the (optionally filtered, virtualized) log lines.
/// Returns `true` if the user clicked "clear".
pub fn show(ui: &mut egui::Ui, state: &mut LogView, logs: &LogBuffer) -> bool {
    let mut clear_requested = false;
    ui.horizontal(|ui| {
        ui.label("Log");
        text_input(ui, &mut state.search, "search", 150.0);
        // All controls grouped at the right edge; the log body stays the focus.
        // (right_to_left lays out in reverse, so the reading order is
        //  top, bottom, clear … follow [switch].)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            toggle(ui, &mut state.follow, "follow").on_hover_text("follow new output");
            ui.label("follow");
            ui.add_space(6.0);
            if ui.add(icon_button(icons::clear())).on_hover_text("clear log").clicked() {
                clear_requested = true;
            }
            if ui
                .add(icon_button(icons::scroll_bottom()))
                .on_hover_text("scroll to bottom")
                .clicked()
            {
                state.scroll_to = Some(ScrollTo::Bottom);
            }
            if ui.add(icon_button(icons::scroll_top())).on_hover_text("scroll to top").clicked() {
                state.scroll_to = Some(ScrollTo::Top);
            }
        });
    });

    // Tight line spacing for a terminal-like density (the app default is roomier).
    ui.spacing_mut().item_spacing.y = 2.0;
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let base = ui.visuals().text_color();
    let query = state.search.trim().to_lowercase();
    let scroll_to = state.scroll_to.take();

    if query.is_empty() {
        if logs.is_empty() {
            ui.weak("no output yet");
            return clear_requested;
        }
        // One extra virtual row leaves a blank line at the bottom (easier to
        // select/drag).
        let area = egui::ScrollArea::vertical().auto_shrink([false, false]);
        scrolled(area, scroll_to, state.follow).show_rows(
            ui,
            row_height,
            logs.len() + 1,
            |ui, range| {
                for row in range {
                    match logs.get(row) {
                        Some(line) => render_line(ui, &line.text, &font, base, None),
                        None => {
                            ui.monospace(" ");
                        }
                    }
                }
            },
        );
        return clear_requested;
    }

    // Filtered view: match against the ANSI-stripped text; highlight the query.
    let visible: Vec<usize> = (0..logs.len())
        .filter(|&index| {
            logs.get(index)
                .is_some_and(|line| crate::ansi::strip(&line.text).to_lowercase().contains(&query))
        })
        .collect();
    if visible.is_empty() {
        ui.weak("no matching lines");
        return clear_requested;
    }
    let area = egui::ScrollArea::vertical().auto_shrink([false, false]);
    scrolled(area, scroll_to, false).show_rows(ui, row_height, visible.len() + 1, |ui, range| {
        for row in range {
            match visible.get(row) {
                Some(&index) => {
                    if let Some(line) = logs.get(index) {
                        render_line(ui, &line.text, &font, base, Some(&query));
                    }
                }
                None => {
                    ui.monospace(" ");
                }
            }
        }
    });

    clear_requested
}

/// Apply a one-shot scroll request, or fall back to tailing when following.
fn scrolled(area: egui::ScrollArea, scroll_to: Option<ScrollTo>, follow: bool) -> egui::ScrollArea {
    match scroll_to {
        Some(ScrollTo::Top) => area.vertical_scroll_offset(0.0),
        Some(ScrollTo::Bottom) => area.vertical_scroll_offset(f32::MAX),
        None => area.stick_to_bottom(follow),
    }
}

fn render_line(
    ui: &mut egui::Ui,
    text: &str,
    font: &egui::FontId,
    base: egui::Color32,
    highlight: Option<&str>,
) {
    let job = crate::ansi::to_job(text, font.clone(), base, highlight);
    ui.add(egui::Label::new(job).selectable(true));
}
