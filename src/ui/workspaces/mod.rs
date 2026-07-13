//! Workspaces: tab bundles of log panes. Each workspace is a named egui_tiles
//! split tree over server ids — up to [`MAX_PANES`] logs side by side — and the
//! app keeps up to [`MAX_WORKSPACES`] workspaces switched by a tab strip above
//! the dock. Log DATA stays in the shared `running` map; a workspace only holds
//! layout plus its own per-server [`LogView`] state, so the same server can be
//! open in several workspaces with independent search/follow/scroll.
//!
//! The struct is mutable UI state owned by the app (like the old single
//! `LogView`): tile trees are mutated in place by egui_tiles during rendering
//! (internal pane drags / resizes), so the dock needs `&mut` access. Process
//! lifecycle operations still go through [`Action`]s.

mod dock;
mod tabs;

use crate::ui::log_view::LogView;
use crate::ui::{Action, View};
use eframe::egui;
use egui_tiles::{Tile, TileId, Tree};
use std::collections::HashMap;

/// A workspace shows at most this many log panes at once.
pub const MAX_PANES: usize = 4;
/// At most this many workspaces (tabs).
pub const MAX_WORKSPACES: usize = 100;

/// One workspace: a named split view over server ids.
pub struct Workspace {
    /// Monotonic and stable across closes. egui tree/widget ids key off it, so
    /// two workspaces never share layout or scroll state.
    id: u64,
    name: String,
    /// The split layout; each pane's payload is a server id.
    tree: Tree<String>,
    /// Per-server log view state (search / follow / scroll), for this workspace.
    views: HashMap<String, LogView>,
    /// The focused pane's server id (accent border + sidebar highlight).
    focused: Option<String>,
}

impl Workspace {
    fn new(id: u64, name: String) -> Self {
        Self {
            id,
            name,
            tree: Tree::empty(Self::tree_id(id)),
            views: HashMap::new(),
            focused: None,
        }
    }

    /// The egui id of this workspace's tile tree (must be globally unique).
    fn tree_id(ws_id: u64) -> egui::Id {
        egui::Id::new(("logs_dock", ws_id))
    }

    /// Server ids with an open pane, in tree order. Walks from the root so
    /// detached tiles never count.
    pub fn open_ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(root) = self.tree.root {
            collect_panes(&self.tree, root, &mut out);
        }
        out
    }

    pub fn is_open(&self, server_id: &str) -> bool {
        self.open_ids().iter().any(|s| s == server_id)
    }

    pub fn focused(&self) -> Option<&str> {
        self.focused.as_deref()
    }

    /// Focus `server_id`'s pane if it is open in this workspace.
    pub fn focus(&mut self, server_id: &str) {
        if self.is_open(server_id) {
            self.focused = Some(server_id.to_owned());
        }
    }

    /// Open `server_id` at an automatic position: appended to the root
    /// container, or wrapping a single root pane in a horizontal split. Already
    /// open → just focus (one pane per server per workspace). Returns a
    /// user-facing notice when the workspace is full.
    pub fn open_auto(&mut self, server_id: &str) -> Option<&'static str> {
        if self.is_open(server_id) {
            self.focused = Some(server_id.to_owned());
            return None;
        }
        if self.open_ids().len() >= MAX_PANES {
            return Some("A workspace shows at most 4 logs — close one first.");
        }
        match self.tree.root {
            None => {
                self.tree =
                    Tree::new_horizontal(Self::tree_id(self.id), vec![server_id.to_owned()]);
            }
            Some(root) => {
                let pane = self.tree.tiles.insert_pane(server_id.to_owned());
                if matches!(self.tree.tiles.get(root), Some(Tile::Container(_))) {
                    // Append at the end of the existing root container
                    // (move_tile_to_container clamps the index).
                    self.tree.move_tile_to_container(pane, root, usize::MAX, false);
                } else {
                    // Root is a single pane: wrap both in a horizontal split.
                    let wrap = self.tree.tiles.insert_horizontal_tile(vec![root, pane]);
                    self.tree.root = Some(wrap);
                }
            }
        }
        self.focused = Some(server_id.to_owned());
        None
    }

    /// Close `server_id`'s pane here (if open), dropping its view state.
    pub fn close_server(&mut self, server_id: &str) {
        while let Some(tile) = self.find_pane(server_id) {
            self.tree.remove_recursively(tile);
        }
        self.views.remove(server_id);
        self.fix_focus_and_reset();
    }

    /// The tile currently showing `server_id`, reachable from the root.
    fn find_pane(&self, server_id: &str) -> Option<TileId> {
        fn walk(tree: &Tree<String>, tile: TileId, server_id: &str) -> Option<TileId> {
            match tree.tiles.get(tile)? {
                Tile::Pane(s) => (s == server_id).then_some(tile),
                Tile::Container(c) => c
                    .children_vec()
                    .into_iter()
                    .find_map(|child| walk(tree, child, server_id)),
            }
        }
        walk(&self.tree, self.tree.root?, server_id)
    }

    /// Re-point the focus at an open pane (or none), and swap a paneless tree
    /// for a clean empty one so stale containers don't linger and the
    /// placeholder shows again.
    fn fix_focus_and_reset(&mut self) {
        let open = self.open_ids();
        if open.is_empty() {
            self.tree = Tree::empty(Self::tree_id(self.id));
            self.focused = None;
            return;
        }
        let focus_gone = self
            .focused
            .as_deref()
            .is_none_or(|f| !open.iter().any(|s| s == f));
        if focus_gone {
            self.focused = open.first().cloned();
        }
    }
}

fn collect_panes(tree: &Tree<String>, tile: TileId, out: &mut Vec<String>) {
    match tree.tiles.get(tile) {
        Some(Tile::Pane(server)) => out.push(server.clone()),
        Some(Tile::Container(container)) => {
            for child in container.children_vec() {
                collect_panes(tree, child, out);
            }
        }
        None => {}
    }
}

/// The workspace set: the tabs, which one is active, and transient strip state.
pub struct Workspaces {
    list: Vec<Workspace>,
    active: usize,
    /// Next workspace number; monotonic, never reused after a close.
    next_id: u64,
    /// In-progress tab rename: (workspace index, edit buffer).
    renaming: Option<(usize, String)>,
}

impl Workspaces {
    pub fn new() -> Self {
        Self {
            list: vec![Workspace::new(1, "Workspace 1".to_owned())],
            active: 0,
            next_id: 2,
            renaming: None,
        }
    }

    pub fn active(&self) -> &Workspace {
        &self.list[self.active]
    }

    pub fn active_mut(&mut self) -> &mut Workspace {
        &mut self.list[self.active]
    }

    /// Append (and switch to) a new empty workspace; no-op at the cap.
    fn add(&mut self) {
        if self.list.len() >= MAX_WORKSPACES {
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.list.push(Workspace::new(id, format!("Workspace {id}")));
        self.active = self.list.len() - 1;
    }

    /// Close the workspace at `index`. The last one is replaced by a fresh
    /// empty workspace, so there is always at least one.
    fn close(&mut self, index: usize) {
        if index >= self.list.len() {
            return;
        }
        self.list.remove(index);
        if self.list.is_empty() {
            let id = self.next_id;
            self.next_id += 1;
            self.list.push(Workspace::new(id, format!("Workspace {id}")));
            self.active = 0;
            return;
        }
        if index < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.list.len() - 1);
    }

    /// Drop `server_id`'s panes from every workspace (server deleted).
    pub fn close_server_everywhere(&mut self, server_id: &str) {
        for ws in &mut self.list {
            ws.close_server(server_id);
        }
    }

    /// Render the tab strip and the active workspace's dock.
    pub fn show(&mut self, ui: &mut egui::Ui, view: &View, action: &mut Option<Action>) {
        tabs::strip(ui, self);
        ui.add_space(8.0);
        dock::show_active(ui, self, view, action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new(9, "test".to_owned())
    }

    #[test]
    fn open_auto_first_pane_then_appends_in_order() {
        let mut w = ws();
        assert!(w.open_auto("a").is_none());
        assert_eq!(w.open_ids(), ["a"]);
        assert!(w.open_auto("b").is_none());
        assert!(w.open_auto("c").is_none());
        assert_eq!(w.open_ids(), ["a", "b", "c"]);
        assert_eq!(w.focused(), Some("c"));
    }

    #[test]
    fn open_auto_duplicate_focuses_without_adding() {
        let mut w = ws();
        w.open_auto("a");
        w.open_auto("b");
        assert!(w.open_auto("a").is_none());
        assert_eq!(w.open_ids(), ["a", "b"]); // no duplicate pane
        assert_eq!(w.focused(), Some("a"));
    }

    #[test]
    fn open_auto_rejects_past_the_pane_cap() {
        let mut w = ws();
        for id in ["a", "b", "c", "d"] {
            assert!(w.open_auto(id).is_none());
        }
        assert!(w.open_auto("e").is_some(), "5th open must be rejected");
        assert_eq!(w.open_ids().len(), MAX_PANES);
    }

    #[test]
    fn close_server_moves_focus_and_resets_when_empty() {
        let mut w = ws();
        w.open_auto("a");
        w.open_auto("b");
        w.focus("b");
        w.close_server("b");
        assert_eq!(w.open_ids(), ["a"]);
        assert_eq!(w.focused(), Some("a"));
        w.close_server("a");
        assert!(w.open_ids().is_empty());
        assert!(w.tree.is_empty(), "paneless tree resets to empty");
        assert_eq!(w.focused(), None);
    }

    #[test]
    fn workspaces_close_keeps_at_least_one_and_fixes_active() {
        let mut wss = Workspaces::new();
        wss.add();
        wss.add(); // three workspaces, active = last
        assert_eq!(wss.active, 2);
        wss.close(0); // closing an earlier tab shifts active down
        assert_eq!(wss.active, 1);
        wss.close(1); // close the active (last) tab
        assert_eq!(wss.active, 0);
        wss.close(0); // closing the only tab replaces it with a fresh one
        assert_eq!(wss.list.len(), 1);
        assert!(wss.active().open_ids().is_empty());
    }

    #[test]
    fn workspaces_add_stops_at_the_cap() {
        let mut wss = Workspaces::new();
        for _ in 0..(MAX_WORKSPACES + 10) {
            wss.add();
        }
        assert_eq!(wss.list.len(), MAX_WORKSPACES);
    }

    #[test]
    fn workspace_ids_are_not_reused_after_close() {
        let mut wss = Workspaces::new();
        wss.add(); // id 2
        wss.close(1);
        wss.add();
        assert_eq!(wss.list[1].id, 3, "ids stay monotonic");
    }
}
