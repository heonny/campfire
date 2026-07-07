//! The add/edit server form, rendered inside a modal.
//!
//! [`EditorForm`] holds the in-progress text fields; [`EditorForm::to_config`]
//! parses and validates them into a [`ServerConfig`], and [`show`] renders the
//! form and reports what the user did via [`EditorOutcome`].

use crate::model::{EnvVar, Preset, ServerConfig};
use eframe::egui;
use uuid::Uuid;

/// What the user did with the editor this frame.
pub enum EditorOutcome {
    None,
    Cancel,
    Save(ServerConfig),
    Delete(String),
}

/// Editable text state for one server. Text fields are parsed on save so the
/// user can type freely (and see validation errors) without partial values
/// leaking into the persisted model.
pub struct EditorForm {
    editing_id: Option<String>,
    name: String,
    preset: Preset,
    cwd: String,
    command: String,
    port: String,
    env_file: String,
    env: Vec<(String, String)>,
    shell: String,
    error: Option<String>,
}

impl EditorForm {
    /// A blank form for creating a new server.
    pub fn new_server() -> Self {
        Self {
            editing_id: None,
            name: String::new(),
            preset: Preset::Custom,
            cwd: String::new(),
            command: String::new(),
            port: String::new(),
            env_file: String::new(),
            env: Vec::new(),
            shell: String::new(),
            error: None,
        }
    }

    /// A form pre-filled from an existing server (its id is preserved on save).
    pub fn from_config(config: &ServerConfig) -> Self {
        Self {
            editing_id: Some(config.id.clone()),
            name: config.name.clone(),
            preset: config.preset,
            cwd: config.cwd.to_string_lossy().into_owned(),
            command: config.command.clone(),
            port: config.port.map(|p| p.to_string()).unwrap_or_default(),
            env_file: config
                .env_file
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            env: config
                .env
                .iter()
                .map(|e| (e.key.clone(), e.value.clone()))
                .collect(),
            shell: config.shell.clone().unwrap_or_default(),
            error: None,
        }
    }

    /// Overwrite command/port with a preset's defaults (invoked when the user
    /// picks a preset from the dropdown).
    fn apply_preset(&mut self, preset: Preset) {
        self.preset = preset;
        self.command = preset.default_command().to_string();
        self.port = preset.default_port().map(|p| p.to_string()).unwrap_or_default();
    }

    /// Parse and validate the form into a [`ServerConfig`], or return a
    /// user-facing error message.
    pub fn to_config(&self) -> Result<ServerConfig, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Name is required.".to_string());
        }
        let command = self.command.trim();
        if command.is_empty() {
            return Err("Command is required.".to_string());
        }
        let cwd = self.cwd.trim();
        if cwd.is_empty() {
            return Err("Working directory is required.".to_string());
        }
        let port = match self.port.trim() {
            "" => None,
            text => match text.parse::<u16>() {
                Ok(0) | Err(_) => {
                    return Err(format!("Port '{text}' must be a number between 1 and 65535."));
                }
                Ok(port) => Some(port),
            },
        };
        let env = self
            .env
            .iter()
            .filter(|(key, _)| !key.trim().is_empty())
            .map(|(key, value)| EnvVar { key: key.trim().to_string(), value: value.clone() })
            .collect();
        let env_file = non_empty(&self.env_file).map(Into::into);
        let shell = non_empty(&self.shell);

        Ok(ServerConfig {
            id: self.editing_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string()),
            name: name.to_string(),
            preset: self.preset,
            cwd: cwd.into(),
            command: command.to_string(),
            port,
            env_file,
            env,
            shell,
        })
    }

    /// An approximate rendering of the resolved invocation, for display.
    pub fn preview(&self) -> String {
        let mut prefix = String::new();
        if let Ok(port) = self.port.trim().parse::<u16>() {
            prefix.push_str(&format!("PORT={port} "));
        }
        for (key, value) in &self.env {
            if !key.trim().is_empty() {
                prefix.push_str(&format!("{}={} ", key.trim(), value));
            }
        }
        let shell = non_empty(&self.shell).unwrap_or_else(default_shell_display);
        format!("{prefix}{shell} '{}'", self.command.trim())
    }
}

fn non_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn default_shell_display() -> String {
    #[cfg(windows)]
    {
        "cmd /C".to_string()
    }
    #[cfg(unix)]
    {
        format!(
            "{} -lc",
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
        )
    }
}

/// Render the form and report the user's action.
pub fn show(ui: &mut egui::Ui, form: &mut EditorForm) -> EditorOutcome {
    let mut outcome = EditorOutcome::None;
    ui.set_min_width(480.0);
    ui.heading(if form.editing_id.is_some() {
        "Edit server"
    } else {
        "Add server"
    });
    ui.separator();

    egui::Grid::new("editor_grid")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut form.name);
            ui.end_row();

            ui.label("Preset");
            let mut chosen = form.preset;
            egui::ComboBox::from_id_salt("preset")
                .selected_text(form.preset.label())
                .show_ui(ui, |ui| {
                    for preset in Preset::ALL {
                        ui.selectable_value(&mut chosen, preset, preset.label());
                    }
                });
            if chosen != form.preset {
                form.apply_preset(chosen);
            }
            ui.end_row();

            ui.label("Working dir");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut form.cwd).desired_width(260.0));
                if ui.button("Browse…").clicked() {
                    let mut dialog = rfd::FileDialog::new();
                    if !form.cwd.trim().is_empty() {
                        dialog = dialog.set_directory(form.cwd.trim());
                    }
                    if let Some(path) = dialog.pick_folder() {
                        form.cwd = path.to_string_lossy().into_owned();
                    }
                }
            });
            ui.end_row();

            ui.label("Command");
            ui.text_edit_singleline(&mut form.command);
            ui.end_row();

            ui.label("Port");
            ui.text_edit_singleline(&mut form.port);
            ui.end_row();

            ui.label(".env file");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut form.env_file).desired_width(260.0));
                if ui.button("Browse…").clicked() {
                    let mut dialog = rfd::FileDialog::new();
                    if !form.cwd.trim().is_empty() {
                        dialog = dialog.set_directory(form.cwd.trim());
                    }
                    if let Some(path) = dialog.pick_file() {
                        form.env_file = path.to_string_lossy().into_owned();
                    }
                }
            });
            ui.end_row();

            ui.label("Shell");
            ui.text_edit_singleline(&mut form.shell);
            ui.end_row();
        });

    ui.separator();
    ui.label("Environment variables");
    let mut remove: Option<usize> = None;
    for (index, (key, value)) in form.env.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(key).desired_width(150.0).hint_text("KEY"));
            ui.label("=");
            ui.add(egui::TextEdit::singleline(value).desired_width(180.0).hint_text("value"));
            if ui.button("−").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        form.env.remove(index);
    }
    if ui.button("+ add variable").clicked() {
        form.env.push((String::new(), String::new()));
    }

    ui.separator();
    ui.label("Command preview");
    ui.monospace(form.preview());

    if let Some(error) = &form.error {
        let color = ui.visuals().error_fg_color;
        ui.colored_label(color, error);
    }

    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Save").clicked() {
            match form.to_config() {
                Ok(config) => outcome = EditorOutcome::Save(config),
                Err(message) => form.error = Some(message),
            }
        }
        if ui.button("Cancel").clicked() {
            outcome = EditorOutcome::Cancel;
        }
        if let Some(id) = &form.editing_id
            && ui.button("Delete").clicked()
        {
            outcome = EditorOutcome::Delete(id.clone());
        }
    });

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(name: &str, port: &str) -> EditorForm {
        EditorForm {
            editing_id: None,
            name: name.to_string(),
            preset: Preset::Custom,
            cwd: "/tmp".to_string(),
            command: "run".to_string(),
            port: port.to_string(),
            env_file: String::new(),
            env: Vec::new(),
            shell: String::new(),
            error: None,
        }
    }

    #[test]
    fn to_config_parses_and_empties_become_none() {
        let mut f = form("api", "3000");
        f.command = "npm run dev".to_string();
        f.env = vec![
            ("K".to_string(), "V".to_string()),
            ("  ".to_string(), "dropped".to_string()),
        ];
        let config = f.to_config().unwrap();
        assert_eq!(config.name, "api");
        assert_eq!(config.port, Some(3000));
        assert_eq!(config.env_file, None);
        assert_eq!(config.shell, None);
        assert_eq!(config.env.len(), 1); // blank-key row filtered out
        assert_eq!(config.env[0].key, "K");
        assert!(!config.id.is_empty());
    }

    #[test]
    fn to_config_rejects_bad_input() {
        assert!(form("", "8080").to_config().is_err()); // empty name
        assert!(form("ok", "abc").to_config().is_err()); // non-numeric port
        assert!(form("ok", "0").to_config().is_err()); // port 0
        assert!(form("ok", "").to_config().is_ok()); // empty port -> None, ok
        assert!(form("ok", "8080").to_config().is_ok());
    }

    #[test]
    fn editing_id_is_preserved() {
        let mut f = form("a", "");
        f.editing_id = Some("fixed-id".to_string());
        assert_eq!(f.to_config().unwrap().id, "fixed-id");
    }

    #[test]
    fn preview_includes_port_and_command() {
        let mut f = form("a", "3000");
        f.command = "npm run dev".to_string();
        let preview = f.preview();
        assert!(preview.contains("PORT=3000"), "got: {preview}");
        assert!(preview.contains("npm run dev"), "got: {preview}");
    }
}
