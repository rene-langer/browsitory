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

fn write_file(dir: &std::path::Path, relative_path: &str, contents: &str) {
    std::fs::write(dir.join(relative_path), contents).expect("write file");
}

fn init_repo() -> (tempfile::TempDir, git2::Repository) {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let repo = git2::Repository::init(dir.path()).expect("init repo");
    let mut config = repo.config().expect("repo config");
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    (dir, repo)
}

#[test]
fn open_status_close_round_trip_through_the_sidecar() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "untracked.txt", "hello");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();

    let open = sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));
    assert_eq!(open["result"], serde_json::Value::Null);

    let status = sidecar.call(2, "get_status", serde_json::json!({"repoPath": repo_path}));
    let entries = status["result"].as_array().expect("status result array");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["path"], "untracked.txt");
    assert_eq!(entries[0]["kind"], "New");

    let close = sidecar.call(3, "close_repo", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(close["result"], serde_json::Value::Null);
}

#[test]
fn get_status_on_an_unopened_repo_returns_a_json_rpc_error() {
    let mut sidecar = Sidecar::spawn();

    let status = sidecar.call(1, "get_status", serde_json::json!({"repoPath": "/no/such/repo"}));

    assert!(status["error"]["message"]
        .as_str()
        .unwrap()
        .contains("repo not open"));
    assert!(status.get("result").is_none());
}
