//! Read local Cargo targets without running build scripts or resolving dependencies.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct CargoProject {
    pub commands: Vec<(String, String)>,
    pub default_command: Option<String>,
    pub note: Option<&'static str>,
}

#[derive(Deserialize)]
struct Manifest {
    package: Option<Package>,
    workspace: Option<toml::Value>,
    #[serde(default)]
    bin: Vec<Binary>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Package {
    name: String,
    default_run: Option<String>,
    autobins: Option<bool>,
    edition: Option<toml::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Binary {
    name: String,
    path: Option<PathBuf>,
    #[serde(default)]
    required_features: Vec<String>,
}

pub fn detect(dir: &Path) -> Option<CargoProject> {
    let text = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let manifest: Manifest = toml::from_str(&text).ok()?;
    let Some(package) = manifest.package else {
        return Some(CargoProject {
            commands: Vec::new(),
            default_command: None,
            note: Some("Cargo workspace: choose a package folder or enter cargo run -p <package>."),
        });
    };
    let (binaries, excluded_target) = collect_binaries(dir, &package, manifest.bin);
    let run = if manifest.workspace.is_some() && safe_target_name(&package.name) {
        format!("cargo run -p {}", package.name)
    } else {
        "cargo run".to_owned()
    };
    let commands: Vec<_> = binaries
        .keys()
        .filter(|name| safe_target_name(name))
        .map(|name| (name.clone(), format!("{run} --bin {name}")))
        .collect();
    let default_command = match package.default_run {
        Some(name) => commands
            .iter()
            .find(|(n, _)| n == &name)
            .map(|(_, cmd)| cmd.clone()),
        None if commands.len() == 1 && binaries.len() == 1 && !excluded_target => {
            if manifest.workspace.is_some() {
                Some(commands[0].1.clone())
            } else {
                Some("cargo run".into())
            }
        }
        None => None,
    };
    let note = if commands.is_empty() {
        Some(
            "No runnable Cargo binary found. Choose a binary package or enter a command with the required features.",
        )
    } else if default_command.is_none() {
        Some("Choose a Cargo binary below; no unambiguous default was found.")
    } else {
        None
    };
    Some(CargoProject {
        commands,
        default_command,
        note,
    })
}

fn collect_binaries(
    dir: &Path,
    package: &Package,
    explicit: Vec<Binary>,
) -> (BTreeMap<String, PathBuf>, bool) {
    let inferred = infer_binaries(dir, &package.name);
    // Edition 2015 disables discovery when explicit targets are present.
    let legacy = package
        .edition
        .as_ref()
        .is_none_or(|e| e.as_str() == Some("2015"));
    let mut binaries = if package.autobins.unwrap_or(!legacy || explicit.is_empty()) {
        inferred.clone()
    } else {
        BTreeMap::new()
    };
    let mut excluded_target = false;
    for binary in explicit {
        let path = binary
            .path
            .map(|p| dir.join(p))
            .or_else(|| inferred.get(&binary.name).cloned());
        binaries.remove(&binary.name);
        if let Some(path) = &path {
            binaries.retain(|_, inferred_path| inferred_path != path);
        }
        // Feature activation is a user choice, not an automatic launch default.
        if binary.required_features.is_empty()
            && let Some(path) = path.filter(|p| p.is_file())
        {
            binaries.insert(binary.name, path);
        } else {
            excluded_target = true;
        }
    }
    (binaries, excluded_target)
}

fn safe_target_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn infer_binaries(dir: &Path, package: &str) -> BTreeMap<String, PathBuf> {
    let mut binaries = BTreeMap::new();
    let main = dir.join("src/main.rs");
    if main.is_file() {
        binaries.insert(package.to_owned(), main);
    }
    let Ok(entries) = std::fs::read_dir(dir.join("src/bin")) else {
        return binaries;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let source = if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            path.clone()
        } else if path.is_dir() && path.join("main.rs").is_file() {
            path.join("main.rs")
        } else {
            continue;
        };
        let name = if path.is_dir() {
            path.file_name()
        } else {
            path.file_stem()
        };
        if let Some(name) = name.and_then(|name| name.to_str()) {
            binaries.insert(name.to_owned(), source);
        }
    }
    binaries
}
