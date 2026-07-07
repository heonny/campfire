//! A spawned server process: its group-child handle, captured logs, and status.
//!
//! One [`RunningProcess`] owns the child (in its own process group / Job Object)
//! on the UI thread. Background reader threads stream stdout/stderr into a
//! channel; [`RunningProcess::poll`] (called each frame) drains them into the
//! log buffer and detects exit without blocking. Shutdown is graceful-then-force
//! and covers the whole process tree.
#![allow(dead_code)] // started_at()/Starting are surfaced in later steps (health check, UI).

use crate::model::ServerConfig;
use crate::process::command::build_command;
use crate::process::log_buffer::{LogBuffer, LogLine, Stream};
use command_group::{CommandGroup, GroupChild};
#[cfg(unix)]
use command_group::{Signal, UnixChildExt};
use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// A UI-repaint callback, shared with reader threads and the shutdown timer.
type Wake = Arc<dyn Fn() + Send + Sync>;

/// Lifecycle state of a managed process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Stopped,
    Starting,
    Running,
    Crashed { code: Option<i32> },
}

/// A running (or recently-finished) server process.
pub struct RunningProcess {
    child: GroupChild,
    log_rx: Receiver<LogLine>,
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
            child,
            log_rx,
            logs: LogBuffer::default(),
            status: Status::Running,
            started_at: SystemTime::now(),
            stop_requested: false,
            force_deadline: None,
            wake,
            pid,
        })
    }

    /// Drain pending log lines, detect exit, and escalate shutdown if overdue.
    /// Non-blocking; call once per frame while the process is non-terminal.
    pub fn poll(&mut self) {
        while let Ok(line) = self.log_rx.try_recv() {
            self.logs.push(line.stream, line.text);
        }

        if self.is_terminal() {
            return;
        }

        if let Some(deadline) = self.force_deadline
            && Instant::now() >= deadline
        {
            let _ = self.child.kill();
            self.force_deadline = None;
        }

        match self.child.try_wait() {
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
                // A monitoring failure, distinct from an external kill — surface
                // it, since Status::Crashed{None} can't carry the distinction.
                eprintln!("campfire: try_wait failed for pid {}: {err}", self.pid);
                self.status = Status::Crashed { code: None };
                self.force_deadline = None;
            }
        }
    }

    /// Request shutdown of the whole process group. On Unix, sends SIGTERM and
    /// escalates to SIGKILL after `grace` (via [`RunningProcess::poll`]); on
    /// Windows, the Job Object is terminated at once (no graceful signal exists).
    pub fn stop(&mut self, grace: Duration) {
        if self.stop_requested || self.is_terminal() {
            return;
        }
        self.stop_requested = true;

        #[cfg(unix)]
        {
            if self.child.signal(Signal::SIGTERM).is_ok() {
                self.force_deadline = Some(Instant::now() + grace);
                // Guarantee poll() runs at the deadline to escalate to SIGKILL,
                // even if the render loop is idle (e.g. a minimized window).
                let wake = Arc::clone(&self.wake);
                thread::spawn(move || {
                    thread::sleep(grace);
                    wake();
                });
                return;
            }
        }
        let _ = self.child.kill();
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn logs(&self) -> &LogBuffer {
        &self.logs
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
}

impl Drop for RunningProcess {
    fn drop(&mut self) {
        // Never leak a running server: force-kill the group on drop (app close or
        // process removal). Best-effort — ignore errors from an already-dead child.
        if !self.is_terminal() {
            let _ = self.child.kill();
            let _ = self.child.try_wait();
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
        let found = proc.logs().iter().any(|l| l.text.contains("campfire-marker"));
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
}
