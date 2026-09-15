//! Static help content, shown in a modal: three getting-started steps, a
//! shortcut table, a few tips, and where the files live. Returns `true` when
//! the user clicks Close.

use super::{modal_scroll, text_button};
use eframe::egui;

pub fn show(ui: &mut egui::Ui) -> bool {
    ui.set_max_width(560.0);
    ui.heading("Help");
    ui.add_space(4.0);

    modal_scroll(ui).show(ui, |ui| {
        section(ui, "Getting started");
        bullets(
            ui,
            &[
                "Press + to add a project: pick a preset, choose the working \
                 directory, set the command. Env vars and a .env file are optional.",
                "Drag a card into the log area to open its log; the highlighted \
                 half shows where it splits (up to 4 logs). Open log arranges new panes \
                 in two rows; narrow windows show one log at a time. Clicking a card \
                 shows its log in the focused pane instead.",
                "Run / Stop live in the pane header; Restart is in More and the card's \
                 right-click menu. Stop waits up to 10 s for a graceful shutdown; \
                 press it again to force-quit.",
            ],
        );

        section(ui, "Shortcuts");
        shortcuts(
            ui,
            &[
                (
                    "Cmd/Ctrl+F",
                    "find / filter in the focused log (Esc closes)",
                ),
                ("Enter / Shift+Enter", "next / previous match"),
                ("Cmd/Ctrl+B", "hide or show the sidebar"),
                ("Cmd/Ctrl+1–9, 0", "switch workspace tab"),
                ("Cmd/Ctrl+W", "close the workspace tab"),
                ("Cmd/Ctrl+Enter", "save the project form"),
                ("Double-click a tab", "rename the workspace"),
            ],
        );

        section(ui, "Good to know");
        bullets(
            ui,
            &[
                "The port is injected as both PORT (Node) and SERVER_PORT (Spring \
                 Boot). If your framework reads neither, put it in the command or \
                 an env var. A port already in use, or shared by two projects, is \
                 flagged.",
                "Commands run through your login shell. If nvm or sdkman isn't \
                 found, set Shell to `zsh -lic` so ~/.zshrc is sourced.",
                "Click a project's :port to open localhost in the browser; the \
                 right-click menu also reveals the folder and copies the command.",
                "Logs: Follow tails the output; More contains Wrap and Clear output. A \
                 '↓ N new' chip appears when output arrives while scrolled up.",
                "Workspaces are per session; projects recovered from a previous \
                 run open automatically.",
            ],
        );

        section(ui, "Files");
        bullets(
            ui,
            &[
                "Projects: com.heonny.campfire/servers.toml in the OS app-config \
                 directory; running-state in running.json next to it.",
                "Uninstall: remove the app and delete that folder (or run \
                 scripts/uninstall-macos.sh / uninstall-windows.ps1).",
            ],
        );
    });

    ui.add_space(8.0);
    // A plain button: closing help is not a primary action, and the accent is
    // reserved for ones that are (Save).
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add(text_button("Close")).clicked()
    })
    .inner
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.strong(title);
    ui.add_space(2.0);
}

/// Short items, each on its own line behind a quiet bullet.
fn bullets(ui: &mut egui::Ui, items: &[&str]) {
    for item in items {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.weak("•");
            ui.label(*item);
        });
    }
}

/// Key on the left in monospace, what it does on the right: a table scans
/// far faster than the same facts buried in prose.
fn shortcuts(ui: &mut egui::Ui, rows: &[(&str, &str)]) {
    egui::Grid::new("help_shortcuts")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for (key, what) in rows {
                ui.label(egui::RichText::new(*key).monospace());
                ui.label(*what);
                ui.end_row();
            }
        });
}
