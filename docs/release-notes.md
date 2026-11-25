Campfire 0.1.1 fills in launch commands for new projects and fixes port handling around restarts.

- Selecting a project folder fills the name and default command for Node, Gradle, Cargo, and Go projects.
- A server whose port is still held by a previous run is no longer reported ready at once. The port check now sees IPv6 and wildcard listeners such as the JVM's.
- Restart waits for the old server to release its port, so Gradle apps restart cleanly.
- The log no longer goes blank after jumping to the bottom.
- The help dialog shows the version and commit hash.

See CHANGELOG.md for the full list.

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
