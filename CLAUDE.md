# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Campfire is a small native desktop app (Rust + egui/eframe) for running and managing multiple local dev servers: start/stop/restart, live ANSI-colored logs with search, per-server env/port config, and port-conflict detection. It ships as a single lightweight binary for macOS and Windows.

## Commands

```bash
cargo run                     # run in development
cargo build --release         # size-optimized binary (~9 MB) -> target/release/campfire
cargo test                    # all unit tests
cargo test to_job_highlights  # run a single test by name substring
cargo clippy --all-targets    # lint; keep clean before committing
cargo fmt                     # format

./scripts/bundle-macos.sh     # macOS: build Campfire.app (+ generated .icns) -> target/release/bundle
```

Rust edition 2024; `egui`/`eframe` are pinned at 0.35.

## Architecture

### State and data flow — understand this first
`main.rs` owns **all** mutable app state (`CampfireApp`) and mutates it in exactly one place, `apply_action`. Everything under `src/ui/` is render-only: panels receive a read-only `ui::View` snapshot and report user intent through the unified `ui::Action` enum. Actions are collected during a frame and applied **after** the panels close, so render code never borrows app state mutably. To add a feature, extend `View` (what a panel can see) and `Action` (what it can request) rather than mutating state from inside a panel.

The entry point is `eframe::App::ui(&mut Ui, ..)` — egui 0.35, **not** the older `update` — and panels use the unified `egui::Panel`. The 0.35 API differs substantially from older egui and from most online examples; verify signatures against the installed crate source, not memory.

### Process lifecycle — `src/process/`
- `command.rs` builds the real invocation: it wraps the user's command string in a login shell (`$SHELL -lc` on unix, `cmd /C` on windows; a per-server `shell` override exists for version managers like nvm) and layers the environment — explicit env vars, then a `.env` file, then injected `PORT` **and** `SERVER_PORT` (Spring Boot reads `SERVER_PORT`, Node reads `PORT`, so both are set).
- `running.rs` — `RunningProcess` spawns through `command-group` so the whole process **group** can be tree-killed. stdout/stderr are read on threads into an mpsc channel and drained non-blockingly by `poll()` each frame. Stop is graceful: SIGTERM → 3 s grace → SIGKILL on the group. `Drop` force-kills the group, so closing the app also stops its servers — which is why the release profile keeps unwinding (**no `panic = "abort"`**): abort would skip `Drop` and orphan child processes. A `RunningProcess` is either **Owned** (spawned here, with live log pipes) or **Adopted** (recovered from a previous session by PID — stop/restart work, but there are no live logs); the public API is identical, so the UI is agnostic.
- `log_buffer.rs` — a byte-capped 5 MiB ring buffer.
- Orphan safety net (two layers, because `Drop` only runs on a clean close, not on SIGKILL/crash/power-loss):
  - `runtime_state.rs` — persists the running set to `running.json` in the OS data dir on every start/stop. On launch, a recorded PID that is still alive **and** whose `start_time` matches (guards against PID reuse) is a confirmed orphan: it is **adopted** if its config still exists, or stopped to free its port if the config was deleted.
  - `kill_tree.rs` — terminate a group / probe liveness by **PID alone**, without the `GroupChild` handle (Unix `killpg`, since command-group makes PGID == PID; Windows walks the `sysinfo` subtree). Reaps orphans whose handle died with a previous instance.
  - `shutdown.rs` — a SIGTERM/SIGINT/SIGHUP (and Windows console-close) handler that relays termination to the tracked groups before exiting, since a signal skips `Drop`. Uncatchable signals (SIGKILL) fall through to the next-launch reconcile above.

### Rendering and theme — `src/theme.rs`, `src/ui/`
- `theme.rs` installs light `Visuals`, the bundled Pretendard font (covers Latin + Hangul; egui's default fonts have no CJK), the frame helpers, and the palette.
- **Section-block layout**: the UI is white rounded blocks (`block_frame`, one shared radius) floating on a warm grey canvas (`CANVAS_FILL`). Sections are separated by surface contrast, **not** divider lines — panels set `.show_separator_line(false)` and carry the inter-block gaps as margins via `canvas_frame(margin)`. When adding a section, wrap it in `block_frame` rather than reaching for `ui.separator()`. Frame helpers: `block_frame` (top-level section) / `card_frame` (list item) / `modal_frame` (dialog) / `inset_frame` (code block).
- `App::clear_color` is overridden to `CANVAS_FILL`: eframe's default surface clear is near-black, and a panel resize drag can leave a one-frame gap that would otherwise flash black.
- The sidebar panel's resize indicator is restyled to a slim accent line via `with_accent_resize_indicator` (egui draws it with the interactive `fg_stroke`, near-black by default); the helper saves/restores the stroke because `fg_stroke` doubles as other widgets' hover text color, so it must not be changed globally.
- Buttons are intentionally **chromeless**: interactive `bg_stroke` is zeroed globally (no border), and `icon_button` sets `frame_when_inactive(false)` so an icon shows no fill at rest and only the hover fill on interaction. Do **not** re-add a per-button stroke — egui budgets the border width inside a button's inner margin, so changing it between rest and hover makes the layout jitter. Text inputs get their border from a wrapping frame (`ui::text_input`) because the global widget border is off.
- Rows that pair a variable-width name with fixed trailing items (sidebar card, detail header) lay the trailing items out first (right-to-left) and give the name a `Label::truncate()` in the remaining space, so a narrow width elides the name instead of overlapping.
- `ansi.rs` turns ANSI SGR sequences into a colored egui `LayoutJob` and highlights search matches; `strip()` returns plain text for filtering.
- Icons are Lucide SVGs (ISC-licensed, in `assets/icons/`) rasterized at runtime by `egui_extras` (svg feature); `egui_extras::install_image_loaders` runs once at startup.

### Other subsystems
- `model.rs` / `store.rs` — config is TOML written to the OS app-config directory (`com.heonny.campfire/servers.toml`) with a schema version and atomic temp-file-plus-rename saves.
- `metrics.rs` — per-server CPU/memory via `sysinfo`, summed over the whole process subtree (the tracked PID is the shell, not the app itself).
- `port.rs` — detects both ports already in use and ports assigned to two servers in the config.

## Conventions

- **Public open-source repo**: never commit absolute/home paths, personal data, or other private info in code, comments, docs, or commit messages.
- Cross-platform code branches with `#[cfg(unix)]` / `#[cfg(windows)]`; both macOS and Windows are targets.
- Commit messages use conventional prefixes (`feat:` / `fix:` / `refactor:` / `chore:` / `docs:`), are imperative, and carry no emoji or AI-generation markers.
