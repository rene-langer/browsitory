use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod dispatch;
mod protocol;

use protocol::{JsonRpcRequest, JsonRpcResponse};

/// `exit_grace`'s default, the extension's `SIDECAR_EXIT_GRACE_MS`.
const EXIT_GRACE: Duration = Duration::from_secs(5);

/// One JSON-RPC request in, one response line out — but each request is dispatched on its own
/// thread rather than inline in this loop. `dispatch::dispatch` blocks the calling thread on a
/// per-repo `Worker`'s reply channel for the whole duration of the underlying git operation
/// (`get_blame`, a big `get_commit_graph`, etc.); if that happened on *this* thread, the next
/// line of stdin wouldn't be read again until it returned, so one slow request against one open
/// repo would stall every other request against every other open repo — exactly the freeze the
/// desktop app's async-command design (see `docs/ARCHITECTURE.md`) exists to avoid. Spawning a
/// thread per request means this loop is only ever busy for the time it takes to parse a line and
/// spawn a thread, so it's always ready to read the next one.
///
/// `repos` is therefore an `Arc<WorkerRegistry>` (`dispatch::Repos`), shared by every in-flight
/// request thread. The registry holds its lock only long enough to update its maps or clone a
/// `WorkerHandle` out of them (see `worker_handle`'s doc comment) — never across the blocking
/// reply-wait — so the lock itself can't reintroduce the same serialization.
///
/// Responses (and the transfer-progress notifications `spawn_progress_relay` emits) are written
/// through the shared `stdout` mutex, which is what keeps concurrent writers from interleaving a
/// line; see that function's doc comment in `dispatch/remote.rs`.
fn main() {
    // Before any repository is opened: process-wide libgit2 settings (#840).
    git_core::repo::configure_libgit2();

    let stdin = io::stdin();
    let stdout = Arc::new(Mutex::new(io::stdout()));
    let repos: dispatch::Repos = Arc::default();
    // Set once an answer can't be written: nothing reads stdout any more.
    let reader_gone = Arc::new(AtomicBool::new(false));
    // The request threads still running, finished ones dropped as new requests arrive.
    let mut running: Vec<thread::JoinHandle<()>> = Vec::new();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                eprintln!("vscode-sidecar: dropping malformed request: {error}");
                continue;
            }
        };

        let repos = Arc::clone(&repos);
        let stdout = Arc::clone(&stdout);
        let reader_gone = Arc::clone(&reader_gone);
        running.retain(|request| !request.is_finished());
        running.push(thread::spawn(move || {
            let response =
                match dispatch::dispatch(&request.method, request.params, &repos, &stdout) {
                    Ok(result) => JsonRpcResponse::ok(request.id, result),
                    Err(error) => JsonRpcResponse::err(request.id, error),
                };

            if reader_gone.load(Ordering::Relaxed) {
                return;
            }
            let serialized = serde_json::to_string(&response).expect("response always serializes");
            let mut out = stdout.lock().unwrap_or_else(|error| error.into_inner());
            if writeln!(out, "{serialized}")
                .and_then(|()| out.flush())
                .is_err()
            {
                // The extension host has gone away (a window reload or close). The other
                // requests keep running to their end (#1129): exiting here would cut a
                // checkout or an index write off halfway. Stdin ends with the host too, and
                // `main` then returns below.
                reader_gone.store(true, Ordering::Relaxed);
            }
        }));
    }

    // Stdin ended: the extension closes it when its panel closes (#961), and it ends when the
    // extension host exits. Returning from `main` would end the process at once, cutting a
    // running request off wherever it is; in the middle of a libgit2 index write that leaves
    // `.git/index.lock` behind, and every later git command in the repository fails on it. So
    // the running requests finish (and are answered) first, for at most the grace period. The
    // extension's own kill after its grace period is a timer in the extension host, which never
    // fires when the host itself exits (#1129).
    let deadline = Instant::now() + exit_grace();
    loop {
        running.retain(|request| !request.is_finished());
        if running.is_empty() {
            return;
        }
        if Instant::now() >= deadline {
            eprintln!(
                "vscode-sidecar: {} request(s) still running after the exit grace period; exiting",
                running.len()
            );
            std::process::exit(1);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// How long the running requests may take after stdin ended: the extension's
/// `SIDECAR_EXIT_GRACE_MS`. Only the tests set `BROWSITORY_SIDECAR_TEST_EXIT_GRACE_MS`.
fn exit_grace() -> Duration {
    std::env::var("BROWSITORY_SIDECAR_TEST_EXIT_GRACE_MS")
        .ok()
        .and_then(|millis| millis.parse().ok())
        .map_or(EXIT_GRACE, Duration::from_millis)
}

// Hide the developer's / CI runner's global and system git config from every test (#111).
#[cfg(test)]
test_support::hermetic_git!();
