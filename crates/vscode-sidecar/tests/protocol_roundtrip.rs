use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

struct Sidecar {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl Sidecar {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vscode-sidecar"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn vscode-sidecar");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn call(&mut self, id: u64, method: &str, params: serde_json::Value) -> serde_json::Value {
        let request =
            serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{request}").expect("write request");
        self.stdin.flush().expect("flush request");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("parse response")
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn unknown_method_returns_a_json_rpc_error_not_a_hang() {
    let mut sidecar = Sidecar::spawn();

    let response = sidecar.call(1, "does_not_exist", serde_json::json!({}));

    assert_eq!(response["error"]["message"], "unknown method: does_not_exist");
    assert!(response.get("result").is_none());
}

#[test]
fn malformed_request_is_dropped_without_killing_the_sidecar() {
    let mut sidecar = Sidecar::spawn();

    writeln!(sidecar.stdin, "not json").expect("write malformed line");
    sidecar.stdin.flush().expect("flush malformed line");

    let response = sidecar.call(1, "does_not_exist", serde_json::json!({}));
    assert_eq!(response["error"]["message"], "unknown method: does_not_exist");
}
