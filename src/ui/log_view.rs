//! The log viewer: a search box, follow toggle, clear button, and a virtualized
//! tailing list. Visual polish (stderr colors, spacing) is a later pass.

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
    let query = state.search.trim().to_lowercase();

    if query.is_empty() {
        // Common case: no filter — virtualize directly over the buffer, with no
        // per-frame index allocation.
        if logs.is_empty() {
            ui.weak("no output yet");
            return clear_requested;
        }
        egui::ScrollArea::vertical()
            .stick_to_bottom(state.follow)
            .auto_shrink([false, false])
            .show_rows(ui, row_height, logs.len(), |ui, range| {
                for row in range {
                    if let Some(line) = logs.get(row) {
                        ui.monospace(&line.text);
                    }
                }
            });
        return clear_requested;
    }

    // Filtered view: collect matching indices (a scan is user-initiated and
    // less frequent than plain tailing). A search shouldn't force-scroll.
    let visible: Vec<usize> = (0..logs.len())
        .filter(|&index| {
            logs.get(index)
                .is_some_and(|line| line.text.to_lowercase().contains(&query))
        })
        .collect();
    if visible.is_empty() {
        ui.weak("no matching lines");
        return clear_requested;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, visible.len(), |ui, range| {
            for row in range {
                if let Some(line) = logs.get(visible[row]) {
                    ui.monospace(&line.text);
                }
            }
        });

    clear_requested
}
