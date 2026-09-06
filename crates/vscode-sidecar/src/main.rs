use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use std::thread;

mod dispatch;
mod protocol;

use protocol::{JsonRpcRequest, JsonRpcResponse};

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
/// `repos` is therefore an `Arc<Mutex<...>>` (`dispatch::Repos`) rather than an owned `HashMap`,
/// shared by every in-flight request thread. `dispatch.rs`'s handlers only ever hold that lock
/// long enough to mutate the map or clone a `WorkerHandle` out of it (see `worker_handle`'s doc
/// comment) — never across the blocking reply-wait — so the lock itself can't reintroduce the
/// same serialization.
///
/// Responses (and the transfer-progress notifications `spawn_progress_relay` emits) are written
/// through the shared `stdout` mutex, which is what keeps concurrent writers from interleaving a
/// line; see that function's doc comment in `dispatch.rs`.
fn main() {
    let stdin = io::stdin();
    let stdout = Arc::new(Mutex::new(io::stdout()));
    let repos: dispatch::Repos = Arc::new(Mutex::new(HashMap::new()));

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
        thread::spawn(move || {
            let response =
                match dispatch::dispatch(&request.method, request.params, &repos, &stdout) {
                    Ok(result) => JsonRpcResponse::ok(request.id, result),
                    Err(message) => JsonRpcResponse::err(request.id, message),
                };

            let serialized = serde_json::to_string(&response).expect("response always serializes");
            let written = {
                let mut out = stdout.lock().unwrap_or_else(|error| error.into_inner());
                writeln!(out, "{serialized}").and_then(|()| out.flush())
            };
            if written.is_err() {
                // The reading end (the extension host) has gone away. Each request now runs on
                // its own thread, so there's no single loop left to `break` out of gracefully as
                // the pre-concurrency version did — exit the whole process instead, matching the
                // same "reading end is gone, stop" outcome.
                std::process::exit(0);
            }
        });
    }
}
