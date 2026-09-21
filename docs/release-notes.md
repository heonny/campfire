Campfire's first release: a native desktop app for managing local development servers.

- Start, stop, and restart servers with per-project commands, environment variables, and ports.
- Watch live logs in tabbed workspaces with up to four panes, search, and ANSI colors.
- Reorder projects by dragging, monitor CPU and memory, and receive ready/crash notifications.
- Glass surfaces for the sidebar, dialogs, and notifications.

### Downloads

- **macOS Apple Silicon (arm64), macOS 11 or later:** extract the ZIP and move `bundle/Campfire.app` to Applications.
- **Windows x64:** extract the ZIP and run `campfire.exe`.
- A compatible graphics adapter is required. Intel Mac binaries are not included.
- These binaries are not Developer ID signed/notarized (macOS) or Authenticode signed (Windows). The operating system may block or warn on first launch.
- `SHA256SUMS.txt` contains checksums for both downloads.

### Verification

The release workflow runs formatting checks, Clippy, and tests on both platforms before publishing. Builds use the committed Cargo.lock and Rust 1.96.1.

### Maintainers

Update Cargo.toml, Cargo.lock, and these notes before the next release. Push the matching `v<version>` tag to build and publish. A manual workflow run builds and uploads artifacts without publishing a release.
