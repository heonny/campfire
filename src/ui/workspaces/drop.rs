//! Drag-to-place: dropping a sidebar card onto the dock opens that server's
//! log split against the pane under the cursor (nearest edge decides the side),
//! or as the first pane of an empty workspace. While the drag hovers the dock,
//! a translucent accent preview shows exactly where the pane would land.
//!
//! The zone math and tree surgery are pure (no `Ui`), so they're unit-tested;
//! only the overlay painting and release handling touch egui.

use super::{FULL_NOTICE, MAX_PANES, Workspace};
use crate::theme;
use crate::ui::SidebarDrag;
use eframe::egui;
use egui_tiles::{ContainerKind, Tile, TileId, Tree};

/// Which side of a target pane a drop lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Zone {
    Left,
    Right,
    Top,
    Bottom,
}

impl Zone {
    fn is_horizontal(self) -> bool {
        matches!(self, Zone::Left | Zone::Right)
    }

    /// Whether the new pane sits before (left of / above) the target.
    fn is_before(self) -> bool {
        matches!(self, Zone::Left | Zone::Top)
    }
}

/// The zone of `rect` nearest to `pos` (normalized by the side lengths, so a
/// wide flat pane still offers usable top/bottom bands).
pub(super) fn zone_at(rect: egui::Rect, pos: egui::Pos2) -> Zone {
    let width = rect.width().max(1.0);
    let height = rect.height().max(1.0);
    let candidates = [
        (Zone::Left, (pos.x - rect.left()) / width),
        (Zone::Right, (rect.right() - pos.x) / width),
        (Zone::Top, (pos.y - rect.top()) / height),
        (Zone::Bottom, (rect.bottom() - pos.y) / height),
    ];
    candidates
        .into_iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("four candidates")
        .0
}

/// The half of `rect` the new pane would take when dropped on `zone` — the
/// drop preview.
pub(super) fn zone_rect(rect: egui::Rect, zone: Zone) -> egui::Rect {
    let center = rect.center();
    match zone {
        Zone::Left => egui::Rect::from_min_max(rect.min, egui::pos2(center.x, rect.max.y)),
        Zone::Right => egui::Rect::from_min_max(egui::pos2(center.x, rect.min.y), rect.max),
        Zone::Top => egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, center.y)),
        Zone::Bottom => egui::Rect::from_min_max(egui::pos2(rect.min.x, center.y), rect.max),
    }
}

/// Split `target`'s `zone` side off for a new pane holding `server`.
///
/// When `target`'s parent is already a linear container in the wanted
/// direction, the new pane slides in as a sibling; otherwise `target` is
/// wrapped in a fresh split of that direction (also handling `target` being
/// the root). All ids stay valid — egui keeps per-tile layout state.
pub(super) fn insert_at(tree: &mut Tree<String>, target: TileId, zone: Zone, server: &str) {
    let wanted = if zone.is_horizontal() {
        ContainerKind::Horizontal
    } else {
        ContainerKind::Vertical
    };
    let new_pane = tree.tiles.insert_pane(server.to_owned());
    let parent = tree.tiles.parent_of(target);

    // Same-direction linear parent: insert as a sibling next to the target.
    if let Some(parent_id) = parent
        && let Some(Tile::Container(container)) = tree.tiles.get(parent_id)
        && container.kind() == wanted
    {
        let target_idx = child_index(container.children_vec(), target).unwrap_or(0);
        let idx = if zone.is_before() {
            target_idx
        } else {
            target_idx + 1
        };
        tree.move_tile_to_container(new_pane, parent_id, idx, false);
        return;
    }

    // Otherwise wrap the target in a new split of the wanted direction. The
    // wrap starts with just the new pane; the target is then moved in on the
    // proper side (which also detaches it from its old slot).
    let wrap = if zone.is_horizontal() {
        tree.tiles.insert_horizontal_tile(vec![new_pane])
    } else {
        tree.tiles.insert_vertical_tile(vec![new_pane])
    };
    let target_slot = if zone.is_before() { 1 } else { 0 };
    match parent {
        Some(parent_id) => {
            let idx = tree
                .tiles
                .get(parent_id)
                .and_then(|tile| match tile {
                    Tile::Container(c) => child_index(c.children_vec(), target),
                    Tile::Pane(_) => None,
                })
                .unwrap_or(0);
            tree.move_tile_to_container(wrap, parent_id, idx, false);
            tree.move_tile_to_container(target, wrap, target_slot, false);
        }
        None => {
            // Target is the root pane: the wrap becomes the new root.
            tree.move_tile_to_container(target, wrap, target_slot, false);
            tree.root = Some(wrap);
        }
    }
}

fn child_index(children: Vec<TileId>, child: TileId) -> Option<usize> {
    children.into_iter().position(|c| c == child)
}

/// What a card drop at the current pointer would do.
enum Target {
    /// Empty workspace (or the gap between panes): become a/the pane.
    Append,
    /// The server is already open here: focus its pane.
    Focus,
    /// Split this pane on this side.
    Split(TileId, Zone),
    /// Workspace already shows [`MAX_PANES`] logs.
    Reject,
}

/// While a sidebar card drags over the dock, paint the drop preview; on
/// release, apply the drop. Returns a rejection notice to surface, if any.
pub(super) fn handle_card_drag(
    ui: &egui::Ui,
    ws: &mut Workspace,
    drag: &SidebarDrag,
    dock_rect: egui::Rect,
) -> Option<&'static str> {
    let server = drag.server.as_deref()?;
    let pos = ui.ctx().pointer_hover_pos()?;
    if !dock_rect.contains(pos) {
        return None;
    }

    let target = if ws.is_open(server) {
        Target::Focus
    } else if ws.open_ids().len() >= MAX_PANES {
        Target::Reject
    } else {
        ws.pane_tiles()
            .into_iter()
            .find_map(|(tile, _)| {
                let rect = ws.tree.tiles.rect(tile)?;
                rect.contains(pos).then_some((tile, rect))
            })
            .map(|(tile, rect)| Target::Split(tile, zone_at(rect, pos)))
            .unwrap_or(Target::Append)
    };

    paint_preview(ui, ws, &target, server, pos, dock_rect);

    if !drag.finished {
        return None;
    }
    match target {
        Target::Focus => {
            ws.focus(server);
            None
        }
        Target::Append => ws.open_at(None, server),
        Target::Split(tile, zone) => ws.open_at(Some((tile, zone)), server),
        Target::Reject => Some(FULL_NOTICE),
    }
}

/// The translucent accent overlay marking where the drop would land (or a
/// "full" hint when it can't). Painted on the foreground layer so it sits above
/// the pane contents.
fn paint_preview(
    ui: &egui::Ui,
    ws: &Workspace,
    target: &Target,
    server: &str,
    pos: egui::Pos2,
    dock: egui::Rect,
) {
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("card_drop_overlay"),
    ));
    let preview = match target {
        Target::Append => Some(dock),
        // Already open: highlight the existing pane the drop would focus.
        Target::Focus => ws.find_pane(server).and_then(|t| ws.tree.tiles.rect(t)),
        Target::Split(tile, zone) => ws.tree.tiles.rect(*tile).map(|r| zone_rect(r, *zone)),
        Target::Reject => None,
    };
    match (preview, target) {
        (Some(rect), _) => {
            let rect = rect.shrink(2.0);
            painter.rect_filled(
                rect,
                egui::CornerRadius::same(8),
                theme::ACCENT_WEAK.gamma_multiply(0.6),
            );
            painter.rect_stroke(
                rect,
                egui::CornerRadius::same(8),
                egui::Stroke::new(2.0, theme::ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        (None, Target::Reject) => {
            painter.text(
                pos + egui::vec2(14.0, 14.0),
                egui::Align2::LEFT_TOP,
                FULL_NOTICE,
                egui::TextStyle::Small.resolve(ui.style()),
                theme::DANGER,
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 50.0))
    }

    #[test]
    fn zone_at_picks_the_nearest_normalized_edge() {
        assert_eq!(zone_at(rect(), egui::pos2(5.0, 25.0)), Zone::Left);
        assert_eq!(zone_at(rect(), egui::pos2(95.0, 25.0)), Zone::Right);
        assert_eq!(zone_at(rect(), egui::pos2(50.0, 3.0)), Zone::Top);
        assert_eq!(zone_at(rect(), egui::pos2(50.0, 47.0)), Zone::Bottom);
    }

    #[test]
    fn zone_rect_is_the_matching_half() {
        let r = rect();
        assert_eq!(zone_rect(r, Zone::Left).max.x, 50.0);
        assert_eq!(zone_rect(r, Zone::Right).min.x, 50.0);
        assert_eq!(zone_rect(r, Zone::Top).max.y, 25.0);
        assert_eq!(zone_rect(r, Zone::Bottom).min.y, 25.0);
    }

    fn ws_with(ids: &[&str]) -> Workspace {
        let mut w = Workspace::new(7, "t".to_owned());
        for id in ids {
            assert!(w.open_auto(id).is_none());
        }
        w
    }

    fn root_kind(w: &Workspace) -> Option<ContainerKind> {
        match w.tree.tiles.get(w.tree.root?)? {
            Tile::Container(c) => Some(c.kind()),
            Tile::Pane(_) => None,
        }
    }

    #[test]
    fn drop_left_on_sibling_joins_the_same_split() {
        let mut w = ws_with(&["a", "b"]);
        let target = w.find_pane("a").unwrap();
        assert!(w.open_at(Some((target, Zone::Left)), "c").is_none());
        assert_eq!(w.open_ids(), ["c", "a", "b"]);
        assert_eq!(root_kind(&w), Some(ContainerKind::Horizontal));
    }

    #[test]
    fn drop_right_inserts_after_the_target() {
        let mut w = ws_with(&["a", "b"]);
        let target = w.find_pane("a").unwrap();
        assert!(w.open_at(Some((target, Zone::Right)), "c").is_none());
        assert_eq!(w.open_ids(), ["a", "c", "b"]);
    }

    #[test]
    fn drop_top_wraps_the_target_in_a_vertical_split() {
        let mut w = ws_with(&["a", "b"]);
        let target = w.find_pane("a").unwrap();
        assert!(w.open_at(Some((target, Zone::Top)), "c").is_none());
        assert_eq!(w.open_ids(), ["c", "a", "b"]);
        // Root stays horizontal; its first child is now a vertical [c, a].
        assert_eq!(root_kind(&w), Some(ContainerKind::Horizontal));
        let root = w.tree.root.unwrap();
        let Some(Tile::Container(root_c)) = w.tree.tiles.get(root) else {
            panic!("root must be a container");
        };
        let first = root_c.children_vec()[0];
        let Some(Tile::Container(sub)) = w.tree.tiles.get(first) else {
            panic!("first child must be the wrap container");
        };
        assert_eq!(sub.kind(), ContainerKind::Vertical);
    }

    #[test]
    fn drop_bottom_on_a_root_pane_wraps_the_root() {
        let mut w = ws_with(&["a"]);
        let target = w.find_pane("a").unwrap();
        assert!(w.open_at(Some((target, Zone::Bottom)), "c").is_none());
        assert_eq!(w.open_ids(), ["a", "c"]);
        assert_eq!(root_kind(&w), Some(ContainerKind::Vertical));
    }

    #[test]
    fn drop_duplicate_focuses_instead_of_splitting() {
        let mut w = ws_with(&["a", "b"]);
        let target = w.find_pane("b").unwrap();
        assert!(w.open_at(Some((target, Zone::Left)), "a").is_none());
        assert_eq!(w.open_ids(), ["a", "b"]);
        assert_eq!(w.focused(), Some("a"));
    }

    #[test]
    fn drop_past_the_cap_is_rejected() {
        let mut w = ws_with(&["a", "b", "c", "d"]);
        let target = w.find_pane("a").unwrap();
        assert!(w.open_at(Some((target, Zone::Left)), "e").is_some());
        assert_eq!(w.open_ids().len(), MAX_PANES);
    }
}
