//! Hand-offs to the OS: open a URL in the default browser, reveal a folder in
//! the file manager. Fire-and-forget; a failure is only logged (the user sees
//! nothing happen and can retry), so no error plumbing.

use std::path::Path;
use std::process::Command;

pub fn open_url(url: &str) {
    open(url);
}

pub fn reveal_dir(dir: &Path) {
    open(&dir.to_string_lossy());
}

/// The platform's "open this thing with its default handler" command.
fn open(target: &str) {
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(target).spawn();
    #[cfg(windows)]
    let result = Command::new("cmd")
        .args(["/C", "start", "", target])
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(target).spawn();
    if let Err(err) = result {
        eprintln!("campfire: could not open '{target}': {err}");
    }
}

pub fn localhost_url(port: u16) -> String {
    format!("http://localhost:{port}")
}
