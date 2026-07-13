# Multi-window log monitoring — implementation plan

> Status: **implemented** (workspaces edition). The original click-to-open
> phases below were superseded during review by a workspace model: tabbed
> workspaces (≤100), each an egui_tiles split of ≤4 log panes, opened by
> **dragging a server card into the dock** (drop position decides the split),
> rearranged by dragging pane titles, and resizable. Workspaces are
> session-only — a `workspaces.json` persistence layer shipped briefly and was
> then removed as not worth the state. See `src/ui/workspaces/` and the
> CLAUDE.md "Workspaces" section for the as-built architecture; the plan below
> is kept for history.

## Context

Campfire today shows **one** selected server's logs in a single detail pane
(`src/ui/detail.rs`, driven by `view.selected`). The goal is a log **monitoring**
view: watch up to **4** servers' logs at once, arranged as resizable dock splits,
with a collapsible sidebar and richer drag feedback, so Campfire becomes a usable
local "control room" for several dev servers.

Decisions already agreed:
- **Layout**: dynamic split / dock — panes are resizable and rearrangeable.
- **Card drag**: full reorder animation — the dragged card floats to the cursor
  and the other cards slide out of the way.
- **Rollout**: staged — sidebar collapse → drag animation → multi-window.

## Current architecture (what this touches)

- `src/main.rs` — `CampfireApp` owns all state and mutates it only in
  `apply_action`. `ui()` lays out `Panel::top` (top bar) + `Panel::left`
  (`"server_list"`, resizable 180–420) + `CentralPanel` (detail).
- `src/ui/mod.rs` — read-only `View` snapshot + `Action` enum; render code
  reports intent, the app applies it after the panels close.
- `src/ui/detail.rs` — renders the single `view.selected` server: header +
  `log_body`.
- `src/ui/log_view/` — `LogView` (search / follow / scroll state) lives **once**
  on `CampfireApp`. Log DATA is in `RunningProcess::logs()` (the `running` map),
  keyed by server id — so multiple views over the same data is cheap.
- `src/ui/server_list.rs` — cards; drag reorder via insertion line +
  `Action::Reorder` / `move_in_place`. Gotcha (already in CLAUDE.md): detect the
  drag with `response.dragged()`, never a hand-built `Id` + `is_being_dragged`.
- `src/theme.rs` — panel/frame helpers (`block_frame`, `canvas_frame`, …),
  palette, `with_accent_resize_indicator`.

## Phase 1 — Collapsible sidebar (small; was in the original mockup)

- State: add `sidebar_collapsed: bool` to `CampfireApp` (session-only for now).
- Toggle button (a Lucide `panel-left`-style icon in `assets/icons/`) in the top
  bar; the original mockup labels it "사이드바 버튼 클릭시 접기/펼치기".
- When collapsed, skip rendering `Panel::left` so the `CentralPanel` (logs) takes
  the full width. `clear_color` already prevents a black flash on resize.
- Add `Action::ToggleSidebar`.
- Verify: toggling expands/restores the log area cleanly.

## Phase 2 — Drag reorder with reposition animation

- Keep the current insertion-line result (`move_in_place`); add motion:
  - **Float**: while dragged, draw the card on the `Order::Tooltip` layer
    transformed to the cursor (the `dnd_drag_source` technique), keeping
    `Sense::click_and_drag()` so click-to-select still works.
  - **Slide**: interpolate each card's y toward its target slot with
    `ctx.animate_value_with_time`; recompute targets from the current hover index
    each frame so the others part around the drop point.
- Files: `src/ui/server_list.rs` (card render + animated y offset).
- Verify: dragging floats the card, the rest slide, dropping settles into place.

## Phase 3 — Multi-window logs (dynamic dock) — core, large

- **State model**:
  - Replace single `selected: Option<String>` with `open_logs: Vec<String>`
    (≤4) + `focused: Option<String>` (the sidebar highlight follows `focused`).
  - Open a log from a card; cap at 4.
  - Sidebar card shows **open** + **focused** state (border/dot) — explicitly
    requested.
- **Layout — dynamic dock**: evaluate the `egui_tiles` crate (docking/tiling for
  egui: splits, drag-to-rearrange, resize, close — out of the box). Confirm an
  egui-0.35-compatible version and an MIT/Apache license before adding it.
  Fallback: manual nested split panels + drag handles (more code, less polish).
- **Per-pane view state**: `LogView` (1) → `HashMap<String, LogView>` keyed by
  server id, so each pane keeps its own search / follow / scroll. Log data stays
  shared in the `running` map.
- Reuse `detail.rs`'s log rendering for each pane/tile.
- **Actions**: `OpenLog(id)` / `CloseLog(id)` / `FocusLog(id)` (split/resize/
  rearrange handled inside the tiles state).

## Open questions (resolve at implementation)

- Adopt `egui_tiles` (strongly fits "dynamic dock") vs hand-rolled splits — check
  version compatibility + license first.
- Trigger for opening a pane: single-click select, a dedicated "open" action, or
  the status button.
- Persist the open set + layout across launches, or session-only?
- Behavior at the 4-pane cap: block the new one, or evict the oldest.

## Verification (whole feature, per phase)

- `cargo test`, `cargo clippy --all-targets`, `cargo build` clean at each phase.
- Manual `cargo run`: toggle sidebar; drag-reorder with animation; open 2–4 logs,
  split/resize/rearrange, per-pane search, card open/focus indicators.
- Caveat: `cargo run` shares `running.json` with any live instance and can
  adopt/kill your real dev servers — verify in a throwaway setup or accept the
  risk knowingly.
