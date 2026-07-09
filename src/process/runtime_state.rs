//! Persistence of the servers this app currently has running, so an instance
//! that dies WITHOUT running `Drop` (SIGKILL / crash / power loss) can be
//! reconciled on the next launch instead of leaking orphaned processes that
//! keep holding their ports.
//!
//! This is ephemeral machine state, deliberately separate from the user's
//! server config ([`crate::store`]): it lives in the data dir, is rewritten on
//! every start/stop, and any entry left behind whose process is still alive is
//! exactly an orphan to recover.
#![allow(dead_code)] // Wired into main.rs reconcile/track in this and later stages.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use sysinfo::{Pid, ProcessesToUpdate, System};

/// One running process, recorded with enough identity to re-find it after our
/// own death. `start_time` (seconds since epoch, from sysinfo) guards against
/// PID reuse: a recycled PID will not share the original process's start time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEntry {
    pub server_id: String,
    pub name: String,
    pub pid: u32,
    pub start_time: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

/// Default path: `running.json` in the per-OS *data* dir — ephemeral state,
/// kept out of the config dir that holds `servers.toml`.
pub fn state_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "heonny", "campfire")?;
    Some(dirs.data_local_dir().join("running.json"))
}

/// Load recorded entries from `path`. A missing file yields an empty list (the
/// normal clean-shutdown case); a corrupt file is treated the same way rather
/// than blocking startup — this is best-effort recovery state, not user data.
pub fn load_from(path: &Path) -> Vec<RuntimeEntry> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Atomically write `entries` to `path`. Errors are returned for the caller to
/// surface non-fatally: a failed write only risks a stale reconcile next launch.
pub fn save_to(path: &Path, entries: &[RuntimeEntry]) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(entries)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    crate::fs_util::write_atomic(path, text.as_bytes())
}

/// The OS start time (seconds since epoch) of `pid`, via a targeted sysinfo
/// refresh. `None` if the process is already gone. Captured at spawn time and
/// stored in [`RuntimeEntry::start_time`] as the PID-reuse anchor.
pub fn process_start_time(pid: u32) -> Option<u64> {
    let mut system = System::new();
    let p = Pid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[p]), true);
    system.process(p).map(|proc| proc.start_time())
}

/// Of `entries`, the ones whose recorded process is still alive AND whose start
/// time still matches — i.e. genuine orphans of a previous session. An entry
/// whose PID is gone, or reused by an unrelated process (start-time mismatch),
/// is dropped and never returned, so we never signal the wrong process.
pub fn confirmed_orphans(entries: &[RuntimeEntry]) -> Vec<RuntimeEntry> {
    if entries.is_empty() {
        return Vec::new();
    }
    // Refresh the whole process table rather than a targeted `Some(&pids)`: a
    // stale file may hold any PID value, and feeding an out-of-range PID to a
    // targeted refresh is unreliable across platforms. Runs once at startup, so
    // the cost is negligible.
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    entries
        .iter()
        .filter(|e| {
            system
                .process(Pid::from_u32(e.pid))
                .is_some_and(|proc| proc.start_time() == e.start_time)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pid: u32, start_time: u64) -> RuntimeEntry {
        RuntimeEntry {
            server_id: "id-1".into(),
            name: "api".into(),
            pid,
            start_time,
            port: Some(8090),
        }
    }

    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("campfire-runtime-{tag}-{}", std::process::id()));
        p.push("running.json");
        p
    }

    #[test]
    fn save_then_load_roundtrips() {
        let path = temp_path("roundtrip");
        let entries = vec![entry(111, 222), entry(333, 444)];
        save_to(&path, &entries).unwrap();
        assert_eq!(load_from(&path), entries);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_file_loads_empty() {
        let path = temp_path("missing");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert!(load_from(&path).is_empty());
    }

    #[test]
    fn corrupt_file_loads_empty() {
        let path = temp_path("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        assert!(load_from(&path).is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn confirmed_orphans_matches_live_pid_with_correct_start_time() {
        // Our own process is alive; its true start time makes it a "confirmed"
        // match, while a wrong start time (PID reuse) and a dead PID do not.
        let me = std::process::id();
        let real_start = process_start_time(me).expect("own start time");

        let entries = vec![
            entry(me, real_start),           // alive + matching -> orphan
            entry(me, real_start ^ 0xFF),    // alive but reused (mismatch) -> dropped
            entry(u32::MAX - 1, real_start), // dead pid -> dropped
        ];
        let orphans = confirmed_orphans(&entries);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].pid, me);
        assert_eq!(orphans[0].start_time, real_start);
    }

    #[test]
    fn confirmed_orphans_empty_input_is_empty() {
        assert!(confirmed_orphans(&[]).is_empty());
    }
}
