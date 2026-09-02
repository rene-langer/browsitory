use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};

use repo_service::worker::Worker;

mod dispatch;
mod protocol;

use protocol::{JsonRpcRequest, JsonRpcResponse};

fn main() {
    let stdin = io::stdin();
    // Shared with the per-transfer notification relay threads `dispatch` spawns (see
    // `dispatch::spawn_progress_relay`): both sides take this lock for the whole
    // write-plus-flush of one line, so a `transferProgress` notification can never interleave
    // with a response mid-line.
    let stdout = Arc::new(Mutex::new(io::stdout()));
    let mut repos: HashMap<String, Worker> = HashMap::new();

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

        let response =
            match dispatch::dispatch(&request.method, request.params, &mut repos, &stdout) {
                Ok(result) => JsonRpcResponse::ok(request.id, result),
                Err(message) => JsonRpcResponse::err(request.id, message),
            };

        let serialized = serde_json::to_string(&response).expect("response always serializes");
        let written = {
            let mut out = stdout.lock().unwrap_or_else(|error| error.into_inner());
            writeln!(out, "{serialized}").and_then(|()| out.flush())
        };
        if written.is_err() {
            // The reading end (the extension host) has gone away; exit gracefully rather than
            // panicking, matching the EOF-on-stdin path above.
            break;
        }
    }
}
