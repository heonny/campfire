# Changelog

## Unreleased

### Added

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

### Scope

- Registration reads local files only; it does not execute project code, install
  dependencies, or automatically select ports or environment files.
- Recursive workspace/module discovery, custom Gradle tasks and plugin aliases,
  Cargo feature activation, and conditional or nested Go entry points require
  manual configuration.
