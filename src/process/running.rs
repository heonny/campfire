//! A managed server process: its handle (owned child, or an adopted PID), its
//! captured logs, and status.
//!
//! A [`RunningProcess`] is one of two shapes:
//! - **Owned** — spawned by this app. We hold the group-child handle on the UI
//!   thread; background reader threads stream stdout/stderr into a channel that
//!   [`RunningProcess::poll`] drains each frame. Shutdown is graceful-then-force
//!   over the whole process group.
//! - **Adopted** — recovered from a previous session that was force-killed
//!   before it could run `Drop`. The original handle and pipes died with that
//!   instance, so we only have the leader PID; we can stop/restart it (via
//!   [`crate::process::kill_tree`]) and watch its liveness, but not stream logs.
#![allow(dead_code)] // started_at()/Starting are surfaced in later steps (health check, UI).

use crate::model::ServerConfig;
use crate::process::command::build_command;
use crate::process::kill_tree;
use crate::process::log_buffer::{LogBuffer, LogLine, Stream};
use crate::process::runtime_state::{self, RuntimeEntry};
use command_group::{CommandGroup, GroupChild};
#[cfg(unix)]
use command_group::{Signal, UnixChildExt};
use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// A UI-repaint callback, shared with reader threads and the shutdown timer.
type Wake = Arc<dyn Fn() + Send + Sync>;

/// How often an adopted process re-checks that its PID is still alive. Owned
/// processes learn of exit immediately via `try_wait`; adopted ones have no
/// handle, so they poll — throttled to keep the syscall cost negligible.
const ADOPTED_LIVENESS_INTERVAL: Duration = Duration::from_secs(1);

/// Lifecycle state of a managed process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Stopped,
    Starting,
    Running,
    Crashed { code: Option<i32> },
}

/// How a [`RunningProcess`] is attached to its OS process.
enum ProcessHandle {
    /// Spawned and owned by this app: we hold the group-child handle and stream
    /// its output over `log_rx`.
    Owned {
        child: GroupChild,
        log_rx: Receiver<LogLine>,
    },
    /// Recovered from a previous session: the original handle died with that
    /// instance, so only the leader PID remains. `start_time` re-confirms
    /// identity (against PID reuse) on each liveness poll; `last_liveness`
    /// throttles those polls. No live logs.
    Adopted {
        start_time: u64,
        last_liveness: Option<Instant>,
    },
}

/// A running (or recently-finished) server process.
pub struct RunningProcess {
    handle: ProcessHandle,
    logs: LogBuffer,
    status: Status,
    started_at: SystemTime,
    stop_requested: bool,
    /// When set, escalate to a forceful kill once this instant passes — bounds a
    /// graceful SIGTERM. Checked in [`RunningProcess::poll`].
    force_deadline: Option<Instant>,
    /// Wake the UI (repaint). Held so [`RunningProcess::stop`] can schedule a
    /// deadline wake-up independent of the render loop's cadence.
    wake: Wake,
    pid: u32,
}

impl RunningProcess {
    /// Spawn `config` in its own process group with piped stdout/stderr. `wake`
    /// is invoked from the reader threads whenever a line arrives, so the UI can
    /// repaint (e.g. `move || ctx.request_repaint()`).
    pub fn spawn<W>(config: &ServerConfig, wake: W) -> std::io::Result<Self>
    where
        W: Fn() + Send + Sync + 'static,
    {
        let mut cmd = build_command(config)?;
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        let mut child = cmd.group_spawn()?;
        let pid = child.id();

        let wake: Wake = Arc::new(wake);
        let (tx, log_rx) = channel::<LogLine>();
        // Take the pipes BEFORE any wait/kill — command-group's inner Child can
        // desync afterward.
        if let Some(out) = child.inner().stdout.take() {
            spawn_reader(out, Stream::Stdout, tx.clone(), Arc::clone(&wake));
        }
        if let Some(err) = child.inner().stderr.take() {
            spawn_reader(err, Stream::Stderr, tx.clone(), Arc::clone(&wake));
        }
        drop(tx); // reader threads hold the only senders now

        Ok(Self {
            handle: ProcessHandle::Owned { child, log_rx },
            logs: LogBuffer::default(),
            status: Status::Running,
            started_at: SystemTime::now(),
            stop_requested: false,
            force_deadline: None,
            wake,
            pid,
        })
    }

    /// Recover a server left running by a previous session as a managed process
    /// we can stop/restart but not stream logs from (the original pipes died
    /// with that instance). Identity is re-checked against `entry.start_time` on
    /// each poll, so a reused PID is never mistaken for the original.
    pub fn adopt<W>(entry: &RuntimeEntry, wake: W) -> Self
    where
        W: Fn() + Send + Sync + 'static,
    {
        Self {
            handle: ProcessHandle::Adopted {
                start_time: entry.start_time,
                last_liveness: None,
            },
            logs: LogBuffer::default(),
            status: Status::Running,
            started_at: SystemTime::UNIX_EPOCH + Duration::from_secs(entry.start_time),
            stop_requested: false,
            force_deadline: None,
            wake: Arc::new(wake),
            pid: entry.pid,
        }
    }

    /// Drain pending log lines, detect exit, and escalate shutdown if overdue.
    /// Non-blocking; call once per frame while the process is non-terminal.
    pub fn poll(&mut self) {
        // Owned processes stream logs; adopted ones have no pipe to drain.
        if let ProcessHandle::Owned { log_rx, .. } = &self.handle {
            while let Ok(line) = log_rx.try_recv() {
                self.logs.push(line.stream, line.text);
            }
        }

        if self.is_terminal() {
            return;
        }

        if let Some(deadline) = self.force_deadline
            && Instant::now() >= deadline
        {
            self.force_kill();
            self.force_deadline = None;
        }

        // Owned: reap through the handle for an exact exit code.
        if let ProcessHandle::Owned { child, .. } = &mut self.handle {
            match child.try_wait() {
                Ok(Some(exit)) => {
                    let code = exit.code();
                    self.status = if self.stop_requested || code == Some(0) {
                        Status::Stopped
                    } else {
                        Status::Crashed { code }
                    };
                    self.force_deadline = None;
                }
                Ok(None) => {}
                Err(err) => {
                    // A monitoring failure, distinct from an external kill —
                    // surface it, since Status::Crashed{None} can't carry the
                    // distinction.
                    eprintln!("campfire: try_wait failed for pid {}: {err}", self.pid);
                    self.status = Status::Crashed { code: None };
                    self.force_deadline = None;
                }
            }
        }

        // Adopted: poll liveness by PID (no exit code available without a handle).
        if let ProcessHandle::Adopted {
            start_time,
            last_liveness,
        } = &mut self.handle
        {
            let due = last_liveness.is_none_or(|at| at.elapsed() >= ADOPTED_LIVENESS_INTERVAL);
            if due {
                *last_liveness = Some(Instant::now());
                let start_time = *start_time;
                // Alive AND still the same process (start_time guards PID reuse).
                let alive = runtime_state::process_start_time(self.pid) == Some(start_time);
                if !alive {
                    self.status = if self.stop_requested {
                        Status::Stopped
                    } else {
                        Status::Crashed { code: None }
                    };
                    self.force_deadline = None;
                }
            }
        }
    }

    /// Request shutdown of the whole process group. Sends a graceful signal and
    /// escalates to a forceful kill after `grace` (via [`RunningProcess::poll`]).
    /// Owned processes signal through the group-child handle; adopted ones go
    /// through [`crate::process::kill_tree`] by PID.
    ///
    /// A second `stop` while a graceful shutdown is already in flight escalates
    /// immediately — the user asked twice, so the remaining grace is skipped and
    /// the group is force-killed now (mirrors IntelliJ's "stop again to force").
    pub fn stop(&mut self, grace: Duration) {
        if self.is_terminal() {
            return;
        }
        if self.stop_requested {
            self.force_kill();
            self.force_deadline = None;
            return;
        }
        self.stop_requested = true;

        if self.request_termination() {
            // A graceful signal was sent — bound it: escalate to a forceful kill
            // after `grace`, and guarantee poll() runs then to do so, even if the
            // render loop is idle (e.g. a minimized window).
            self.force_deadline = Some(Instant::now() + grace);
            let wake = Arc::clone(&self.wake);
            thread::spawn(move || {
                thread::sleep(grace);
                wake();
            });
        }
    }

    /// Send the initial termination, graceful where the platform allows. Returns
    /// whether a grace period applies, so [`RunningProcess::stop`] schedules the
    /// SIGKILL escalation.
    fn request_termination(&mut self) -> bool {
        match &mut self.handle {
            ProcessHandle::Owned { child, .. } => {
                #[cfg(unix)]
                {
                    // SIGINT (Ctrl+C), not SIGTERM: a build tool that relays a
                    // child's logs — Gradle `bootRun` running a Spring Boot app —
                    // forwards SIGINT to that child and keeps relaying its
                    // shutdown output, so graceful-shutdown logs reach us instead
                    // of being cut off when the relay itself exits.
                    if child.signal(Signal::SIGINT).is_ok() {
                        return true;
                    }
                }
                let _ = child.kill();
                false
            }
            // No graceful group signal without the handle on Windows, but on Unix
            // killpg(SIGINT) mirrors the owned path; escalation still applies.
            ProcessHandle::Adopted { .. } => {
                kill_tree::tree_kill(self.pid, kill_tree::Signal::Interrupt);
                cfg!(unix)
            }
        }
    }

    /// Force-kill the process now, by whichever mechanism this handle supports.
    fn force_kill(&mut self) {
        match &mut self.handle {
            ProcessHandle::Owned { child, .. } => {
                let _ = child.kill();
            }
            ProcessHandle::Adopted { .. } => {
                kill_tree::tree_kill(self.pid, kill_tree::Signal::Kill);
            }
        }
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn logs(&self) -> &LogBuffer {
        &self.logs
    }

    /// Clear the captured log buffer; new output keeps streaming in.
    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn started_at(&self) -> SystemTime {
        self.started_at
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.status, Status::Stopped | Status::Crashed { .. })
    }

    /// Whether this process was recovered from a previous session (adopted by
    /// PID, no live log stream) rather than spawned by this instance.
    pub fn is_recovered(&self) -> bool {
        matches!(self.handle, ProcessHandle::Adopted { .. })
    }
}

impl Drop for RunningProcess {
    fn drop(&mut self) {
        // Never leak a running server: force-kill the group on drop (app close or
        // process removal). Best-effort — ignore errors from an already-dead child.
        if self.is_terminal() {
            return;
        }
        match &mut self.handle {
            ProcessHandle::Owned { child, .. } => {
                let _ = child.kill();
                let _ = child.try_wait();
            }
            ProcessHandle::Adopted { .. } => {
                kill_tree::tree_kill(self.pid, kill_tree::Signal::Kill);
            }
        }
    }
}

fn spawn_reader<R>(reader: R, stream: Stream, tx: Sender<LogLine>, wake: Wake)
where
    R: std::io::Read + Send + 'static,
{
    // Fire-and-forget: the thread exits on EOF or once `tx.send` fails because
    // the receiver (`log_rx`) was dropped. It always terminates, so no join.
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    // Lossy decode so a non-UTF-8 byte (common in build output)
                    // never truncates the rest of the stream.
                    let text = String::from_utf8_lossy(&buf)
                        .trim_end_matches(['\r', '\n'])
                        .to_string();
                    if tx.send(LogLine { stream, text }).is_err() {
                        break;
                    }
                    wake();
                }
                Err(_) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Preset;

    fn drain_until_terminal(proc: &mut RunningProcess, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            proc.poll();
            if proc.is_terminal() {
                // Poll a bit more so reader threads flush trailing output.
                for _ in 0..5 {
                    thread::sleep(Duration::from_millis(10));
                    proc.poll();
                }
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn config_with_command(command: &str) -> ServerConfig {
        let mut config = ServerConfig::from_preset("test", std::env::temp_dir(), Preset::Custom);
        config.command = command.into();
        config
    }

    #[test]
    fn captures_output_and_clean_exit_is_stopped() {
        let mut proc =
            RunningProcess::spawn(&config_with_command("echo campfire-marker"), || {}).unwrap();
        drain_until_terminal(&mut proc, Duration::from_secs(5));

        assert!(proc.is_terminal(), "echo did not exit in time");
        assert_eq!(proc.status(), &Status::Stopped); // exit 0, not user-stopped
        let found = proc
            .logs()
            .iter()
            .any(|l| l.text.contains("campfire-marker"));
        assert!(found, "marker not captured; {} lines", proc.logs().len());
    }

    #[test]
    fn nonzero_exit_is_crashed() {
        let mut proc = RunningProcess::spawn(&config_with_command("exit 3"), || {}).unwrap();
        drain_until_terminal(&mut proc, Duration::from_secs(5));

        assert!(proc.is_terminal());
        assert_eq!(proc.status(), &Status::Crashed { code: Some(3) });
    }

    #[test]
    fn stop_terminates_long_running_process() {
        #[cfg(unix)]
        let command = "sleep 30";
        #[cfg(windows)]
        let command = "ping -n 30 127.0.0.1 > NUL";
        let mut proc = RunningProcess::spawn(&config_with_command(command), || {}).unwrap();
        assert_eq!(proc.status(), &Status::Running);

        proc.stop(Duration::from_millis(300));
        drain_until_terminal(&mut proc, Duration::from_secs(5));

        assert!(proc.is_terminal(), "process did not stop");
        assert_eq!(proc.status(), &Status::Stopped);
    }

    #[cfg(unix)]
    #[test]
    fn second_stop_escalates_past_the_grace_window() {
        // A process that ignores the graceful signal (SIGINT) and SIGTERM: the
        // graceful stop won't end it, so only a forced kill can — proving the
        // second stop escalated rather than waiting out the (very long) grace.
        let command = "trap '' INT TERM; echo ready; while true; do sleep 0.05; done";
        let mut proc = RunningProcess::spawn(&config_with_command(command), || {}).unwrap();
        for _ in 0..10 {
            proc.poll();
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(proc.status(), &Status::Running);

        // First stop: SIGINT (ignored) with a grace far longer than the test.
        proc.stop(Duration::from_secs(60));
        thread::sleep(Duration::from_millis(150));
        proc.poll();
        assert!(
            !proc.is_terminal(),
            "a signal-ignoring process should survive within its grace window"
        );

        // Second stop: must force-kill now, not wait out the 60s grace.
        proc.stop(Duration::from_secs(60));
        drain_until_terminal(&mut proc, Duration::from_secs(5));
        assert!(proc.is_terminal(), "second stop did not force-kill the group");
        assert_eq!(proc.status(), &Status::Stopped);
    }

    #[test]
    fn lossy_decode_keeps_capturing_after_invalid_utf8() {
        // `printf` emits a raw 0xFF byte then a valid line; capture must survive.
        let mut proc = RunningProcess::spawn(
            &config_with_command(r"printf '\xff\nafter-invalid\n'"),
            || {},
        )
        .unwrap();
        drain_until_terminal(&mut proc, Duration::from_secs(5));

        assert!(proc.is_terminal());
        let found = proc.logs().iter().any(|l| l.text.contains("after-invalid"));
        assert!(found, "line after invalid UTF-8 was dropped");
    }

    #[test]
    fn adopting_dead_pid_becomes_terminal_on_first_poll() {
        // An almost-certainly-dead PID: the very first liveness poll must flip it
        // to terminal (crashed, since we did not request a stop).
        let entry = RuntimeEntry {
            server_id: "x".into(),
            name: "x".into(),
            pid: u32::MAX - 1,
            start_time: 1,
            port: None,
        };
        let mut adopted = RunningProcess::adopt(&entry, || {});
        assert!(adopted.is_recovered());
        assert_eq!(adopted.status(), &Status::Running); // optimistic until first poll

        adopted.poll();
        assert!(adopted.is_terminal());
        assert_eq!(adopted.status(), &Status::Crashed { code: None });
    }

    #[cfg(unix)]
    #[test]
    fn adopted_stop_terminates_the_real_group() {
        use command_group::CommandGroup;
        use std::process::Command;

        // Spawn a real group; we keep the handle only to reap the leader zombie
        // afterwards, mirroring how the OS reaps a genuinely-orphaned process.
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 30 & wait")
            .group_spawn()
            .expect("spawn group");
        let pid = child.id();
        let start_time = runtime_state::process_start_time(pid).expect("start time");

        let entry = RuntimeEntry {
            server_id: "x".into(),
            name: "x".into(),
            pid,
            start_time,
            port: None,
        };
        let mut adopted = RunningProcess::adopt(&entry, || {});
        adopted.stop(Duration::from_millis(200));

        // stop() tree-kills the group; the leader must exit. Reap it so the PID
        // leaves the table, as the OS would for a real orphan.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait().expect("try_wait") {
                Some(_) => break,
                None if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(20));
                }
                None => panic!("adopted stop did not terminate the group"),
            }
        }

        drain_until_terminal(&mut adopted, Duration::from_secs(5));
        assert!(adopted.is_terminal(), "adopted process did not stop");
        assert_eq!(adopted.status(), &Status::Stopped);
    }
}
