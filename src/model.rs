//! Data model for a managed server and its presets.
//!
//! [`ServerConfig`] is the persisted definition of one server (see the `store`
//! module for persistence). Runtime state — status, pid, log buffer — lives
//! elsewhere and is intentionally NOT part of this serialized model.
#![allow(dead_code)] // Public API wired incrementally; some items land in later steps (spawn, UI).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// A framework preset. Picking one pre-fills a server's command, port, and
/// typical environment in the UI; every field stays user-editable afterward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    SpringBoot,
    Flink,
    NextJs,
    Go,
    #[default]
    Custom,
}

impl Preset {
    /// Every preset, in display order — for rendering the picker.
    pub const ALL: [Preset; 5] = [
        Preset::SpringBoot,
        Preset::Flink,
        Preset::NextJs,
        Preset::Go,
        Preset::Custom,
    ];

    /// The shell command a freshly-picked preset suggests as a starting point.
    pub fn default_command(self) -> &'static str {
        match self {
            Preset::SpringBoot => "./gradlew bootRun",
            Preset::Flink => "./bin/start-cluster.sh",
            Preset::NextJs => "npm run dev",
            Preset::Go => "go run .",
            Preset::Custom => "",
        }
    }

    /// Conventional default port for the preset, if it has one.
    pub fn default_port(self) -> Option<u16> {
        match self {
            Preset::SpringBoot => Some(8080),
            Preset::Flink => Some(8081),
            Preset::NextJs => Some(3000),
            Preset::Go => Some(8080),
            Preset::Custom => None,
        }
    }

    /// Human-readable label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            Preset::SpringBoot => "Java Spring Boot",
            Preset::Flink => "Java Flink",
            Preset::NextJs => "Node Next.js",
            Preset::Go => "Go",
            Preset::Custom => "Custom",
        }
    }
}

/// A single environment-variable override applied when launching the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

/// The persisted definition of one managed server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Stable identity, generated once and preserved across edits/renames.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub preset: Preset,
    /// Working directory the command runs in (e.g. the project root).
    pub cwd: PathBuf,
    /// Editable shell command. A preset seeds it; the user may change it freely.
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Optional path to a `.env` file loaded before `env` overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_file: Option<PathBuf>,
    /// Inline environment overrides, layered on top of `env_file`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<EnvVar>,
    /// Optional shell invocation prefix that runs `command`, e.g. `zsh -lic` to
    /// source `.zshrc` for nvm-managed tools. When None, the platform default is
    /// used (`$SHELL -lc` on Unix, `cmd /C` on Windows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
}

impl ServerConfig {
    /// Create a new server seeded from `preset`, with a freshly generated id.
    pub fn from_preset(name: impl Into<String>, cwd: impl Into<PathBuf>, preset: Preset) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            preset,
            cwd: cwd.into(),
            command: preset.default_command().to_owned(),
            port: preset.default_port(),
            env_file: None,
            env: Vec::new(),
            shell: None,
        }
    }

    /// Validate the user-editable fields, returning every problem found
    /// (an empty vec means the config is valid). Filesystem existence of
    /// `cwd`/`env_file` is a launch-time concern, checked separately.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        if self.name.trim().is_empty() {
            errors.push(ValidationError::EmptyName);
        }
        if self.command.trim().is_empty() {
            errors.push(ValidationError::EmptyCommand);
        }
        if self.cwd.as_os_str().is_empty() {
            errors.push(ValidationError::EmptyCwd);
        }
        if self.port == Some(0) {
            errors.push(ValidationError::InvalidPort);
        }
        errors
    }
}

/// A validation problem on a [`ServerConfig`] field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    EmptyName,
    EmptyCommand,
    EmptyCwd,
    InvalidPort,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_preset_seeds_command_and_port() {
        let s = ServerConfig::from_preset("api", "/srv/api", Preset::NextJs);
        assert_eq!(s.command, "npm run dev");
        assert_eq!(s.port, Some(3000));
        assert_eq!(s.preset, Preset::NextJs);
        assert!(!s.id.is_empty());
    }

    #[test]
    fn custom_preset_has_no_defaults() {
        let s = ServerConfig::from_preset("x", "/tmp", Preset::Custom);
        assert_eq!(s.command, "");
        assert_eq!(s.port, None);
    }

    #[test]
    fn validate_flags_empty_name_and_command() {
        let mut s = ServerConfig::from_preset("", "/tmp", Preset::Go);
        s.command = "   ".into();
        let errs = s.validate();
        assert!(errs.contains(&ValidationError::EmptyName));
        assert!(errs.contains(&ValidationError::EmptyCommand));
    }

    #[test]
    fn validate_ok_for_complete_config() {
        let s = ServerConfig::from_preset("api", "/srv/api", Preset::Go);
        assert!(s.validate().is_empty());
    }

    #[test]
    fn roundtrips_through_toml() {
        let mut s = ServerConfig::from_preset("api", "/srv/api", Preset::SpringBoot);
        s.env.push(EnvVar {
            key: "PROFILE".into(),
            value: "dev".into(),
        });
        let text = toml::to_string(&s).unwrap();
        let back: ServerConfig = toml::from_str(&text).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn preset_serializes_as_kebab_string() {
        let s = ServerConfig::from_preset("api", "/srv/api", Preset::NextJs);
        let text = toml::to_string(&s).unwrap();
        assert!(text.contains("preset = \"next-js\""), "got:\n{text}");
    }

    #[test]
    fn port_none_omits_key() {
        let s = ServerConfig::from_preset("api", "/srv/api", Preset::Custom);
        assert_eq!(s.port, None);
        let text = toml::to_string(&s).unwrap();
        assert!(
            !text.contains("port"),
            "port key should be omitted when None:\n{text}"
        );
    }

    #[test]
    fn windows_style_path_roundtrips() {
        let mut s = ServerConfig::from_preset("api", r"C:\Users\dev\api", Preset::Go);
        s.env_file = Some(r"C:\Users\dev\api\.env".into());
        let text = toml::to_string(&s).unwrap();
        let back: ServerConfig = toml::from_str(&text).unwrap();
        assert_eq!(s, back);
        assert_eq!(back.cwd, std::path::PathBuf::from(r"C:\Users\dev\api"));
    }
}
