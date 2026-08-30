use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use repo_service::worker::Worker;

mod dispatch;
mod protocol;

use protocol::{JsonRpcRequest, JsonRpcResponse};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
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

        let response = match dispatch::dispatch(&request.method, request.params, &mut repos) {
            Ok(result) => JsonRpcResponse::ok(request.id, result),
            Err(message) => JsonRpcResponse::err(request.id, message),
        };

        let serialized = serde_json::to_string(&response).expect("response always serializes");
        if writeln!(stdout, "{serialized}").is_err() {
            // The reading end (the extension host) has gone away; exit gracefully rather than
            // panicking, matching the EOF-on-stdin path above.
            break;
        }
        if stdout.flush().is_err() {
            break;
        }
    }
}
