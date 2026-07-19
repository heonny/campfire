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
mod drop;
mod tabs;

use drop::Zone;

use crate::ui::log_view::LogView;
use crate::ui::{Action, View};
use eframe::egui;
use egui_tiles::{Tile, TileId, Tree};
use std::collections::{HashMap, HashSet};

/// A workspace shows at most this many log panes at once.
pub const MAX_PANES: usize = 4;
/// At most this many workspaces (tabs).
pub const MAX_WORKSPACES: usize = 100;
/// The rejection notice when a workspace is already showing [`MAX_PANES`] logs.
const FULL_NOTICE: &str = "A workspace shows at most 4 logs — close one first.";

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
        self.pane_tiles().into_iter().map(|(_, id)| id).collect()
    }

    /// `(tile, server id)` for every pane reachable from the root, in tree
    /// order — the hit-test set for drop targeting.
    fn pane_tiles(&self) -> Vec<(TileId, String)> {
        let mut out = Vec::new();
        if let Some(root) = self.tree.root {
            collect_panes(&self.tree, root, &mut out, &mut HashSet::new());
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

    /// Card click: show `server_id`'s log **without changing the layout** —
    /// focus its pane if open, otherwise swap it into the focused pane (falling
    /// back to the first pane), or open it as the first pane of an empty
    /// workspace. Splitting stays a drag (or context-menu) gesture.
    pub fn show_log(&mut self, server_id: &str) {
        if self.is_open(server_id) {
            self.focused = Some(server_id.to_owned());
            return;
        }
        let target = self
            .focused
            .as_deref()
            .and_then(|focused| self.find_pane(focused))
            .or_else(|| self.pane_tiles().first().map(|(tile, _)| *tile));
        match target {
            Some(tile) => {
                if let Some(Tile::Pane(server)) = self.tree.tiles.get_mut(tile) {
                    let old = std::mem::replace(server, server_id.to_owned());
                    self.views.remove(&old); // the old content's view state goes with it
                    self.focused = Some(server_id.to_owned());
                }
            }
            // Workspace is empty: the click just opens it (cannot be full).
            None => {
                let _ = self.open_at(None, server_id);
            }
        }
    }

    /// Open `server_id` at an automatic position (the non-drag path): appended
    /// to the root container, or wrapping a single root pane in a horizontal
    /// split. See [`Workspace::open_at`] for the shared rules.
    pub fn open_auto(&mut self, server_id: &str) -> Option<&'static str> {
        self.open_at(None, server_id)
    }

    /// Open `server_id`'s log pane. `target` places it against an existing
    /// pane's side (drag-to-place); `None` appends automatically. Already open
    /// → just focus (one pane per server per workspace). Returns a user-facing
    /// notice when the workspace is full. Module-internal: external callers go
    /// through [`Workspace::open_auto`] or the dock's drop handling.
    fn open_at(
        &mut self,
        target: Option<(TileId, Zone)>,
        server_id: &str,
    ) -> Option<&'static str> {
        if self.is_open(server_id) {
            self.focused = Some(server_id.to_owned());
            return None;
        }
        if self.open_ids().len() >= MAX_PANES {
            return Some(FULL_NOTICE);
        }
        match (target, self.tree.root) {
            (Some((tile, zone)), Some(_)) => {
                drop::insert_at(&mut self.tree, tile, zone, server_id);
            }
            (_, None) => {
                // First pane: a bare pane root (what egui_tiles' simplify would
                // reduce a single-child container to anyway), so the later
                // "wrap the root pane" split paths behave the same in headless
                // tests as after an in-app simplify pass.
                let mut tiles = egui_tiles::Tiles::default();
                let root = tiles.insert_pane(server_id.to_owned());
                self.tree = Tree::new(Self::tree_id(self.id), root, tiles);
            }
            (None, Some(root)) => {
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
        self.pane_tiles()
            .into_iter()
            .find_map(|(tile, id)| (id == server_id).then_some(tile))
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

/// Depth-first pane walk. `visited` defends against a cyclic tree (a container
/// listing itself as a descendant): our surgery never builds one, but
/// egui_tiles itself only breaks cycles during `Tree::ui`, and an unbounded
/// recursion here would be a stack overflow — cheap insurance.
fn collect_panes(
    tree: &Tree<String>,
    tile: TileId,
    out: &mut Vec<(TileId, String)>,
    visited: &mut HashSet<TileId>,
) {
    if !visited.insert(tile) {
        return;
    }
    match tree.tiles.get(tile) {
        Some(Tile::Pane(server)) => out.push((tile, server.clone())),
        Some(Tile::Container(container)) => {
            for child in container.children_vec() {
                collect_panes(tree, child, out, visited);
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

    // `active < list.len()` is maintained by every mutator (add/close/from_doc)
    // and `list` is never empty; the clamp makes that invariant enforced at one
    // chokepoint instead of a panic at whichever call site regresses first.
    pub fn active(&self) -> &Workspace {
        &self.list[self.active.min(self.list.len() - 1)]
    }

    pub fn active_mut(&mut self) -> &mut Workspace {
        let index = self.active.min(self.list.len() - 1);
        &mut self.list[index]
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

    /// Switch the active workspace (tab click / Cmd+number), giving the
    /// incoming one a sensible focused pane.
    fn switch_to(&mut self, index: usize) {
        if index >= self.list.len() || index == self.active {
            return;
        }
        self.active = index;
        self.list[index].fix_focus_and_reset();
    }

    /// Workspace keyboard shortcuts: Cmd/Ctrl+1–9 jump to that tab, Cmd/Ctrl+0
    /// to the tenth, Cmd/Ctrl+W closes the active one (the last workspace is
    /// replaced by a fresh empty one).
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        const KEYS: [egui::Key; 10] = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
            egui::Key::Num7,
            egui::Key::Num8,
            egui::Key::Num9,
            egui::Key::Num0,
        ];
        let mut switch = None;
        let mut close_active = false;
        ctx.input_mut(|input| {
            for (index, key) in KEYS.iter().enumerate() {
                if input.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::COMMAND,
                    *key,
                )) {
                    switch = Some(index);
                }
            }
            if input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::W,
            )) {
                close_active = true;
            }
        });
        if let Some(index) = switch {
            self.switch_to(index);
        }
        if close_active {
            self.close(self.active);
        }
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

    /// Render the tab strip and the active workspace's dock, then the card-drop
    /// overlay when a sidebar card is being dragged over it. Returns the dock's
    /// screen rect (the sidebar uses last frame's to gate its reorder) and any
    /// rejection notice from a drop.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        view: &View,
        action: &mut Option<Action>,
        drag: &crate::ui::SidebarDrag,
    ) -> (egui::Rect, Option<&'static str>) {
        self.handle_shortcuts(ui.ctx());
        tabs::strip(ui, self, view);
        let dock_rect = ui.available_rect_before_wrap();
        dock::show_active(ui, self, view, action);
        // After the tree rendered, its pane rects are laid out for this frame —
        // exactly what the drop preview needs.
        let notice = drop::handle_card_drag(ui, self.active_mut(), drag, dock_rect);
        (dock_rect, notice)
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
    fn show_log_focuses_when_open_swaps_when_not() {
        let mut w = ws();
        w.open_auto("a");
        w.open_auto("b");
        w.focus("a");
        // Not open: swaps into the focused pane, layout unchanged.
        w.show_log("c");
        assert_eq!(w.open_ids(), ["c", "b"]);
        assert_eq!(w.focused(), Some("c"));
        // Open elsewhere: only focuses.
        w.show_log("b");
        assert_eq!(w.open_ids(), ["c", "b"]);
        assert_eq!(w.focused(), Some("b"));
    }

    #[test]
    fn show_log_opens_the_first_pane_when_empty() {
        let mut w = ws();
        w.show_log("a");
        assert_eq!(w.open_ids(), ["a"]);
        assert_eq!(w.focused(), Some("a"));
    }

    #[test]
    fn show_log_swap_drops_the_old_panes_view_state() {
        let mut w = ws();
        w.open_auto("a");
        w.views.insert("a".to_owned(), LogView::default());
        w.show_log("b");
        assert!(!w.views.contains_key("a"), "replaced pane's view state dropped");
    }

    #[test]
    fn pane_walk_survives_a_cyclic_tree() {
        // Nothing builds a cyclic tree on purpose, but the walk must terminate
        // even on one (egui_tiles only breaks cycles during Tree::ui).
        let mut tiles = egui_tiles::Tiles::default();
        let pane = tiles.insert_pane("a".to_owned());
        let cycle = tiles.insert_horizontal_tile(vec![pane]);
        if let Some(Tile::Container(container)) = tiles.get_mut(cycle) {
            container.add_child(cycle); // self-reference
        }
        let mut w = ws();
        w.tree = Tree::new(Workspace::tree_id(9), cycle, tiles);
        assert_eq!(w.open_ids(), ["a"]); // terminates, panes still found
        assert!(w.find_pane("a").is_some());
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
    fn switch_to_changes_active_and_ignores_out_of_range() {
        let mut wss = Workspaces::new();
        wss.add();
        wss.switch_to(0);
        assert_eq!(wss.active, 0);
        wss.switch_to(99); // out of range: no-op
        assert_eq!(wss.active, 0);
    }

    #[test]
    fn closing_the_single_workspace_leaves_a_fresh_one() {
        let mut wss = Workspaces::new();
        wss.active_mut().open_auto("a");
        wss.close(0);
        assert_eq!(wss.list.len(), 1);
        assert!(wss.active().open_ids().is_empty(), "fresh empty workspace");
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
