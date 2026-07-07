//! Campfire — local multi-server manager.
//!
//! Targets egui/eframe 0.35: the app entry point is `App::ui(&mut Ui, ..)` and
//! panels use the unified `egui::Panel` type shown into a `&mut Ui`.

mod model;
mod store;

use eframe::egui;
use model::ServerConfig;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Campfire")
            .with_inner_size([1024.0, 640.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Campfire",
        options,
        Box::new(|_cc| Ok(Box::new(CampfireApp::new()))),
    )
}

/// Root application state. Grows as features land (process control, logs).
struct CampfireApp {
    /// Persisted server definitions, loaded once at startup.
    servers: Vec<ServerConfig>,
    /// Index of the currently selected server, if any.
    selected: Option<usize>,
}

impl CampfireApp {
    fn new() -> Self {
        let servers = match store::load() {
            Ok(doc) => doc.servers,
            Err(err) => {
                eprintln!("campfire: could not load config, starting empty ({err})");
                Vec::new()
            }
        };
        Self { servers, selected: None }
    }
}

impl eframe::App for CampfireApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Campfire");
                ui.separator();
                ui.label("local multi-server manager");
            });
        });

        egui::Panel::left("server_list")
            .default_size(220.0)
            .show(ui, |ui| {
                ui.heading("Servers");
                ui.separator();
                if self.servers.is_empty() {
                    ui.weak("(등록된 서버 없음)");
                } else {
                    let mut clicked = None;
                    for (index, server) in self.servers.iter().enumerate() {
                        let label = match server.port {
                            Some(port) => format!("{}   :{port}", server.name),
                            None => server.name.clone(),
                        };
                        if ui.selectable_label(self.selected == Some(index), label).clicked() {
                            clicked = Some(index);
                        }
                    }
                    if clicked.is_some() {
                        self.selected = clicked;
                    }
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            match self.selected.and_then(|index| self.servers.get(index)) {
                Some(server) => {
                    ui.heading(&server.name);
                    ui.weak(&server.command);
                }
                None => {
                    ui.centered_and_justified(|ui| {
                        ui.weak("좌측에서 서버를 선택하거나 새로 추가하세요");
                    });
                }
            }
        });
    }
}
