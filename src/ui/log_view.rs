//! The log viewer: a search box, follow toggle, clear button, and a virtualized
//! tailing list with ANSI colors, selectable lines, and a blank bottom row.

use crate::process::log_buffer::LogBuffer;
use eframe::egui;

/// View state for the log pane (search text + whether to tail).
pub struct LogView {
    search: String,
    follow: bool,
}

impl Default for LogView {
    fn default() -> Self {
        Self { search: String::new(), follow: true }
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
        ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text("search")
                .desired_width(220.0),
        );
        ui.checkbox(&mut state.follow, "follow");
        if ui.button("clear").clicked() {
            clear_requested = true;
        }
    });

    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let base = ui.visuals().text_color();
    let query = state.search.trim().to_lowercase();

    if query.is_empty() {
        if logs.is_empty() {
            ui.weak("no output yet");
            return clear_requested;
        }
        // One extra virtual row renders as a blank line at the bottom, so the
        // last real line has room below it (easier to select/drag).
        egui::ScrollArea::vertical()
            .stick_to_bottom(state.follow)
            .auto_shrink([false, false])
            .show_rows(ui, row_height, logs.len() + 1, |ui, range| {
                for row in range {
                    match logs.get(row) {
                        Some(line) => render_line(ui, &line.text, &font, base),
                        None => {
                            ui.monospace(" ");
                        }
                    }
                }
            });
        return clear_requested;
    }

    // Filtered view: match against the ANSI-stripped text.
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
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, visible.len() + 1, |ui, range| {
            for row in range {
                match visible.get(row) {
                    Some(&index) => {
                        if let Some(line) = logs.get(index) {
                            render_line(ui, &line.text, &font, base);
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

fn render_line(ui: &mut egui::Ui, text: &str, font: &egui::FontId, base: egui::Color32) {
    let job = crate::ansi::to_job(text, font.clone(), base);
    ui.add(egui::Label::new(job).selectable(true));
}
