# Changelog

## Unreleased

## 0.1.1 - 2026-11-25

### Added

- Show the app version and commit hash in the help dialog.

- Fill the project name and default launch command when selecting a folder.
  - Node: prefer `dev`, then `start`, using the detected npm, pnpm, Yarn, or Bun runner.
  - Gradle: suggest `bootRun` or `run` without requiring a Spring Boot preset;
    prefer the platform's local wrapper and fall back to `gradle` on PATH.
  - Rust: detect Cargo binaries, respect `default-run` and `autobins`, and offer
    a binary picker when the default is ambiguous.
  - Go: suggest `go run .` for modules with an unconditional root main package.
- Show guidance when no default can be determined or several project types coexist.
- Preserve manually edited commands and existing saved configurations during detection.

### Fixed

- Ignore commented-out Gradle plugins and plugin declarations with `apply false`
  when deriving task suggestions.
- Detect ports held by IPv6 or wildcard listeners, such as the JVM's `*:8080`.
  Previously a start could pass the port check while the old server still held
  the port, and the server was reported ready immediately.
- Restart waits for the old server to release its port before relaunching,
  instead of failing when a build tool such as Gradle is still shutting the
  app down.
- Keep the log visible after jumping to the bottom; the pane could render blank.

### Scope

- Registration reads local files only; it does not execute project code, install
  dependencies, or automatically select ports or environment files.
- Recursive workspace/module discovery, custom Gradle tasks and plugin aliases,
  Cargo feature activation, and conditional or nested Go entry points require
  manual configuration.
