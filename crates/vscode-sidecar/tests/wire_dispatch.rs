//! #306: the sidecar's exact response lines, end to end. Each request goes through the real
//! binary (`main.rs`'s loop, `dispatch`, `JsonRpcResponse`/`JsonRpcError` and the response
//! writer), and the test compares the line it answers byte for byte.
//!
//! - Every method from `frontend/src/ipc/generated/sidecar-methods.json` (kept equal to the
//!   `methods!` table by `method_list_is_current`) with no `params`, with `{}` and with a string
//!   (`"x"`): the missing-field and wrong-type errors of every params shape, and the answers of
//!   the methods that take none. Checked in as `tests/wire/dispatch-responses.txt`.
//! - Answers on a temp repository, compared with the matching line of
//!   `tests/wire/dto-snapshots.txt`, which checks that the snapshot test's model of the
//!   encoding is what the binary writes.
//!
//! The sidecar always runs with `BROWSITORY_CONFIG_DIR` set to an empty temp dir, never the
//! user's config. Regenerate with
//! `UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar --test wire_dispatch`.

test_support::hermetic_git!();

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// The id of every request, so the lines don't depend on their order.
const ID: u64 = 7;

struct Sidecar {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    _config_dir: tempfile::TempDir,
}

impl Sidecar {
    fn spawn() -> Self {
        let config_dir = tempfile::TempDir::new().expect("create config dir");
        let mut child = Command::new(env!("CARGO_BIN_EXE_vscode-sidecar"))
            .env("BROWSITORY_CONFIG_DIR", config_dir.path())
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
            _config_dir: config_dir,
        }
    }

    /// Sends one request (without `params` when `None`) and returns the response line as
    /// written, without its line ending.
    fn call(&mut self, method: &str, params: Option<serde_json::Value>) -> String {
        let mut request = serde_json::json!({"jsonrpc": "2.0", "id": ID, "method": method});
        if let Some(params) = params {
            request["params"] = params;
        }
        writeln!(self.stdin, "{request}").expect("write request");
        self.stdin.flush().expect("flush request");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        assert!(line.ends_with('\n'), "{method}: no complete response line");
        line.trim_end_matches(['\r', '\n']).to_string()
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn methods() -> Vec<String> {
    let path = test_support::repo_root().join("frontend/src/ipc/generated/sidecar-methods.json");
    let text = std::fs::read_to_string(&path).expect("read sidecar-methods.json");
    serde_json::from_str(&text).expect("parse sidecar-methods.json")
}

/// Methods whose answer depends on the machine, not on the params: their lines are pinned as
/// `MACHINE_DEPENDENT_LINE` once the answer is checked to be a result (no error).
/// `list_font_families` lists the installed fonts (#858; its shape is checked in
/// `protocol_roundtrip.rs`).
const MACHINE_DEPENDENT: [&str; 1] = ["list_font_families"];
const MACHINE_DEPENDENT_LINE: &str = "<a result that depends on the machine>";

#[test]
fn every_method_answers_bad_params_with_the_pinned_line() {
    let mut sidecar = Sidecar::spawn();
    let mut cases = Vec::new();
    for method in methods() {
        for (shape, params) in [
            ("absent", None),
            ("empty", Some(serde_json::json!({}))),
            ("string", Some(serde_json::json!("x"))),
        ] {
            let mut line = sidecar.call(&method, params);
            if MACHINE_DEPENDENT.contains(&method.as_str()) {
                let answer: serde_json::Value = serde_json::from_str(&line).expect("json line");
                assert!(answer.get("result").is_some(), "{method}/{shape}: {line}");
                line = MACHINE_DEPENDENT_LINE.to_string();
            }
            cases.push((format!("{method}/{shape}"), line));
        }
    }
    let file = test_support::wire_snapshot_file(
        "# Generated: the sidecar's response line to every method with no params, `{}` and `\"x\"` (#306).\n\
         # Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar --test wire_dispatch\n",
        &cases,
    );
    test_support::assert_generated_file(
        &test_support::repo_root().join("crates/vscode-sidecar/tests/wire/dispatch-responses.txt"),
        &file,
    );
}

fn snapshot(case: &str) -> String {
    let path = test_support::repo_root().join("crates/vscode-sidecar/tests/wire/dto-snapshots.txt");
    test_support::read_wire_snapshot(&path)
        .into_iter()
        .find(|(name, _)| name == case)
        .unwrap_or_else(|| panic!("no snapshot {case}"))
        .1
}

fn ok_line(result: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":{ID},"result":{result}}}"#)
}

/// A repository with `a.txt` committed and `src/lib.rs` untracked: the status the
/// `StatusEntry/New` snapshot describes.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let repo = git2::Repository::init(dir.path()).expect("init repo");
    let mut config = repo.config().expect("repo config");
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    config.set_bool("core.autocrlf", false).unwrap();
    drop(config);
    std::fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = repo.signature().unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "Add", &tree, &[])
        .unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "x").unwrap();
    dir
}

#[test]
fn answers_on_a_repository_are_the_snapshot_bytes() {
    let dir = repo();
    let path = dir.path().to_str().expect("utf-8 temp path").to_string();
    let mut sidecar = Sidecar::spawn();
    assert_eq!(
        sidecar.call("open_repo", Some(serde_json::json!({ "path": path }))),
        ok_line("null")
    );

    assert_eq!(
        sidecar.call("get_status", Some(serde_json::json!({ "repoPath": path }))),
        ok_line(&format!("[{}]", snapshot("StatusEntry/New")))
    );

    // `read_file_version`'s `{ text }` has no DTO, so its bytes are pinned here.
    let version = |file: &str| serde_json::json!({ "repoPath": path, "path": file, "version": { "kind": "Head" } });
    assert_eq!(
        sidecar.call("read_file_version", Some(version("a.txt"))),
        ok_line(r#"{"text":"hello\n"}"#)
    );
    assert_eq!(
        sidecar.call("read_file_version", Some(version("missing.txt"))),
        ok_line(r#"{"text":null}"#)
    );
}
