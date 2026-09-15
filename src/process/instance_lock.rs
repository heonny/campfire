//! Single-instance guard. Two Campfire instances would share `running.json`:
//! the second adopts the first's servers as "recovered", and closing it then
//! force-kills them. So the first instance leaves `campfire.lock` (its PID and
//! start time) in the data dir, and a later launch that finds that process
//! still alive refuses to start.

use crate::process::runtime_state::process_start_time;
use std::path::PathBuf;

/// Held for the life of the app; removes the lock file on drop. A stale file
/// (crash / SIGKILL) is harmless: the liveness check ignores dead PIDs.
pub struct InstanceLock {
    path: PathBuf,
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn lock_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "heonny", "campfire")?;
    Some(dirs.data_local_dir().join("campfire.lock"))
}

/// Take the lock. `None` means another live instance holds it. If the data dir
/// is unavailable the lock is skipped rather than blocking startup.
pub fn acquire() -> Option<InstanceLock> {
    let Some(path) = lock_path() else {
        return Some(InstanceLock {
            path: PathBuf::new(),
        });
    };
    let pid = std::process::id();
    let start = process_start_time(pid).unwrap_or(0);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // create_new is atomic: two simultaneous launches can't both win it. An
    // existing file is either a live instance (refuse) or stale (replace).
    let claim = || {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, format!("{pid} {start}").as_bytes())
            })
    };
    if claim().is_err() {
        if let Ok(text) = std::fs::read_to_string(&path)
            && other_instance_alive(&text, pid)
        {
            return None;
        }
        let _ = std::fs::remove_file(&path);
        let _ = claim(); // ponytail: a loss here means two launches in the same ms; the orphan reconcile still bounds the damage.
    }
    Some(InstanceLock { path })
}

/// Parse "`pid start_time`" and check that process is alive, is not us, and
/// still has the recorded start time (guards PID reuse).
fn other_instance_alive(text: &str, own_pid: u32) -> bool {
    let mut parts = text.split_whitespace();
    let (Some(pid), Some(start)) = (
        parts.next().and_then(|p| p.parse::<u32>().ok()),
        parts.next().and_then(|s| s.parse::<u64>().ok()),
    ) else {
        return false;
    };
    pid != own_pid && process_start_time(pid) == Some(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_or_own_pid_is_not_another_instance() {
        assert!(!other_instance_alive("", 1));
        assert!(!other_instance_alive("nope", 1));
        let me = std::process::id();
        let start = process_start_time(me).unwrap();
        assert!(!other_instance_alive(&format!("{me} {start}"), me));
    }

    #[test]
    fn live_process_with_matching_start_time_is_another_instance() {
        let me = std::process::id();
        let start = process_start_time(me).unwrap();
        // Our own process, seen from a different "own pid": alive and matching.
        assert!(other_instance_alive(&format!("{me} {start}"), me + 1));
        // Wrong start time (PID reuse): not a match.
        assert!(!other_instance_alive(
            &format!("{me} {}", start + 1),
            me + 1
        ));
    }
}
