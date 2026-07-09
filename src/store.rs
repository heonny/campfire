//! Persistence for server configs: load/save a TOML document under the OS
//! application-config directory.
//!
//! The `*_from`/`*_to` functions take an explicit path and form the testable
//! core; [`load`]/[`save`] wrap them with the resolved default location.

use crate::model::ServerConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Current on-disk schema version. Reserved for forward migration; `load_from`
/// does not yet branch on it (there is only v1 to date).
pub const SCHEMA_VERSION: u32 = 1;

/// A document missing the `version` key defaults to the oldest known schema (1)
/// rather than the current one, so a versionless file would migrate through
/// every step once migrations exist.
fn default_version() -> u32 {
    1
}

/// The root persisted document: a schema version plus the list of servers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigDoc {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub servers: Vec<ServerConfig>,
}

impl Default for ConfigDoc {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            servers: Vec::new(),
        }
    }
}

/// Errors from loading or saving the config document.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not locate an OS config directory")]
    NoConfigDir,
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Default path: `servers.toml` inside the per-OS config directory for the
/// `com.heonny.campfire` application (resolved via `directories`).
pub fn config_path() -> Result<PathBuf, StoreError> {
    let dirs = directories::ProjectDirs::from("com", "heonny", "campfire")
        .ok_or(StoreError::NoConfigDir)?;
    Ok(dirs.config_dir().join("servers.toml"))
}

/// Load a document from an explicit path. A missing file is NOT an error — it
/// is the first-run case and yields the default (empty) document.
pub fn load_from(path: &Path) -> Result<ConfigDoc, StoreError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(toml::from_str(&text)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigDoc::default()),
        Err(source) => Err(StoreError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Save a document to an explicit path. Serializes to TOML and writes it
/// atomically (temp file + rename) via [`crate::fs_util::write_atomic`], so a
/// crash mid-write cannot leave a half-written (corrupt) config.
pub fn save_to(path: &Path, doc: &ConfigDoc) -> Result<(), StoreError> {
    let text = toml::to_string_pretty(doc)?;
    crate::fs_util::write_atomic(path, text.as_bytes()).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Load from the default OS config location.
pub fn load() -> Result<ConfigDoc, StoreError> {
    load_from(&config_path()?)
}

/// Save to the default OS config location.
pub fn save(doc: &ConfigDoc) -> Result<(), StoreError> {
    save_to(&config_path()?, doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Preset;

    /// A unique-per-test path under the OS temp dir (avoids a tempfile dep).
    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("campfire-test-{tag}-{}", std::process::id()));
        p.push("servers.toml");
        p
    }

    #[test]
    fn missing_file_loads_default() {
        let path = temp_path("missing");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        let doc = load_from(&path).unwrap();
        assert_eq!(doc, ConfigDoc::default());
        assert_eq!(doc.version, SCHEMA_VERSION);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let path = temp_path("roundtrip");
        let mut doc = ConfigDoc::default();
        doc.servers
            .push(ServerConfig::from_preset("api", "/srv/api", Preset::NextJs));
        doc.servers
            .push(ServerConfig::from_preset("db", "/srv/db", Preset::Custom));
        save_to(&path, &doc).unwrap();
        let back = load_from(&path).unwrap();
        assert_eq!(doc, back);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn save_leaves_no_temp_file() {
        let path = temp_path("atomic");
        save_to(&path, &ConfigDoc::default()).unwrap();
        let tmp = path.with_extension(format!("toml.tmp.{}", std::process::id()));
        assert!(!tmp.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_file_returns_parse_error() {
        let path = temp_path("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not valid toml {{{").unwrap();
        assert!(matches!(load_from(&path), Err(StoreError::Parse(_))));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn document_missing_version_defaults_to_one() {
        let path = temp_path("noversion");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "servers = []\n").unwrap();
        let doc = load_from(&path).unwrap();
        assert_eq!(doc.version, 1);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
