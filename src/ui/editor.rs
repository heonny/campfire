//! The add/edit server form, rendered inside a modal.
//!
//! [`EditorForm`] holds the in-progress text fields; [`EditorForm::to_config`]
//! parses and validates them into a [`ServerConfig`], and [`show`] renders the
//! form and reports what the user did via [`EditorOutcome`].

use super::{primary_button, text_button, text_input};
use crate::gradle::{self, GradleProject};
use crate::model::{EnvVar, Preset, ServerConfig};
use crate::project::{NodeProject, detect_node_project};
use eframe::egui;
use std::path::{Path, PathBuf};
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
    /// Detected Node project for the current `cwd` (package manager + scripts),
    /// refreshed lazily by [`EditorForm::refresh_detection`]. Feeds the Scripts
    /// picker; never persisted into [`ServerConfig`].
    detected: Option<NodeProject>,
    /// The `cwd` value detection last ran for, so `package.json` is re-read only
    /// when the path actually changes — not on every frame.
    detected_for: String,
    /// Path to the Gradle build script feeding the Tasks picker (Spring Boot
    /// preset only). Auto-located under `cwd`, but user-overridable to point at a
    /// specific `build.gradle`. Transient UI state — never persisted.
    gradle_file: String,
    /// The last value auto-located into `gradle_file`. Lets `refresh_detection`
    /// tell an auto-fill apart from a manual Browse, so a user's override is
    /// preserved across incidental `cwd` edits rather than silently clobbered.
    gradle_file_auto: String,
    /// Detected Gradle project for the current `gradle_file`, refreshed lazily by
    /// [`EditorForm::refresh_gradle`].
    detected_gradle: Option<GradleProject>,
    /// The `gradle_file` value the parse last ran for, so the build script is
    /// re-read only when the path changes.
    detected_gradle_for: String,
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
            detected: None,
            detected_for: String::new(),
            gradle_file: String::new(),
            gradle_file_auto: String::new(),
            detected_gradle: None,
            detected_gradle_for: String::new(),
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
            detected: None,
            detected_for: String::new(),
            gradle_file: String::new(),
            gradle_file_auto: String::new(),
            detected_gradle: None,
            detected_gradle_for: String::new(),
        }
    }

    /// Overwrite command/port with a preset's defaults (invoked when the user
    /// picks a preset from the dropdown).
    fn apply_preset(&mut self, preset: Preset) {
        self.preset = preset;
        self.command = preset.default_command().to_string();
        self.port = preset
            .default_port()
            .map(|p| p.to_string())
            .unwrap_or_default();
        // Switching *to* Spring Boot must re-locate a build file even when `cwd`
        // is unchanged, so force detection to re-run next frame. Other preset
        // switches leave the (cwd-keyed) Node cache alone.
        if preset == Preset::SpringBoot {
            self.detected_for.clear();
        }
    }

    /// Re-read `package.json` when `cwd` changed since the last check. Cheap to
    /// call every frame: the filesystem is touched only when the path differs.
    /// For the Spring Boot preset it also auto-locates a Gradle build file under
    /// the new `cwd` — but only when the field is empty or still holds the last
    /// auto-located value, so a manual Browse is never silently clobbered.
    fn refresh_detection(&mut self) {
        if self.cwd.trim() == self.detected_for {
            return;
        }
        let cwd = self.cwd.trim().to_string();
        self.detected = if cwd.is_empty() {
            None
        } else {
            detect_node_project(Path::new(&cwd))
        };
        if self.preset == Preset::SpringBoot
            && (self.gradle_file.trim().is_empty() || self.gradle_file == self.gradle_file_auto)
        {
            let located = (!cwd.is_empty())
                .then(|| gradle::find_build_file(Path::new(&cwd)))
                .flatten()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.gradle_file = located.clone();
            self.gradle_file_auto = located;
        }
        self.detected_for = cwd;
    }

    /// Re-parse the Gradle build file when its path changed. Cheap to call every
    /// frame: the file is read only when `gradle_file` differs from last time.
    fn refresh_gradle(&mut self) {
        let file = self.gradle_file.trim().to_string();
        if file == self.detected_gradle_for {
            return;
        }
        self.detected_gradle = if file.is_empty() {
            None
        } else {
            gradle::detect_gradle_project(Path::new(&file))
        };
        self.detected_gradle_for = file;
    }

    /// Directory to open the Gradle-file browser in: the current file's folder
    /// when set, otherwise the working directory.
    fn gradle_dialog_dir(&self) -> Option<PathBuf> {
        let file = self.gradle_file.trim();
        if !file.is_empty()
            && let Some(parent) = Path::new(file).parent()
            && !parent.as_os_str().is_empty()
        {
            return Some(parent.to_path_buf());
        }
        let cwd = self.cwd.trim();
        (!cwd.is_empty()).then(|| PathBuf::from(cwd))
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
                    return Err(format!(
                        "Port '{text}' must be a number between 1 and 65535."
                    ));
                }
                Ok(port) => Some(port),
            },
        };
        let env = self
            .env
            .iter()
            .filter(|(key, _)| !key.trim().is_empty())
            .map(|(key, value)| EnvVar {
                key: key.trim().to_string(),
                value: value.clone(),
            })
            .collect();
        let env_file = non_empty(&self.env_file).map(Into::into);
        let shell = non_empty(&self.shell);

        Ok(ServerConfig {
            id: self
                .editing_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
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

/// A small bold section heading with a little breathing room under it.
fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).strong());
    ui.add_space(6.0);
}

/// Render the form and report the user's action.
pub fn show(ui: &mut egui::Ui, form: &mut EditorForm) -> EditorOutcome {
    let mut outcome = EditorOutcome::None;
    ui.set_min_width(460.0);
    ui.heading(if form.editing_id.is_some() {
        "Edit project"
    } else {
        "Add project"
    });
    ui.weak("Configure how this project runs and its environment.");
    ui.add_space(14.0);

    egui::Grid::new("editor_grid")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label("Name");
            text_input(ui, &mut form.name, "my-server", 280.0);
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
            let cwd_focused = ui
                .horizontal(|ui| {
                    let resp = text_input(ui, &mut form.cwd, "", 210.0);
                    if ui.add(text_button("Browse…")).clicked() {
                        let mut dialog = rfd::FileDialog::new();
                        if !form.cwd.trim().is_empty() {
                            dialog = dialog.set_directory(form.cwd.trim());
                        }
                        if let Some(path) = dialog.pick_folder() {
                            form.cwd = path.to_string_lossy().into_owned();
                            ui.ctx().request_repaint(); // render the Scripts row next frame
                        }
                    }
                    resp.has_focus()
                })
                .inner;
            ui.end_row();

            // Detect the project only while the path field isn't being typed in:
            // reading package.json on every keystroke could stall on a slow mount.
            // This still fires on open, after Browse, and when the field blurs.
            if !cwd_focused {
                form.refresh_detection();
            }

            // Gradle (Spring Boot preset): point at a build file — auto-located
            // under `cwd`, or Browse to a specific one — then pick a task from the
            // plugins it applies. Picking fills Command with `./gradlew <task>`.
            if form.preset == Preset::SpringBoot {
                ui.label("Gradle file");
                let gradle_focused = ui
                    .horizontal(|ui| {
                        let resp = text_input(ui, &mut form.gradle_file, "build.gradle", 210.0);
                        if ui.add(text_button("Browse…")).clicked() {
                            let mut dialog = rfd::FileDialog::new();
                            if let Some(dir) = form.gradle_dialog_dir() {
                                dialog = dialog.set_directory(dir);
                            }
                            if let Some(path) = dialog.pick_file() {
                                form.gradle_file = path.to_string_lossy().into_owned();
                                ui.ctx().request_repaint(); // parse + render Tasks next frame
                            }
                        }
                        resp.has_focus()
                    })
                    .inner;
                ui.end_row();

                // Re-parse only while the path field isn't being typed in, same as
                // the Node detection above.
                if !gradle_focused {
                    form.refresh_gradle();
                }

                let mut picked: Option<(String, Option<u16>)> = None;
                if let Some(project) = &form.detected_gradle
                    && !project.tasks.is_empty()
                {
                    ui.label("Tasks");
                    ui.horizontal(|ui| {
                        // Exact match highlights the picked task; hand-editing
                        // Command falls back to the placeholder (as with Scripts).
                        let current = project
                            .tasks
                            .iter()
                            .find(|t| form.command == gradle::task_command(&t.name))
                            .map(|t| t.name.clone());
                        egui::ComboBox::from_id_salt("gradle_tasks")
                            .selected_text(
                                current
                                    .clone()
                                    .unwrap_or_else(|| "Select a task…".to_string()),
                            )
                            .show_ui(ui, |ui| {
                                for t in &project.tasks {
                                    let selected = current.as_deref() == Some(t.name.as_str());
                                    if ui
                                        .selectable_label(
                                            selected,
                                            format!("{}  —  {}", t.name, t.description),
                                        )
                                        .clicked()
                                    {
                                        picked = Some((
                                            gradle::task_command(&t.name),
                                            project.port_hint,
                                        ));
                                    }
                                }
                            });
                        if !project.plugins.is_empty() {
                            ui.weak(format!("plugins: {}", project.plugins.join(", ")));
                        }
                    });
                    ui.end_row();
                }
                if let Some((command, port_hint)) = picked {
                    form.command = command;
                    if form.port.trim().is_empty()
                        && let Some(port) = port_hint
                    {
                        form.port = port.to_string();
                    }
                }
            }

            // Scripts: shown only when `cwd` holds a Node project. Picking one
            // fills Command with `<pm> run <script>` and, when Port is still
            // blank, seeds it from a recognized framework (next/vite).
            let mut picked: Option<(String, Option<u16>)> = None;
            if let Some(project) = &form.detected
                && !project.scripts.is_empty()
            {
                ui.label("Scripts");
                ui.horizontal(|ui| {
                    // Exact match only: highlights the picked script, but once the
                    // user hand-edits Command (e.g. appends flags) it intentionally
                    // falls back to the placeholder rather than guessing.
                    let current = project
                        .scripts
                        .iter()
                        .find(|(name, _)| form.command == project.manager.run(name))
                        .map(|(name, _)| name.clone());
                    egui::ComboBox::from_id_salt("scripts")
                        .selected_text(
                            current
                                .clone()
                                .unwrap_or_else(|| "Select a script…".to_string()),
                        )
                        .show_ui(ui, |ui| {
                            for (name, raw) in &project.scripts {
                                let selected = current.as_deref() == Some(name.as_str());
                                if ui
                                    .selectable_label(selected, format!("{name}  —  {raw}"))
                                    .clicked()
                                {
                                    picked = Some((project.manager.run(name), project.port_hint));
                                }
                            }
                        });
                    ui.weak(format!("via {}", project.manager.as_str()));
                });
                ui.end_row();
            }
            if let Some((command, port_hint)) = picked {
                form.command = command;
                if form.port.trim().is_empty()
                    && let Some(port) = port_hint
                {
                    form.port = port.to_string();
                }
            }

            ui.label("Command");
            text_input(ui, &mut form.command, "npm run dev", 280.0);
            ui.end_row();

            ui.label("Port");
            text_input(ui, &mut form.port, "3000", 100.0);
            ui.end_row();

            ui.label(".env file");
            ui.horizontal(|ui| {
                text_input(ui, &mut form.env_file, "", 210.0);
                if ui.add(text_button("Browse…")).clicked() {
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
            text_input(ui, &mut form.shell, "(default login shell)", 280.0);
            ui.end_row();
        });

    ui.add_space(16.0);
    section_label(ui, "Environment variables");
    let mut remove: Option<usize> = None;
    for (index, (key, value)) in form.env.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            text_input(ui, key, "KEY", 140.0);
            ui.label("=");
            text_input(ui, value, "value", 170.0);
            if ui.add(text_button("−")).clicked() {
                remove = Some(index);
            }
        });
        ui.add_space(4.0);
    }
    if let Some(index) = remove {
        form.env.remove(index);
    }
    if ui.add(text_button("+ add variable")).clicked() {
        form.env.push((String::new(), String::new()));
    }

    ui.add_space(16.0);
    section_label(ui, "Command preview");
    crate::theme::inset_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add(egui::Label::new(
            egui::RichText::new(form.preview()).monospace(),
        ));
    });

    if let Some(error) = &form.error {
        ui.add_space(6.0);
        ui.colored_label(ui.visuals().error_fg_color, error);
    }

    ui.add_space(20.0);
    ui.horizontal(|ui| {
        if let Some(id) = &form.editing_id {
            let delete =
                egui::Button::new(egui::RichText::new("Delete").color(ui.visuals().error_fg_color));
            if ui.add(delete).clicked() {
                outcome = EditorOutcome::Delete(id.clone());
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(primary_button("Save")).clicked() {
                match form.to_config() {
                    Ok(config) => outcome = EditorOutcome::Save(config),
                    Err(message) => form.error = Some(message),
                }
            }
            if ui.add(text_button("Cancel")).clicked() {
                outcome = EditorOutcome::Cancel;
            }
        });
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
            detected: None,
            detected_for: String::new(),
            gradle_file: String::new(),
            gradle_file_auto: String::new(),
            detected_gradle: None,
            detected_gradle_for: String::new(),
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

    /// Two sibling scratch dirs, each with a `build.gradle`, for exercising the
    /// Spring Boot auto-locate / manual-override state machine.
    fn gradle_dirs(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        use std::fs;
        let base =
            std::env::temp_dir().join(format!("campfire-editor-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (a, b) = (base.join("a"), base.join("b"));
        for dir in [&a, &b] {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join("build.gradle"), "plugins { id 'java' }").unwrap();
        }
        (a, b)
    }

    #[test]
    fn gradle_file_auto_follows_cwd_when_not_overridden() {
        let (a, b) = gradle_dirs("follow");
        let mut f = EditorForm::new_server();
        f.preset = Preset::SpringBoot;

        f.cwd = a.to_string_lossy().into_owned();
        f.refresh_detection();
        assert_eq!(f.gradle_file, a.join("build.gradle").to_string_lossy());

        // Untouched auto value tracks the new working directory.
        f.cwd = b.to_string_lossy().into_owned();
        f.refresh_detection();
        assert_eq!(f.gradle_file, b.join("build.gradle").to_string_lossy());

        let _ = std::fs::remove_dir_all(a.parent().unwrap());
    }

    #[test]
    fn gradle_file_manual_override_survives_cwd_change() {
        let (a, b) = gradle_dirs("override");
        let mut f = EditorForm::new_server();
        f.preset = Preset::SpringBoot;
        f.cwd = a.to_string_lossy().into_owned();
        f.refresh_detection();

        // User Browses to a specific module's build file.
        let manual = a.join("app/build.gradle").to_string_lossy().into_owned();
        f.gradle_file = manual.clone();

        // An incidental cwd edit must not clobber the manual override.
        f.cwd = b.to_string_lossy().into_owned();
        f.refresh_detection();
        assert_eq!(f.gradle_file, manual);

        let _ = std::fs::remove_dir_all(a.parent().unwrap());
    }
}
