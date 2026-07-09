//! Best-effort cleanup of managed server groups when the app is terminated by a
//! signal (SIGTERM/SIGINT/SIGHUP, or the Windows console close) rather than a
//! normal window close.
//!
//! A normal close drops each [`crate::process::running::RunningProcess`], whose
//! `Drop` kills the group. A signal bypasses `Drop` entirely, so we relay the
//! termination to the tracked groups here. Signals that cannot be caught
//! (SIGKILL, power loss) still slip past — that is what the PID-persistence
//! reconcile ([`crate::process::runtime_state`]) recovers from on the next
//! launch. This layer only shrinks how often that safety net is needed.

use crate::process::kill_tree;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Leader PIDs of every process group this app currently manages (owned and
/// adopted). The signal handler reads this; ctrlc runs the handler on its own
/// thread (not in async-signal context), so locking here is safe.
static GROUPS: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();

fn groups() -> &'static Mutex<HashSet<u32>> {
    GROUPS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Lock the registry, tolerating poisoning: it holds only PIDs, so a panic
/// elsewhere must never leave the shutdown safety net permanently disabled.
fn locked() -> std::sync::MutexGuard<'static, HashSet<u32>> {
    groups()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Start tracking `leader_pid` for signal-time cleanup (call on spawn/adopt).
pub fn register(leader_pid: u32) {
    locked().insert(leader_pid);
}

/// Stop tracking `leader_pid` — its process ended or was stopped.
pub fn unregister(leader_pid: u32) {
    locked().remove(&leader_pid);
}

/// Send a graceful termination to every tracked group. Called by the signal
/// handler; split out so it is testable without exiting the process.
pub fn terminate_all() {
    let set = locked();
    for &pid in set.iter() {
        kill_tree::tree_kill(pid, kill_tree::Signal::Term);
    }
}

/// Install the termination handler once at startup: on a caught signal, send a
/// graceful termination to every tracked group, then exit. Well-behaved servers
/// shut down on that signal; any that survive are caught by the next-launch
/// reconcile. Call exactly once — a second call is reported and ignored (ctrlc
/// permits only one handler).
pub fn install_handler() {
    let result = ctrlc::set_handler(|| {
        terminate_all();
        // We bypassed Drop by exiting on a signal, but the terminations above are
        // already queued to the child groups. 130 = 128 + SIGINT, conventional
        // for a signal-triggered exit.
        std::process::exit(130);
    });
    if let Err(err) = result {
        eprintln!("campfire: could not install shutdown handler ({err})");
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use command_group::CommandGroup;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn terminate_all_signals_registered_groups() {
        // A real process group, tracked as if it were a managed server.
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 30 & wait")
            .group_spawn()
            .expect("spawn group");
        let pid = child.id();
        register(pid);

        terminate_all();

        // The leader must exit; reap it so no zombie lingers.
        let deadline = Instant::now() + Duration::from_secs(5);
        let terminated = loop {
            match child.try_wait().expect("try_wait") {
                Some(_) => break true,
                None if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                None => break false,
            }
        };
        unregister(pid);
        assert!(terminated, "registered group was not terminated");
    }
}
