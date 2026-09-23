//! The add/edit server form, rendered inside a modal.
//!
//! [`EditorForm`] holds the in-progress text fields; [`EditorForm::to_config`]
//! parses and validates them into a [`ServerConfig`], and [`show`] renders the
//! form and reports what the user did via [`EditorOutcome`].

mod detection;
mod form;
pub use form::show;
mod picker;
mod session;
use session::{EditorSnapshot, Field};

use crate::fs_util::{collapse_home, expand_home};
use crate::gradle::{self, GradleProject};
use crate::model::{EnvVar, Preset, ServerConfig};
use crate::project::{NodeProject, detect_node_project};
use std::path::PathBuf;
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
    detected_cargo: Option<crate::cargo_project::CargoProject>,
    detected_go: Option<crate::go_project::GoProject>,
    auto_name: Option<String>,
    auto_command: Option<String>,
    detection_note: String,
    cargo_query: String,
    /// The `cwd` value detection last ran for, so `package.json` is re-read only
    /// when the path actually changes — not on every frame.
    detected_for: String,
    /// Whether `cwd` names an existing directory (refreshed with detection), for
    /// the inline hint — a typo is otherwise only caught at launch.
    cwd_exists: bool,
    /// The port text last probed, and whether something was listening on it.
    /// Probed on change only (a bind per frame would be wasteful).
    port_checked: String,
    port_in_use: bool,
    /// Path to the Gradle build script feeding the Tasks picker.
    /// Auto-located under `cwd`, but user-overridable to point at a
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
    /// Snapshot used to keep a dirty form open when the modal backdrop is clicked.
    initial_snapshot: Option<EditorSnapshot>,
    discard_requested: bool,
    validation_attempted: bool,
    focus_error: bool,
    script_query: String,
    task_query: String,
    env_candidates: Vec<PathBuf>,
}

impl EditorForm {
    /// A blank form for creating a new server.
    pub fn new_server() -> Self {
        let mut form = Self {
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
            detected_cargo: None,
            detected_go: None,
            auto_name: Some(String::new()),
            auto_command: Some(String::new()),
            detection_note: String::new(),
            cargo_query: String::new(),
            detected_for: String::new(),
            cwd_exists: true,
            port_checked: String::new(),
            port_in_use: false,
            gradle_file: String::new(),
            gradle_file_auto: String::new(),
            detected_gradle: None,
            detected_gradle_for: String::new(),
            initial_snapshot: None,
            discard_requested: false,
            validation_attempted: false,
            focus_error: false,
            script_query: String::new(),
            task_query: String::new(),
            env_candidates: Vec::new(),
        };
        form.initial_snapshot = Some(form.snapshot());
        form
    }

    /// A form pre-filled from an existing server (its id is preserved on save).
    pub fn from_config(config: &ServerConfig) -> Self {
        let mut form = Self {
            editing_id: Some(config.id.clone()),
            name: config.name.clone(),
            preset: config.preset,
            cwd: collapse_home(&config.cwd),
            command: config.command.clone(),
            port: config.port.map(|p| p.to_string()).unwrap_or_default(),
            env_file: config
                .env_file
                .as_ref()
                .map(|p| collapse_home(p))
                .unwrap_or_default(),
            env: config
                .env
                .iter()
                .map(|e| (e.key.clone(), e.value.clone()))
                .collect(),
            shell: config.shell.clone().unwrap_or_default(),
            error: None,
            detected: None,
            detected_cargo: None,
            detected_go: None,
            auto_name: None,
            auto_command: None,
            detection_note: String::new(),
            cargo_query: String::new(),
            detected_for: String::new(),
            cwd_exists: true,
            port_checked: String::new(),
            port_in_use: false,
            gradle_file: String::new(),
            gradle_file_auto: String::new(),
            detected_gradle: None,
            detected_gradle_for: String::new(),
            initial_snapshot: None,
            discard_requested: false,
            validation_attempted: false,
            focus_error: false,
            script_query: String::new(),
            task_query: String::new(),
            env_candidates: Vec::new(),
        };
        form.initial_snapshot = Some(form.snapshot());
        form
    }

    /// Overwrite command/port with a preset's defaults (invoked when the user
    /// picks a preset from the dropdown).
    fn apply_preset(&mut self, preset: Preset) {
        self.auto_command = None;
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
            gradle::detect_gradle_project(&expand_home(&file))
        };
        self.detected_gradle_for = file;
    }

    /// Re-probe the port when its text changed. Only a well-formed port is
    /// probed; a malformed one reads as "not in use" (the parse hint covers it).
    // ponytail: probes 127.0.0.1 only, like the ready probe in running.rs.
    fn refresh_port(&mut self) {
        if self.port == self.port_checked {
            return;
        }
        self.port_checked = self.port.clone();
        self.port_in_use = self
            .port
            .trim()
            .parse::<u16>()
            .is_ok_and(|p| p != 0 && !crate::port::is_port_free(p));
    }

    /// The inline port hint as `(text, is_error)`: a parse problem, else who
    /// else has this port — another server's config, or something already
    /// listening (unless that is this very server, expected while it runs).
    fn port_hint(&self, servers: &[ServerConfig], self_running: bool) -> Option<(String, bool)> {
        let text = self.port.trim();
        if text.is_empty() {
            return None;
        }
        let port = match text.parse::<u16>() {
            Ok(p) if p != 0 => p,
            _ => return Some(("must be 1–65535".to_owned(), true)),
        };
        let other = servers
            .iter()
            .find(|s| s.port == Some(port) && Some(s.id.as_str()) != self.editing_id.as_deref());
        if let Some(other) = other {
            return Some((format!("also used by '{}'", other.name), false));
        }
        if self.port_in_use && !self_running {
            return Some(("already in use on this machine".to_owned(), false));
        }
        None
    }

    pub fn editing_id(&self) -> Option<&str> {
        self.editing_id.as_deref()
    }

    /// Directory to open the Gradle-file browser in: the current file's folder
    /// when set, otherwise the working directory.
    fn gradle_dialog_dir(&self) -> Option<PathBuf> {
        let file = self.gradle_file.trim();
        if !file.is_empty()
            && let Some(parent) = expand_home(file).parent()
            && !parent.as_os_str().is_empty()
        {
            return Some(parent.to_path_buf());
        }
        let cwd = self.cwd.trim();
        (!cwd.is_empty()).then(|| expand_home(cwd))
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
        let cwd_path = expand_home(cwd);
        if !cwd_path.is_dir() {
            return Err(format!("Working directory '{cwd}' doesn't exist."));
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
        let env_file = non_empty(&self.env_file).map(|p| expand_home(&p));
        let shell = non_empty(&self.shell);

        Ok(ServerConfig {
            id: self
                .editing_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            name: name.to_string(),
            preset: self.preset,
            cwd: cwd_path,
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
        for (key, _) in &self.env {
            if !key.trim().is_empty() {
                prefix.push_str(&format!("{}=••• ", key.trim()));
            }
        }
        if let Ok(port) = self.port.trim().parse::<u16>()
            && port > 0
        {
            prefix.push_str(&format!("PORT={port} SERVER_PORT={port} "));
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod detection_tests;

#[cfg(test)]
mod form_tests;

#[cfg(test)]
mod go_tests;
