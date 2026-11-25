//! Embeds the short commit hash as `CAMPFIRE_GIT_HASH`, so the help modal can
//! tell two builds of the same version apart. Falls back to "unknown" when built
//! outside a git checkout (e.g. from a source tarball).

use std::process::Command;

fn main() {
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=CAMPFIRE_GIT_HASH={hash}");
    // logs/HEAD is appended on every commit and checkout, unlike HEAD itself,
    // which stays `ref: refs/heads/<branch>` across commits.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/logs/HEAD");
}
