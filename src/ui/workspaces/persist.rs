//! Best-effort persistence of the workspace set to `workspaces.json` in the
//! per-OS data dir (next to `running.json`). Only the layout is saved — names,
//! trees, the active tab — view state (search / follow / scroll, focus) is
//! transient. A failed save risks nothing but a stale layout next launch, so
//! errors are logged, not surfaced.

use super::{MAX_WORKSPACES, Workspace, Workspaces};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Current on-disk schema version (reserved for forward migration).
const SCHEMA_VERSION: u32 = 1;

/// The persisted document.
#[derive(Serialize, Deserialize)]
struct WorkspacesDoc {
    version: u32,
    active: usize,
    next_id: u64,
    workspaces: Vec<WorkspaceDoc>,
}

#[derive(Serialize, Deserialize)]
struct WorkspaceDoc {
    id: u64,
    name: String,
    tree: egui_tiles::Tree<String>,
}

/// Default path: `workspaces.json` in the per-OS data dir — UI layout state,
/// not user-editable config, so it lives beside `running.json`.
fn state_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "heonny", "campfire")?;
    Some(dirs.data_local_dir().join("workspaces.json"))
}

impl Workspaces {
    /// Load the saved workspace set, dropping panes whose server id is no
    /// longer in `known_ids` (deleted while the app was closed). Falls back to
    /// the default single empty workspace when nothing (valid) is saved.
    pub fn load(known_ids: &[String]) -> Self {
        state_path()
            .and_then(|path| load_from(&path, known_ids))
            .unwrap_or_default()
    }

    /// Persist the current set, best-effort.
    pub fn save(&self) {
        let Some(path) = state_path() else {
            return;
        };
        if let Err(err) = save_to(&path, self) {
            eprintln!("campfire: could not persist workspaces ({err})");
        }
    }

    fn to_doc(&self) -> WorkspacesDoc {
        WorkspacesDoc {
            version: SCHEMA_VERSION,
            active: self.active,
            next_id: self.next_id,
            workspaces: self
                .list
                .iter()
                .map(|ws| WorkspaceDoc {
                    id: ws.id,
                    name: ws.name.clone(),
                    tree: ws.tree.clone(),
                })
                .collect(),
        }
    }

    fn from_doc(doc: WorkspacesDoc, known_ids: &[String]) -> Self {
        let mut list: Vec<Workspace> = doc
            .workspaces
            .into_iter()
            .take(MAX_WORKSPACES)
            .map(|d| {
                let mut ws = Workspace {
                    id: d.id,
                    name: d.name,
                    tree: d.tree,
                    views: HashMap::new(),
                    focused: None,
                };
                ws.prune_unknown(known_ids);
                ws
            })
            .collect();
        if list.is_empty() {
            return Self::new();
        }
        let max_id = list.iter().map(|ws| ws.id).max().unwrap_or(0);
        let active = doc.active.min(list.len() - 1);
        // Focus the first pane of the active workspace so the sidebar highlight
        // starts somewhere sensible.
        if let Some(ws) = list.get_mut(active) {
            ws.focused = ws.open_ids().first().cloned();
        }
        Self {
            active,
            // Never reuse an id smaller than any loaded one (or the default 2).
            next_id: doc.next_id.max(max_id + 1).max(2),
            list,
            renaming: None,
            dirty: false,
            tree_snapshot: None,
        }
    }
}

impl Default for Workspaces {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    /// Remove panes whose server id isn't in `known_ids` (config changed while
    /// the layout was on disk).
    fn prune_unknown(&mut self, known_ids: &[String]) {
        loop {
            let stale = self
                .pane_tiles()
                .into_iter()
                .find(|(_, id)| !known_ids.contains(id));
            match stale {
                Some((tile, _)) => {
                    self.tree.remove_recursively(tile);
                }
                None => break,
            }
        }
        self.fix_focus_and_reset();
    }
}

fn save_to(path: &Path, wss: &Workspaces) -> std::io::Result<()> {
    let text = serde_json::to_string(&wss.to_doc())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    crate::fs_util::write_atomic(path, text.as_bytes())
}

/// `None` on a missing or unreadable file (both mean "start fresh").
fn load_from(path: &Path, known_ids: &[String]) -> Option<Workspaces> {
    let text = std::fs::read_to_string(path).ok()?;
    let doc: WorkspacesDoc = serde_json::from_str(&text).ok()?;
    Some(Workspaces::from_doc(doc, known_ids))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("campfire-ws-{tag}-{}", std::process::id()));
        p.push("workspaces.json");
        p
    }

    fn sample() -> Workspaces {
        let mut wss = Workspaces::new();
        wss.active_mut().open_auto("a");
        wss.active_mut().open_auto("b");
        wss.add();
        wss.list[1].name = "backend".to_owned();
        wss.active_mut().open_auto("c");
        wss
    }

    #[test]
    fn save_then_load_roundtrips_layout() {
        let path = temp_path("roundtrip");
        let wss = sample();
        save_to(&path, &wss).unwrap();

        let known = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let back = load_from(&path, &known).expect("loads");
        assert_eq!(back.list.len(), 2);
        assert_eq!(back.list[0].open_ids(), ["a", "b"]);
        assert_eq!(back.list[1].open_ids(), ["c"]);
        assert_eq!(back.list[1].name, "backend");
        assert_eq!(back.active, 1);
        assert_eq!(back.next_id, wss.next_id);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn load_prunes_servers_no_longer_in_config() {
        let path = temp_path("prune");
        save_to(&path, &sample()).unwrap();

        // "b" and "c" were deleted while the app was closed.
        let known = vec!["a".to_owned()];
        let back = load_from(&path, &known).expect("loads");
        assert_eq!(back.list[0].open_ids(), ["a"]);
        assert!(back.list[1].open_ids().is_empty());
        assert!(back.list[1].tree.is_empty(), "paneless tree resets clean");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_or_corrupt_file_falls_back() {
        let path = temp_path("missing");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert!(load_from(&path, &[]).is_none());

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json {{{").unwrap();
        assert!(load_from(&path, &[]).is_none());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn structural_ops_mark_dirty_and_take_consumes() {
        let mut wss = Workspaces::new();
        assert!(!wss.take_dirty());
        wss.add();
        assert!(wss.take_dirty());
        assert!(!wss.take_dirty(), "take consumes the flag");
        wss.close_server_everywhere("nope");
        assert!(wss.take_dirty());
    }
}
