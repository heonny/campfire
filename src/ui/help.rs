//! Static help/usage content, shown in a modal. Returns `true` when the user
//! clicks Close.

use super::primary_button;
use eframe::egui;

pub fn show(ui: &mut egui::Ui) -> bool {
    ui.set_max_width(560.0);
    ui.heading("Campfire — help");
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .max_height(440.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            section(
                ui,
                "What it does",
                "Run and manage several local dev servers — start, stop, restart, \
                 watch colored logs, and catch port clashes.",
            );
            section(
                ui,
                "Add a server",
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
                "Logs",
                "Search filters the lines, 'follow' tails the bottom, and 'clear' \
                 empties the view. ANSI colors are rendered and lines are \
                 selectable.",
            );
            section(
                ui,
                "Where config is stored",
                "Servers are saved to your OS app-config directory under \
                 com.heonny.campfire/servers.toml.",
            );
        });

    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add(primary_button("Close")).clicked()
    })
    .inner
}

fn section(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.add_space(6.0);
    ui.strong(title);
    ui.label(body);
}
