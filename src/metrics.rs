//! Per-server resource usage (CPU %, memory) via sysinfo.
//!
//! We track the shell wrapper's PID, but the real work happens in its children
//! (e.g. `zsh` → `gradle` → `java`), so usage is summed over the whole process
//! subtree. Refreshed at most once per second while any server is running.

use crate::process::running::RunningProcess;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessesToUpdate, System};

const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// Cached CPU/memory usage per server id.
pub struct Metrics {
    system: System,
    refreshed_at: Option<Instant>,
    values: HashMap<String, (f32, u64)>, // server id -> (cpu %, memory bytes)
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            refreshed_at: None,
            values: HashMap::new(),
        }
    }

    /// Recompute usage if the refresh interval has elapsed and something is
    /// running. Cheap no-op otherwise, so it is safe to call every frame.
    pub fn refresh(&mut self, running: &HashMap<String, RunningProcess>) {
        let due = self
            .refreshed_at
            .is_none_or(|at| at.elapsed() >= REFRESH_INTERVAL);
        if !due || !running.values().any(|p| !p.is_terminal()) {
            return;
        }
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        let children = self.children_map();
        self.values.clear();
        for (id, proc) in running {
            if !proc.is_terminal() {
                let root = Pid::from_u32(proc.pid());
                self.values
                    .insert(id.clone(), subtree_usage(&self.system, &children, root));
            }
        }
        self.refreshed_at = Some(Instant::now());
    }

    /// Cached `(cpu %, memory bytes)` for a server, if known.
    pub fn get(&self, id: &str) -> Option<(f32, u64)> {
        self.values.get(id).copied()
    }

    fn children_map(&self) -> HashMap<Pid, Vec<Pid>> {
        let mut map: HashMap<Pid, Vec<Pid>> = HashMap::new();
        for (pid, proc) in self.system.processes() {
            if let Some(parent) = proc.parent() {
                map.entry(parent).or_default().push(*pid);
            }
        }
        map
    }
}

/// Sum CPU % and memory over `root` and all its descendants.
fn subtree_usage(system: &System, children: &HashMap<Pid, Vec<Pid>>, root: Pid) -> (f32, u64) {
    let mut cpu = 0.0;
    let mut memory = 0;
    let mut stack = vec![root];
    let mut seen = HashSet::new();
    while let Some(pid) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        if let Some(proc) = system.process(pid) {
            cpu += proc.cpu_usage();
            memory += proc.memory();
        }
        if let Some(kids) = children.get(&pid) {
            stack.extend(kids.iter().copied());
        }
    }
    (cpu, memory)
}
