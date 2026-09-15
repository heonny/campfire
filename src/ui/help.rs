//! Static help/usage content, shown in a modal. Returns `true` when the user
//! clicks Close.

use super::{modal_scroll, text_button};
use eframe::egui;

pub fn show(ui: &mut egui::Ui) -> bool {
    ui.set_max_width(560.0);
    ui.heading("Campfire — help");
    ui.add_space(4.0);

    modal_scroll(ui).show(ui, |ui| {
        section(
            ui,
            "What it does",
            "Run and manage several local dev servers — start, stop, restart, \
                 watch colored logs, and catch port clashes.",
        );
        section(
            ui,
            "Add a project",
            "Click + Add. Pick a preset (it fills the command and a default \
                 port), choose the working directory with Browse…, and set the \
                 command to run. Env vars and a .env file are optional.",
        );
        section(
            ui,
            "Ports",
            "The port you set is injected as both PORT (Node/Next) and \
                 SERVER_PORT (Spring Boot). If your framework reads neither, put \
                 the port in the command (e.g. --server.port=8093) or an env var. \
                 Campfire warns when a port is already in use or is used by two \
                 servers.",
        );
        section(
            ui,
            "Shell / PATH",
            "Commands run through your login shell. If a version manager \
                 (nvm, sdkman) isn't found, set the Shell field to `zsh -lic` so \
                 it sources ~/.zshrc.",
        );
        section(
            ui,
            "Workspaces",
            "Drag a project card into the log area to open its log — the \
                 highlighted half shows where it will split (up to 4 logs side \
                 by side). Drag a pane's title to rearrange, drag the gaps to \
                 resize, and × closes a pane. Tabs above bundle layouts into \
                 workspaces: + adds one, double-click renames, middle-click (or \
                 ×, or Cmd/Ctrl+W) closes, and Cmd/Ctrl+1–9 and 0 switch tabs \
                 (workspaces reset on relaunch). Clicking a card shows that log \
                 in the focused pane; right-click → Open log opens without \
                 dragging.",
        );
        section(
            ui,
            "Logs",
            "Press Cmd/Ctrl+F (or the search button) to open the find/grep box \
                 (Esc closes it): find highlights matches and steps between them, \
                 grep filters the lines. Along the bottom, 'follow' tails the output, \
                 'wrap' folds long lines, and 'clear' empties the view; when you \
                 scroll up, a '↓ N new' chip counts output you haven't seen. ANSI \
                 colors are rendered and lines are selectable.",
        );
        section(
            ui,
            "Shortcuts",
            "Cmd/Ctrl+B toggles the sidebar. Cmd/Ctrl+Enter saves the project \
                 form. Click a project's :port to open http://localhost:port; the \
                 right-click menu also reveals the working directory and copies the \
                 command. Stop waits up to 10 s for a graceful shutdown ('stopping'); \
                 press Stop again to force-quit.",
        );
        section(
            ui,
            "Where config is stored",
            "Servers are saved to your OS app-config directory under \
                 com.heonny.campfire/servers.toml; running-state lives next to \
                 it in running.json.",
        );
        section(
            ui,
            "Uninstall",
            "Removing the app leaves those two files behind. To wipe them, \
                 run scripts/uninstall-macos.sh (or uninstall-windows.ps1), or \
                 delete Campfire's saved-data folder(s) by hand — the README \
                 lists the exact path per OS.",
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

fn section(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.add_space(6.0);
    ui.strong(title);
    ui.label(body);
}
