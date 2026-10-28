//! Running programs that are not git.
//!
//! Two shapes live here:
//! - `run_captured` (batch 4e, hooks): combined, size-capped output, a timeout and a cancel
//!   check. Stdout and stderr go through one pipe (true `2>&1` ordering), only the last `limit`
//!   bytes are kept, the child is polled for exit / cancel / deadline, and on Unix it leads its
//!   own process group so a timeout or cancel takes its children down with it. The child is
//!   always reaped before this returns, so no zombie is left behind.
//! - `spawn` / `spawn_detached` (batch 4f, external tools): long-lived programs (editors,
//!   terminals, diff/merge tools) with stdin/stdout closed and stderr optionally drained into a
//!   tail buffer (`drain_tail`).
//!
//! Nothing here goes through a shell. User-configured command lines are split into argv by
//! `split_command_line` (POSIX or Windows rules) and programs are resolved to absolute paths by
//! `find_program`, which only searches absolute `PATH` entries, so neither the current directory
//! nor a repository's work tree can shadow a program. See
//! `docs/superpowers/specs/2026-10-27-parity-batch4e-design.md` and
//! `docs/superpowers/specs/2026-10-27-parity-batch4f-design.md`.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use thiserror::Error;

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

    /// The last `limit` bytes seen so far.
    pub(crate) fn tail_bytes(&self) -> &[u8] {
        &self.bytes[self.bytes.len().saturating_sub(self.limit)..]
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

/// On Unix the program leads its own process group, so timeout/cancel can kill the whole tree;
/// on Windows it gets no console window.
fn own_group_without_window(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Known limitation: launched from a terminal, a program that reads `/dev/tty` from this
        // background group is stopped by SIGTTIN until cancel/timeout (a new session would avoid
        // it but needs `setsid`).
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

/// Writes `stdin` from its own thread so a program that prints a lot before reading stdin cannot
/// deadlock against us. A program that exits without reading closes its end (EPIPE, ignored);
/// that is reported through its exit status.
fn feed_stdin(child: &mut Child, stdin: Option<&[u8]>) {
    if let Some(input) = stdin {
        if let Some(mut child_stdin) = child.stdin.take() {
            let input = input.to_vec();
            std::thread::spawn(move || {
                let _ = child_stdin.write_all(&input);
            });
        }
    }
}

/// Polls `child` for exit, `cancel()` and the deadline. On cancel or timeout the program (and on
/// Unix its process group) is killed and reaped. Returns the exit status and whether it timed out.
fn wait_bounded(
    child: &mut Child,
    cancel: &dyn Fn() -> bool,
    timeout: Duration,
) -> Result<(ExitStatus, bool), RunError> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok((status, false)),
            Ok(None) => {}
            Err(error) => {
                let _ = kill_and_reap(child);
                return Err(RunError::Wait(error));
            }
        }
        if cancel() {
            let _ = kill_and_reap(child);
            return Err(RunError::Cancelled);
        }
        if started.elapsed() >= timeout {
            return kill_and_reap(child)
                .map(|status| (status, true))
                .map_err(RunError::Wait);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
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
    own_group_without_window(&mut command);
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
    feed_stdin(&mut child, stdin);
    let (status, timed_out) = wait_bounded(&mut child, cancel, timeout)?;
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

/// What `run_captured_split` collected.
#[derive(Debug)]
pub(crate) struct SplitCaptured {
    pub success: bool,
    pub timed_out: bool,
    /// The first `stdout_limit` bytes of stdout.
    pub stdout: Vec<u8>,
    /// More than `stdout_limit` bytes arrived: `stdout` is incomplete.
    pub stdout_overflowed: bool,
    /// The last `stderr_limit` bytes of stderr.
    pub stderr: String,
}

/// Like `run_captured`, but with stdout and stderr kept apart, for programs whose stdout is data
/// (a signature) and whose stderr is diagnostics. Stdout keeps its *head* and reports overflow,
/// so a caller never mistakes a truncated result for a complete one; stderr keeps its tail.
pub(crate) fn run_captured_split(
    mut command: Command,
    stdin: Option<&[u8]>,
    cancel: &dyn Fn() -> bool,
    timeout: Duration,
    stdout_limit: usize,
    stderr_limit: usize,
) -> Result<SplitCaptured, RunError> {
    command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    own_group_without_window(&mut command);
    let mut child = spawn_retrying(&mut command).map_err(RunError::Spawn)?;
    drop(command);

    // Both pipes drain continuously, so a chatty program never blocks on a full pipe.
    let (done_tx, done_rx) = mpsc::channel();
    let head = Arc::new(Mutex::new((Vec::new(), false)));
    if let Some(mut out) = child.stdout.take() {
        let head = Arc::clone(&head);
        let done_tx = done_tx.clone();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match out.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => {
                        let mut head = head.lock().unwrap_or_else(|error| error.into_inner());
                        let (bytes, overflowed) = &mut *head;
                        let room = stdout_limit.saturating_sub(bytes.len());
                        bytes.extend_from_slice(&chunk[..read.min(room)]);
                        if read > room {
                            *overflowed = true;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            let _ = done_tx.send(());
        });
    } else {
        let _ = done_tx.send(());
    }
    let tail = Arc::new(Mutex::new(TailBuffer::new(stderr_limit)));
    if let Some(mut err) = child.stderr.take() {
        let tail = Arc::clone(&tail);
        let done_tx = done_tx.clone();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match err.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => tail
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(&chunk[..read]),
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            let _ = done_tx.send(());
        });
    } else {
        let _ = done_tx.send(());
    }
    drop(done_tx);
    feed_stdin(&mut child, stdin);
    let (status, timed_out) = wait_bounded(&mut child, cancel, timeout)?;
    // Both drains, sharing one grace period.
    let deadline = Instant::now() + DRAIN_GRACE;
    for _ in 0..2 {
        let left = deadline.saturating_duration_since(Instant::now());
        if done_rx.recv_timeout(left).is_err() {
            break;
        }
    }
    let (stdout, stdout_overflowed) =
        std::mem::take(&mut *head.lock().unwrap_or_else(|error| error.into_inner()));
    let stderr = std::mem::replace(
        &mut *tail.lock().unwrap_or_else(|error| error.into_inner()),
        TailBuffer::new(0),
    );
    Ok(SplitCaptured {
        success: status.success() && !timed_out,
        timed_out,
        stdout,
        stdout_overflowed,
        stderr: stderr.into_text(),
    })
}

// ---------------------------------------------------------------------------------------------
// External tools (batch 4f): command-line splitting, program lookup, detached spawn.
// ---------------------------------------------------------------------------------------------

/// Which quoting rules `split_command_line` applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStyle {
    /// POSIX shell word splitting without any expansion: `'…'`, `"…"` (with `\"`, `\\`, `\$`,
    /// `` \` `` escapes) and a backslash escaping the next character outside quotes.
    Posix,
    /// `CommandLineToArgvW`: backslashes are literal unless they precede a `"`.
    Windows,
}

impl CommandStyle {
    pub fn native() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Posix
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SplitError {
    #[error("The command is empty.")]
    Empty,
    #[error("The command has an unclosed quote.")]
    UnclosedQuote,
}

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("Couldn't find the program '{0}'.")]
    NotFound(String),
    #[error(transparent)]
    Split(#[from] SplitError),
    #[error("Couldn't start '{program}': {source}")]
    Io {
        program: String,
        source: std::io::Error,
    },
}

/// Splits a user-configured command line into argv. Nothing is expanded: `$VAR`, `~`, globs,
/// `;`, `|` and backticks stay literal characters, because the result is never given to a
/// shell.
pub fn split_command_line(input: &str, style: CommandStyle) -> Result<Vec<String>, SplitError> {
    let words = match style {
        CommandStyle::Posix => split_posix(input)?,
        CommandStyle::Windows => split_windows(input)?,
    };
    if words.is_empty() {
        return Err(SplitError::Empty);
    }
    Ok(words)
}

fn split_posix(input: &str) -> Result<Vec<String>, SplitError> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => current.push(c),
                        None => return Err(SplitError::UnclosedQuote),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(c @ ('"' | '\\' | '$' | '`')) => current.push(c),
                            Some(c) => {
                                current.push('\\');
                                current.push(c);
                            }
                            None => return Err(SplitError::UnclosedQuote),
                        },
                        Some(c) => current.push(c),
                        None => return Err(SplitError::UnclosedQuote),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(c) = chars.next() {
                    current.push(c);
                }
            }
            c => {
                in_word = true;
                current.push(c);
            }
        }
    }
    if in_word {
        words.push(current);
    }
    Ok(words)
}

/// `CommandLineToArgvW` rules: 2n backslashes + `"` → n backslashes and a quote toggle; 2n+1
/// backslashes + `"` → n backslashes and a literal `"`; other backslashes are literal; `""`
/// inside quotes is a literal `"`. An unclosed quote is an error here (Windows tolerates it).
fn split_windows(input: &str) -> Result<Vec<String>, SplitError> {
    let chars: Vec<char> = input.chars().collect();
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quoted = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let start = i;
            while i < chars.len() && chars[i] == '\\' {
                i += 1;
            }
            let n = i - start;
            in_word = true;
            if i < chars.len() && chars[i] == '"' {
                current.extend(std::iter::repeat_n('\\', n / 2));
                if n % 2 == 1 {
                    current.push('"');
                    i += 1;
                }
            } else {
                current.extend(std::iter::repeat_n('\\', n));
            }
            continue;
        }
        if c == '"' {
            in_word = true;
            if quoted && chars.get(i + 1) == Some(&'"') {
                current.push('"');
                i += 2;
                continue;
            }
            quoted = !quoted;
            i += 1;
            continue;
        }
        if c.is_whitespace() && !quoted {
            if in_word {
                words.push(std::mem::take(&mut current));
                in_word = false;
            }
            i += 1;
            continue;
        }
        in_word = true;
        current.push(c);
        i += 1;
    }
    if quoted {
        return Err(SplitError::UnclosedQuote);
    }
    if in_word {
        words.push(current);
    }
    Ok(words)
}

/// Resolves `name` to an absolute program path through `PATH` (and `PATHEXT` on Windows).
pub fn find_program(name: &str) -> Option<PathBuf> {
    let dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };
    find_program_in(name, &dirs, &exts)
}

/// `name` containing a path separator must be absolute and is returned when it exists (trying
/// `exts` too). A bare name is looked up in each **absolute** entry of `dirs`, in order. Relative
/// entries (`.`, `bin`) are skipped: they would resolve against Browsitory's current directory,
/// and a program must never be picked up from wherever that happens to be.
pub fn find_program_in(name: &str, dirs: &[PathBuf], exts: &[String]) -> Option<PathBuf> {
    let candidates = |base: PathBuf| -> Vec<PathBuf> {
        let mut out = Vec::new();
        if exts.is_empty() || base.extension().is_some() {
            out.push(base.clone());
        }
        for ext in exts {
            let mut with_ext = base.clone().into_os_string();
            with_ext.push(ext);
            out.push(PathBuf::from(with_ext));
        }
        out
    };
    if name.is_empty() {
        return None;
    }
    if name.contains('/') || name.contains('\\') {
        let path = PathBuf::from(name);
        if !path.is_absolute() {
            return None;
        }
        return candidates(path).into_iter().find(|p| is_program(p));
    }
    dirs.iter()
        .filter(|dir| dir.is_absolute())
        .flat_map(|dir| candidates(dir.join(name)))
        .find(|p| is_program(p))
}

/// A regular file with an execute bit (Unix, like a shell's `PATH` search), or on Windows any
/// file, including an app execution alias such as `WindowsApps\wt.exe`, which `metadata`
/// cannot follow.
fn is_program(path: &std::path::Path) -> bool {
    match std::fs::metadata(path) {
        #[cfg(unix)]
        Ok(meta) => {
            use std::os::unix::fs::PermissionsExt;
            meta.is_file() && meta.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        Ok(meta) => meta.is_file(),
        Err(_) => cfg!(windows) && std::fs::symlink_metadata(path).is_ok_and(|meta| !meta.is_dir()),
    }
}

/// Git for Windows' `sh.exe`: next to the `git.exe` found on `PATH` (see
/// `hooks::sh_candidates`), else an `sh.exe` on `PATH`.
#[cfg(windows)]
pub fn find_sh() -> Option<PathBuf> {
    find_program("git.exe")
        .and_then(|git| {
            crate::hooks::sh_candidates(&git)
                .into_iter()
                .find(|path| path.is_file())
        })
        .or_else(|| find_program("sh.exe"))
}

/// A batch file (`.cmd`/`.bat`, such as VS Code's `code.cmd`) runs in `cmd.exe`, which would
/// flash or keep open a console window on Windows while the program waits (`code --wait`).
pub fn hides_console(program: &std::path::Path) -> bool {
    program
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"))
}

/// Variables that pin git to one repository/index/prefix. Removed from every spawned program:
/// a tool or terminal opened for one repository must not operate on whichever repository
/// Browsitory itself was launched for (from a git hook or alias, say).
pub(crate) const REPO_PINNING_ENV: [&str; 4] =
    ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_PREFIX"];

/// A program to start. `program` should be absolute (see `find_program`); `args` reach it
/// verbatim, with no shell in between.
#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, OsString)>,
    /// Windows `CREATE_NEW_CONSOLE` (a terminal such as `cmd /K`).
    pub new_console: bool,
    /// Windows `CREATE_NO_WINDOW` (`sh.exe` running a GUI tool).
    pub no_window: bool,
    pub capture_stderr: bool,
}

/// stdin/stdout null; stderr null or piped. Unix: its own process group, so a Ctrl+C in the
/// terminal Browsitory was started from does not take the tool down. Windows:
/// `CREATE_NEW_PROCESS_GROUP` plus the console flags above.
pub fn spawn(spec: &SpawnSpec) -> Result<Child, LaunchError> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(if spec.capture_stderr {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    for key in REPO_PINNING_ENV {
        command.env_remove(key);
    }
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut flags = CREATE_NEW_PROCESS_GROUP;
        if spec.new_console {
            flags |= CREATE_NEW_CONSOLE;
        } else if spec.no_window {
            flags |= CREATE_NO_WINDOW;
        }
        command.creation_flags(flags);
    }
    spawn_retrying(&mut command).map_err(|source| {
        let program = spec.program.display().to_string();
        if source.kind() == std::io::ErrorKind::NotFound {
            LaunchError::NotFound(program)
        } else {
            LaunchError::Io { program, source }
        }
    })
}

/// Spawns and reaps on a background thread, so no zombie is left behind.
pub fn spawn_detached(spec: &SpawnSpec) -> Result<(), LaunchError> {
    let mut child = spawn(spec)?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Reads `source` until EOF on its own thread, keeping only the last `cap` bytes. Draining to
/// the end means a chatty tool never blocks on (or dies of `SIGPIPE` from) a full pipe.
pub(crate) fn drain_tail(
    mut source: impl Read + Send + 'static,
    cap: usize,
) -> (Arc<Mutex<TailBuffer>>, JoinHandle<()>) {
    let tail = Arc::new(Mutex::new(TailBuffer::new(cap)));
    let sink = Arc::clone(&tail);
    let handle = std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match source.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => sink
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(&chunk[..n]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
    });
    (tail, handle)
}

/// The last three non-empty lines (escape sequences removed, lossy UTF-8), or "no output".
pub fn tail_text(bytes: &[u8]) -> String {
    let text = clean_output(&String::from_utf8_lossy(bytes));
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let tail = lines[lines.len().saturating_sub(3)..].join("\n");
    if tail.is_empty() {
        "no output".to_string()
    } else {
        tail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use CommandStyle::{Posix, Windows};

    fn split(input: &str, style: CommandStyle) -> Vec<String> {
        split_command_line(input, style).unwrap()
    }

    #[test]
    fn posix_splits_on_whitespace_and_honours_quotes_and_escapes() {
        assert_eq!(
            split("code  --goto \"{file}:{line}\"", Posix),
            ["code", "--goto", "{file}:{line}"]
        );
        assert_eq!(
            split(r#"'a b' c\ d "e\"f" "g\h" '\n'"#, Posix),
            ["a b", "c d", "e\"f", "g\\h", "\\n"]
        );
        assert_eq!(split(r#"x "" y"#, Posix), ["x", "", "y"]);
        // No shell: `$`, `;`, `|` and backticks are plain characters.
        assert_eq!(
            split("ed $HOME;rm `x` | y", Posix),
            ["ed", "$HOME;rm", "`x`", "|", "y"]
        );
    }

    #[test]
    fn posix_rejects_empty_and_unclosed_quotes() {
        assert_eq!(split_command_line("   ", Posix), Err(SplitError::Empty));
        assert_eq!(
            split_command_line("\"abc", Posix),
            Err(SplitError::UnclosedQuote)
        );
        assert_eq!(
            split_command_line("'abc", Posix),
            Err(SplitError::UnclosedQuote)
        );
    }

    #[test]
    fn windows_keeps_backslashes_literal_except_before_quotes() {
        assert_eq!(
            split(
                r#""C:\Program Files\Sublime Text\subl.exe" {file}:{line}"#,
                Windows
            ),
            [r"C:\Program Files\Sublime Text\subl.exe", "{file}:{line}"]
        );
        assert_eq!(split(r#"a\\\"b"#, Windows), [r#"a\"b"#]);
        assert_eq!(split(r#""a\\" b"#, Windows), [r"a\", "b"]);
        assert_eq!(split(r"C:\dir\ x", Windows), [r"C:\dir\", "x"]);
        assert_eq!(split(r#""say ""hi""""#, Windows), [r#"say "hi""#]);
        assert_eq!(split_command_line("  ", Windows), Err(SplitError::Empty));
        assert_eq!(
            split_command_line("\"abc", Windows),
            Err(SplitError::UnclosedQuote)
        );
    }

    #[test]
    fn find_program_in_searches_absolute_dirs_and_tries_extensions() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("tool"), "").unwrap();
        std::fs::write(dir.path().join("code.cmd"), "").unwrap();
        make_executable(&dir.path().join("tool"));
        make_executable(&dir.path().join("code.cmd"));
        let dirs = vec![
            PathBuf::from("/definitely/missing"),
            dir.path().to_path_buf(),
        ];
        assert_eq!(
            find_program_in("tool", &dirs, &[]),
            Some(dir.path().join("tool"))
        );
        let exts = vec![".EXE".to_string(), ".cmd".to_string()];
        assert_eq!(
            find_program_in("code", &dirs, &exts),
            Some(dir.path().join("code.cmd"))
        );
        assert_eq!(find_program_in("nope", &dirs, &exts), None);
        let explicit = dir.path().join("tool");
        assert_eq!(
            find_program_in(explicit.to_str().unwrap(), &[], &[]),
            Some(explicit)
        );
    }

    #[test]
    fn find_program_in_never_resolves_relative_to_the_current_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("bin")).unwrap();
        std::fs::write(dir.path().join("bin").join("tool"), "").unwrap();
        make_executable(&dir.path().join("bin").join("tool"));
        // A relative `PATH` entry (".", "bin") is skipped, and a relative explicit path is not
        // a program: either would resolve against whatever directory Browsitory runs in.
        let relative = vec![PathBuf::from("."), PathBuf::from("bin")];
        assert_eq!(find_program_in("tool", &relative, &[]), None);
        assert_eq!(find_program_in("bin/tool", &[], &[]), None);
        assert_eq!(find_program_in("./tool", &[], &[]), None);
    }

    fn make_executable(path: &std::path::Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = path;
    }

    #[cfg(unix)]
    #[test]
    fn find_program_in_skips_files_without_an_execute_bit() {
        let first = tempfile::TempDir::new().unwrap();
        let second = tempfile::TempDir::new().unwrap();
        std::fs::write(first.path().join("tool"), "").unwrap();
        std::fs::write(second.path().join("tool"), "").unwrap();
        make_executable(&second.path().join("tool"));
        let dirs = vec![first.path().to_path_buf(), second.path().to_path_buf()];
        assert_eq!(
            find_program_in("tool", &dirs, &[]),
            Some(second.path().join("tool"))
        );
    }

    #[test]
    fn drain_tail_keeps_only_the_last_bytes() {
        let input: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let (tail, reader) = drain_tail(std::io::Cursor::new(input.clone()), 4096);
        reader.join().unwrap();
        let kept = tail.lock().unwrap().tail_bytes().to_vec();
        assert_eq!(kept, input[input.len() - 4096..].to_vec());
    }

    #[test]
    fn tail_text_keeps_the_last_three_non_empty_lines() {
        assert_eq!(tail_text(b"a\n\nb\nc\n  d  \n\n"), "b\nc\nd");
        assert_eq!(tail_text(b"\x1b[31moops\x1b[0m\r\n"), "oops");
        assert_eq!(tail_text(b""), "no output");
    }

    #[cfg(unix)]
    fn spec(program: &str, args: &[&str]) -> SpawnSpec {
        SpawnSpec {
            program: PathBuf::from(program),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: std::env::temp_dir(),
            env: vec![("BROWSITORY_TEST_VAR".into(), "a value".into())],
            new_console: false,
            no_window: false,
            capture_stderr: true,
        }
    }

    #[cfg(unix)]
    #[test]
    fn spawn_passes_argv_verbatim_and_sets_env() {
        // `$1` is a literal argument: no shell ever sees it.
        let mut child = spawn(&spec(
            "/bin/sh",
            &[
                "-c",
                "printf '%s|%s' \"$1\" \"$BROWSITORY_TEST_VAR\" >&2",
                "sh",
                "$(touch /nonexistent); x",
            ],
        ))
        .unwrap();
        let (tail, reader) = drain_tail(child.stderr.take().unwrap(), 4096);
        assert!(child.wait().unwrap().success());
        reader.join().unwrap();
        let text = String::from_utf8(tail.lock().unwrap().tail_bytes().to_vec()).unwrap();
        assert_eq!(text, "$(touch /nonexistent); x|a value");
    }

    #[test]
    fn spawning_a_missing_program_is_not_found() {
        let err = spawn(&SpawnSpec {
            program: PathBuf::from("browsitory-no-such-program"),
            args: Vec::new(),
            cwd: std::env::temp_dir(),
            env: Vec::new(),
            new_console: false,
            no_window: false,
            capture_stderr: false,
        })
        .unwrap_err();
        assert!(matches!(err, LaunchError::NotFound(_)), "{err}");
    }

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

    #[cfg(unix)]
    #[test]
    fn split_keeps_stdout_and_stderr_apart() {
        let captured = run_captured_split(
            sh("printf out; printf err >&2"),
            None,
            &|| false,
            Duration::from_secs(30),
            1024,
            1024,
        )
        .unwrap();
        assert!(captured.success);
        assert_eq!(captured.stdout, b"out");
        assert_eq!(captured.stderr, "err");
        assert!(!captured.stdout_overflowed);
    }

    #[cfg(unix)]
    #[test]
    fn split_feeds_stdin() {
        let captured = run_captured_split(
            sh("cat"),
            Some(b"payload"),
            &|| false,
            Duration::from_secs(30),
            1024,
            1024,
        )
        .unwrap();
        assert_eq!(captured.stdout, b"payload");
    }

    #[cfg(unix)]
    #[test]
    fn split_times_out_and_kills_the_group() {
        let started = Instant::now();
        let captured = run_captured_split(
            sh("sleep 30 & echo started; sleep 30"),
            None,
            &|| false,
            Duration::from_millis(300),
            1024,
            1024,
        )
        .unwrap();
        assert!(captured.timed_out);
        assert!(!captured.success);
        assert_eq!(captured.stdout, b"started\n");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[cfg(unix)]
    #[test]
    fn split_cancel_stops_the_program() {
        let started = Instant::now();
        let result = run_captured_split(
            sh("sleep 30"),
            None,
            &|| started.elapsed() > Duration::from_millis(100),
            Duration::from_secs(60),
            1024,
            1024,
        );
        assert!(matches!(result, Err(RunError::Cancelled)));
        assert!(started.elapsed() < Duration::from_secs(20));
    }

    #[cfg(unix)]
    #[test]
    fn split_reports_stdout_overflow_and_keeps_the_head() {
        let captured = run_captured_split(
            sh("head -c 2000 /dev/zero; echo tail >&2"),
            None,
            &|| false,
            Duration::from_secs(30),
            1000,
            1024,
        )
        .unwrap();
        assert!(captured.stdout_overflowed);
        assert_eq!(captured.stdout.len(), 1000);
        assert_eq!(captured.stderr, "tail\n");
    }

    #[test]
    fn split_spawn_failure_is_a_spawn_error() {
        let result = run_captured_split(
            Command::new("browsitory-no-such-program"),
            None,
            &|| false,
            Duration::from_secs(1),
            1024,
            1024,
        );
        assert!(
            matches!(result, Err(RunError::Spawn(ref error)) if error.kind() == std::io::ErrorKind::NotFound)
        );
    }
}
