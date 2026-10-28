//! Runs an external program with combined, size-capped output, a timeout and a cancel check.
//!
//! The runner is deliberately generic (it knows nothing about hooks) so other capture-and-wait
//! callers can share it: stdout and stderr go through one pipe (true `2>&1` ordering), only the
//! last `limit` bytes are kept, the child is polled for exit / cancel / deadline, and on Unix it
//! leads its own process group so a timeout or cancel takes its children down with it. The
//! child is always reaped before this returns, so no zombie is left behind.
//!
//! Not for long-lived or detached programs (no capture, no waiting): those want a separate
//! helper. See `docs/superpowers/specs/2026-10-27-parity-batch4e-design.md`.

use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// How long to keep reading after the program exited. A background grandchild that inherited
/// the pipe (and escaped the process group) would otherwise keep it open, and this call
/// blocked, for as long as it lives.
pub(crate) const DRAIN_GRACE: Duration = Duration::from_secs(2);
const OMITTED: &str = "[… earlier output omitted …]\n";

#[derive(Debug)]
pub(crate) struct Captured {
    pub success: bool,
    pub output: String,
    pub timed_out: bool,
}

#[derive(Debug)]
pub(crate) enum RunError {
    Spawn(std::io::Error),
    Wait(std::io::Error),
    Cancelled,
}

/// Keeps the last `limit` bytes: a failing program prints its error at the end.
pub(crate) struct TailBuffer {
    limit: usize,
    bytes: Vec<u8>,
    truncated: bool,
}

impl TailBuffer {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            limit,
            bytes: Vec::new(),
            truncated: false,
        }
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        // Trim in batches (at 2x the limit) so a chatty program costs amortized O(1) per byte
        // while memory stays bounded.
        if self.bytes.len() > self.limit.saturating_mul(2) {
            self.trim();
        }
    }

    fn trim(&mut self) {
        if self.bytes.len() > self.limit {
            let excess = self.bytes.len() - self.limit;
            self.bytes.drain(..excess);
            self.truncated = true;
        }
    }

    pub(crate) fn into_text(mut self) -> String {
        self.trim();
        let text = clean_output(&String::from_utf8_lossy(&self.bytes));
        if self.truncated {
            format!("{OMITTED}{text}")
        } else {
            text
        }
    }
}

/// Removes ANSI escape sequences (CSI `ESC [ … final`, OSC `ESC ] … BEL|ESC \`, two-byte
/// `ESC x`), turns `\r\n` into `\n`, and keeps only the text after the last `\r` of each line so
/// progress redraws collapse to their final state.
pub(crate) fn clean_output(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            plain.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    plain
        .replace("\r\n", "\n")
        .split('\n')
        .map(|line| line.rsplit('\r').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(unix)]
fn spawn_retrying(command: &mut Command) -> std::io::Result<Child> {
    // ETXTBSY: a script written a moment ago can still be open for writing in a child forked by
    // another thread (parallel tests, or the user's editor). It clears within milliseconds.
    const ETXTBSY: i32 = 26;
    let mut attempts = 0;
    loop {
        match command.spawn() {
            Err(error) if error.raw_os_error() == Some(ETXTBSY) && attempts < 5 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(50));
            }
            other => return other,
        }
    }
}

#[cfg(not(unix))]
fn spawn_retrying(command: &mut Command) -> std::io::Result<Child> {
    command.spawn()
}

/// Kills the program and, on Unix, every process in its group (it was spawned as the group
/// leader), so a hook's test runner or linter dies with it. Uses the POSIX `kill` utility rather
/// than a `libc` dependency. Only called while the child is not yet reaped, so its pid (and
/// therefore the group id) cannot have been reused. On Windows only the direct child is
/// terminated.
fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-s", "KILL", "--", &format!("-{}", child.id())])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

/// Kills and reaps: never leaves a zombie behind.
fn kill_and_reap(child: &mut Child) -> std::io::Result<ExitStatus> {
    kill_tree(child);
    child.wait()
}

pub(crate) fn run_captured(
    mut command: Command,
    stdin: Option<&[u8]>,
    cancel: &dyn Fn() -> bool,
    timeout: Duration,
    limit: usize,
) -> Result<Captured, RunError> {
    let (mut reader, writer) = std::io::pipe().map_err(RunError::Spawn)?;
    let writer_for_stderr = writer.try_clone().map_err(RunError::Spawn)?;
    command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(writer)
        .stderr(writer_for_stderr);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own group, so timeout/cancel can kill the whole tree. Known limitation: launched from a
        // terminal, a program that reads `/dev/tty` from this background group is stopped by
        // SIGTTIN until cancel/timeout (a new session would avoid it but needs `libc`).
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let spawned = spawn_retrying(&mut command);
    // `command` still owns the pipe's write ends; until they close the reader never sees EOF.
    drop(command);
    let mut child = spawned.map_err(RunError::Spawn)?;

    // The reader drains the pipe continuously, so a chatty program never blocks on a full pipe.
    let buffer = Arc::new(Mutex::new(TailBuffer::new(limit)));
    let (done_tx, done_rx) = mpsc::channel();
    {
        let buffer = Arc::clone(&buffer);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => buffer
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(&chunk[..read]),
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            let _ = done_tx.send(());
        });
    }
    if let Some(input) = stdin {
        if let Some(mut child_stdin) = child.stdin.take() {
            let input = input.to_vec();
            // Written from its own thread so a program that prints a lot before reading stdin
            // cannot deadlock against us. A program that exits without reading closes its end
            // (EPIPE, ignored); that is reported through its exit status.
            std::thread::spawn(move || {
                let _ = child_stdin.write_all(&input);
            });
        }
    }

    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = kill_and_reap(&mut child);
                return Err(RunError::Wait(error));
            }
        }
        if cancel() {
            let _ = kill_and_reap(&mut child);
            return Err(RunError::Cancelled);
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            break kill_and_reap(&mut child).map_err(RunError::Wait)?;
        }
        std::thread::sleep(POLL_INTERVAL);
    };
    let _ = done_rx.recv_timeout(DRAIN_GRACE);
    let collected = std::mem::replace(
        &mut *buffer.lock().unwrap_or_else(|error| error.into_inner()),
        TailBuffer::new(0),
    );
    Ok(Captured {
        success: status.success() && !timed_out,
        output: collected.into_text(),
        timed_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_buffer_keeps_the_last_bytes_and_marks_truncation() {
        let mut buffer = TailBuffer::new(4);
        buffer.push(b"abc");
        buffer.push(b"defgh");
        assert_eq!(buffer.into_text(), "[… earlier output omitted …]\nefgh");

        let mut short = TailBuffer::new(8);
        short.push(b"ok\n");
        assert_eq!(short.into_text(), "ok\n");
    }

    #[test]
    fn tail_buffer_stays_bounded_for_a_chatty_program() {
        let mut buffer = TailBuffer::new(16);
        for _ in 0..10_000 {
            buffer.push(b"0123456789");
        }
        assert!(buffer.bytes.len() <= 32);
    }

    #[test]
    fn clean_output_strips_escapes_and_carriage_return_redraws() {
        assert_eq!(
            clean_output("\u{1b}[31merror\u{1b}[0m: bad\r\n"),
            "error: bad\n"
        );
        assert_eq!(clean_output("\u{1b}]0;title\u{7}done"), "done");
        assert_eq!(clean_output("10%\r50%\r100%\nnext"), "100%\nnext");
    }

    #[cfg(unix)]
    fn sh(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[cfg(unix)]
    #[test]
    fn combines_stdout_and_stderr_in_order() {
        let captured = run_captured(
            sh("echo one; echo two >&2; echo three"),
            None,
            &|| false,
            Duration::from_secs(30),
            1024,
        )
        .unwrap();
        assert!(captured.success);
        assert!(!captured.timed_out);
        assert_eq!(captured.output, "one\ntwo\nthree\n");
    }

    #[cfg(unix)]
    #[test]
    fn delivers_stdin_and_reports_failure() {
        let captured = run_captured(
            sh("cat; exit 3"),
            Some(b"from stdin\n"),
            &|| false,
            Duration::from_secs(30),
            1024,
        )
        .unwrap();
        assert!(!captured.success);
        assert_eq!(captured.output, "from stdin\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_program_that_fills_the_pipe_does_not_deadlock() {
        // ~1 MiB of output, far beyond a pipe buffer, while we keep only the tail.
        let captured = run_captured(
            sh("i=0; while [ $i -lt 20000 ]; do echo 'line of output padding padding padding'; i=$((i+1)); done; echo last"),
            None,
            &|| false,
            Duration::from_secs(60),
            1024,
        )
        .unwrap();
        assert!(captured.success);
        assert!(captured
            .output
            .starts_with("[… earlier output omitted …]\n"));
        assert!(captured.output.ends_with("last\n"));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_a_process_whose_grandchild_holds_the_pipe() {
        let captured = run_captured(
            sh("sleep 30 & echo started; sleep 30"),
            None,
            &|| false,
            Duration::from_millis(500),
            1024,
        )
        .unwrap();
        assert!(captured.timed_out);
        assert!(!captured.success);
        assert_eq!(captured.output, "started\n");
    }

    #[cfg(unix)]
    #[test]
    fn cancel_stops_the_process() {
        let started = Instant::now();
        let result = run_captured(
            sh("sleep 30"),
            None,
            &|| true,
            Duration::from_secs(60),
            1024,
        );
        assert!(matches!(result, Err(RunError::Cancelled)));
        // Generous bound: only proves we did not wait for `sleep 30`.
        assert!(started.elapsed() < Duration::from_secs(20));
    }

    #[test]
    fn a_missing_program_is_a_spawn_error() {
        let result = run_captured(
            Command::new("browsitory-no-such-program"),
            None,
            &|| false,
            Duration::from_secs(1),
            1024,
        );
        assert!(
            matches!(result, Err(RunError::Spawn(ref error)) if error.kind() == std::io::ErrorKind::NotFound)
        );
    }
}
