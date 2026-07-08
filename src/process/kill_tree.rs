//! Terminate a process group / subtree, and probe its liveness, by PID alone —
//! WITHOUT the original `command_group::GroupChild` handle.
//!
//! An owned server is spawned into its own process group: command-group makes
//! the child the group leader, so its PGID equals its PID. That is what lets us
//! reap an *orphaned* group on the next launch — the handle died with the old
//! app instance, but the leader PID (persisted to disk) still identifies the
//! whole group.
//!
//! - Unix: `killpg(leader_pid, sig)` signals the process group; `kill(pid, 0)`
//!   probes liveness. Same mechanism as [`crate::process::running::RunningProcess::stop`].
//! - Windows: there is no PID-only Job Object recovery, so fall back to a
//!   sysinfo subtree walk and terminate each descendant (best-effort).
#![allow(dead_code)] // Consumed by reconcile / adopt / shutdown in later stages.

/// A termination signal, mapped to the platform primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Graceful termination request (Unix `SIGTERM`). Windows has no graceful
    /// group signal, so there it behaves like [`Signal::Kill`].
    Term,
    /// Forceful kill (Unix `SIGKILL` / Windows `TerminateProcess`).
    Kill,
}

/// Whether the process-group leader `leader_pid` is still alive — our identity
/// anchor for an owned server. A dead leader means the group we manage is gone
/// (an orphan whose leader exited is reparented and reaped by the OS).
pub fn group_alive(leader_pid: u32) -> bool {
    platform::group_alive(leader_pid)
}

/// Signal the whole process group / subtree led by `leader_pid`. Best-effort:
/// errors from an already-dead group are ignored.
pub fn tree_kill(leader_pid: u32, signal: Signal) {
    platform::tree_kill(leader_pid, signal)
}

#[cfg(unix)]
mod platform {
    use super::Signal;

    pub fn group_alive(leader_pid: u32) -> bool {
        // kill(pid, 0) sends no signal — it only runs the permission/existence
        // checks: 0 => alive & signalable, ESRCH => gone, EPERM => alive but not
        // ours (still counts as alive).
        let rc = unsafe { libc::kill(leader_pid as libc::pid_t, 0) };
        if rc == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    pub fn tree_kill(leader_pid: u32, signal: Signal) {
        let sig = match signal {
            Signal::Term => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        };
        // command-group makes the child a group leader (PGID == PID), so one
        // killpg reaps the whole tree. Ignore the result — ESRCH just means the
        // group already exited.
        unsafe {
            libc::killpg(leader_pid as libc::pid_t, sig);
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::Signal;
    use std::collections::{HashMap, HashSet};
    use sysinfo::{Pid, ProcessesToUpdate, System};

    pub fn group_alive(leader_pid: u32) -> bool {
        let mut system = System::new();
        let pid = Pid::from_u32(leader_pid);
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        system.process(pid).is_some()
    }

    pub fn tree_kill(leader_pid: u32, _signal: Signal) {
        // No handle-free Job Object recovery exists on Windows, so walk the tree
        // via sysinfo and terminate each process. `TerminateProcess` has no
        // graceful analog, so both signals map to a hard kill. Kill descendants
        // before the root so a parent can't outlive (and re-detach) its child.
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::All, true);
        for pid in subtree(&system, Pid::from_u32(leader_pid)).into_iter().rev() {
            if let Some(proc) = system.process(pid) {
                proc.kill();
            }
        }
    }

    /// PIDs of `root` and all its descendants, root-first.
    fn subtree(system: &System, root: Pid) -> Vec<Pid> {
        let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
        for (pid, proc) in system.processes() {
            if let Some(parent) = proc.parent() {
                children.entry(parent).or_default().push(*pid);
            }
        }
        let mut out = Vec::new();
        let mut stack = vec![root];
        let mut seen = HashSet::new();
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            out.push(pid);
            if let Some(kids) = children.get(&pid) {
                stack.extend(kids.iter().copied());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_alive_true_for_current_process() {
        assert!(group_alive(std::process::id()));
    }

    #[cfg(unix)]
    #[test]
    fn tree_kill_terminates_the_group() {
        use command_group::CommandGroup;
        use std::process::Command;
        use std::time::{Duration, Instant};

        // A leader shell plus a backgrounded child: a two-process group, so the
        // kill has to reach a group member, not just the leader.
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 30 & wait")
            .group_spawn()
            .expect("spawn group");
        let pid = child.id();
        assert!(group_alive(pid), "just-spawned group should be alive");

        tree_kill(pid, Signal::Kill);

        // The leader must exit; reap it via try_wait so no zombie lingers (a
        // zombie would still answer kill(pid, 0), masking the kill).
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait().expect("try_wait") {
                Some(_) => break,
                None if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                None => panic!("group leader did not exit after tree_kill"),
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn tree_kill_frees_a_bound_port() {
        // The end-to-end point of the feature: a real process holding a real
        // port, reaped by PID alone, must release that port. Uses python3 to
        // bind an ephemeral port and hold it open.
        use command_group::CommandGroup;
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        let script = "import socket,sys,time\n\
                      s=socket.socket()\n\
                      s.bind(('127.0.0.1',0))\n\
                      s.listen()\n\
                      print(s.getsockname()[1]); sys.stdout.flush()\n\
                      time.sleep(60)\n";
        let Ok(mut child) = Command::new("python3")
            .arg("-c")
            .arg(script)
            .stdout(Stdio::piped())
            .group_spawn()
        else {
            eprintln!("skipping tree_kill_frees_a_bound_port: python3 unavailable");
            return;
        };
        let pid = child.id();

        // Read the ephemeral port the child bound.
        let stdout = child.inner().stdout.take().expect("child stdout");
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).expect("read bound port");
        let port: u16 = line.trim().parse().expect("port number");

        assert!(!crate::port::is_port_free(port), "port {port} should be held by the child");

        tree_kill(pid, Signal::Kill);
        let _ = child.wait(); // reap the leader

        // Once the holder dies, the port frees (poll briefly — teardown can lag).
        let deadline = Instant::now() + Duration::from_secs(5);
        while !crate::port::is_port_free(port) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(crate::port::is_port_free(port), "port {port} was not freed after tree_kill");
    }
}
