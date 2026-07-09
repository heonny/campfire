//! Detect a Node project from its `package.json`: which package manager runs it,
//! what scripts it exposes (as launch points), and a conventional port hint.
//!
//! This feeds the editor UI only — it does NOT touch the persisted
//! [`crate::model::ServerConfig`]. Point the editor at a directory and its
//! `package.json` scripts become selectable commands. Detection is best-effort:
//! a missing or malformed `package.json` yields `None`, never an error.

use serde::Deserialize;
use std::path::Path;

/// A Node package manager, detected from the project's `packageManager` field
/// or its lockfile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    /// The CLI binary name.
    pub fn as_str(self) -> &'static str {
        match self {
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Bun => "bun",
        }
    }

    /// The shell command that runs a named script, e.g. `pnpm run dev`.
    /// `<pm> run <script>` is valid across all four managers, so it stays uniform.
    pub fn run(self, script: &str) -> String {
        format!("{} run {script}", self.as_str())
    }

    /// Parse the leading token of a `packageManager` field (`"pnpm@10.15.1"`).
    fn from_field(value: &str) -> Option<Self> {
        match value.split('@').next().unwrap_or("").trim() {
            "npm" => Some(PackageManager::Npm),
            "pnpm" => Some(PackageManager::Pnpm),
            "yarn" => Some(PackageManager::Yarn),
            "bun" => Some(PackageManager::Bun),
            _ => None,
        }
    }

    /// Infer from a lockfile present in `dir`, most specific manager first.
    fn from_lockfile(dir: &Path) -> Option<Self> {
        const LOCKFILES: [(&str, PackageManager); 5] = [
            ("pnpm-lock.yaml", PackageManager::Pnpm),
            ("yarn.lock", PackageManager::Yarn),
            ("bun.lockb", PackageManager::Bun),
            ("bun.lock", PackageManager::Bun),
            ("package-lock.json", PackageManager::Npm),
        ];
        LOCKFILES
            .into_iter()
            .find(|(file, _)| dir.join(file).exists())
            .map(|(_, pm)| pm)
    }
}

/// A detected Node project: its manager, ordered scripts, and a port hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeProject {
    pub manager: PackageManager,
    /// `(name, raw command)` pairs in `package.json` declaration order.
    pub scripts: Vec<(String, String)>,
    /// Conventional dev port inferred from a recognized framework dependency.
    pub port_hint: Option<u16>,
}

/// The subset of `package.json` we read. Unknown fields are ignored and missing
/// ones default. `scripts` order is preserved via serde_json's `preserve_order`
/// feature (its `Map` is backed by an insertion-ordered index map).
#[derive(Deserialize, Default)]
struct PackageJson {
    #[serde(default)]
    scripts: serde_json::Map<String, serde_json::Value>,
    #[serde(default, rename = "packageManager")]
    package_manager: Option<String>,
    #[serde(default)]
    dependencies: serde_json::Map<String, serde_json::Value>,
    #[serde(default, rename = "devDependencies")]
    dev_dependencies: serde_json::Map<String, serde_json::Value>,
}

impl PackageJson {
    /// True if `name` appears in either dependency map.
    fn has_dep(&self, name: &str) -> bool {
        self.dependencies.contains_key(name) || self.dev_dependencies.contains_key(name)
    }

    /// A conventional dev port for a recognized framework dependency, if any.
    fn port_hint(&self) -> Option<u16> {
        if self.has_dep("next") || self.has_dep("react-scripts") {
            Some(3000)
        } else if self.has_dep("vite") {
            Some(5173)
        } else {
            None
        }
    }
}

/// Read and parse `<dir>/package.json`. Returns `None` when the file is absent
/// or unparseable — the editor then simply shows no scripts.
pub fn detect_node_project(dir: &Path) -> Option<NodeProject> {
    let text = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let pkg: PackageJson = serde_json::from_str(&text).ok()?;

    let scripts = pkg
        .scripts
        .iter()
        .filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_string())))
        .collect();

    let manager = pkg
        .package_manager
        .as_deref()
        .and_then(PackageManager::from_field)
        .or_else(|| PackageManager::from_lockfile(dir))
        .unwrap_or(PackageManager::Npm);

    Some(NodeProject {
        manager,
        scripts,
        port_hint: pkg.port_hint(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// A fresh, empty scratch directory unique to this test tag and process.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("campfire-proj-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn none_when_no_package_json() {
        let dir = scratch("empty");
        assert!(detect_node_project(&dir).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn none_when_malformed_json() {
        let dir = scratch("malformed");
        fs::write(dir.join("package.json"), "{ not valid json").unwrap();
        assert!(detect_node_project(&dir).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_manager_field_wins_over_lockfile() {
        let dir = scratch("field-wins");
        fs::write(
            dir.join("package.json"),
            r#"{"packageManager":"pnpm@10.15.1","scripts":{"dev":"next dev"}}"#,
        )
        .unwrap();
        // A conflicting lockfile that would otherwise say npm.
        fs::write(dir.join("package-lock.json"), "{}").unwrap();
        assert_eq!(
            detect_node_project(&dir).unwrap().manager,
            PackageManager::Pnpm
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn falls_back_to_lockfile_then_npm() {
        let dir = scratch("lockfile");
        fs::write(dir.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        fs::write(dir.join("yarn.lock"), "").unwrap();
        assert_eq!(
            detect_node_project(&dir).unwrap().manager,
            PackageManager::Yarn
        );
        // With no lockfile at all, default to npm.
        fs::remove_file(dir.join("yarn.lock")).unwrap();
        assert_eq!(
            detect_node_project(&dir).unwrap().manager,
            PackageManager::Npm
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scripts_preserve_declaration_order() {
        let dir = scratch("order");
        fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"dev":"next dev","build":"next build","start":"next start"}}"#,
        )
        .unwrap();
        let project = detect_node_project(&dir).unwrap();
        let names: Vec<&str> = project.scripts.iter().map(|(n, _)| n.as_str()).collect();
        // Declaration order, NOT alphabetical (which would be build, dev, start).
        assert_eq!(names, ["dev", "build", "start"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn next_dependency_hints_port_3000() {
        let dir = scratch("port-next");
        fs::write(
            dir.join("package.json"),
            r#"{"dependencies":{"next":"16.0.7"},"scripts":{"dev":"next dev"}}"#,
        )
        .unwrap();
        assert_eq!(detect_node_project(&dir).unwrap().port_hint, Some(3000));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn vite_dev_dependency_hints_port_5173_and_no_hint_otherwise() {
        let dir = scratch("port-vite");
        fs::write(
            dir.join("package.json"),
            r#"{"devDependencies":{"vite":"5.0.0"},"scripts":{"dev":"vite"}}"#,
        )
        .unwrap();
        assert_eq!(detect_node_project(&dir).unwrap().port_hint, Some(5173));

        fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"start":"node ."}}"#,
        )
        .unwrap();
        assert_eq!(detect_node_project(&dir).unwrap().port_hint, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_builds_uniform_command() {
        assert_eq!(PackageManager::Pnpm.run("dev"), "pnpm run dev");
        assert_eq!(PackageManager::Npm.run("build"), "npm run build");
        assert_eq!(PackageManager::Bun.run("start"), "bun run start");
    }

    #[test]
    fn non_string_script_values_are_skipped() {
        let dir = scratch("non-string");
        fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"dev":"next dev","weird":123}}"#,
        )
        .unwrap();
        let project = detect_node_project(&dir).unwrap();
        let names: Vec<&str> = project.scripts.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["dev"]); // the numeric-valued entry is dropped
        let _ = fs::remove_dir_all(&dir);
    }
}
