//! #306: how the sidecar reads a request's `params`, end to end through the real binary.
//! `tests/wire/dispatch-responses.txt` pins every method's answer to no params, `{}` and `"x"`;
//! these tests pin the rest for a representative set of methods: a field of the wrong type, a
//! later required field missing, a bad enum value, extra fields and a positional array, and that
//! the methods that take no params still accept anything.
//!
//! The sidecar always runs with `BROWSITORY_CONFIG_DIR` set to an empty temp dir, never the
//! user's config.

test_support::hermetic_git!();

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

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

    fn call(&mut self, method: &str, params: Value) -> Value {
        let request = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        writeln!(self.stdin, "{request}").expect("write request");
        self.stdin.flush().expect("flush request");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("parse response")
    }

    /// The error message of a request that must fail.
    fn error(&mut self, method: &str, params: Value) -> String {
        let response = self.call(method, params);
        assert!(response.get("result").is_none(), "{method}: {response}");
        response["error"]["message"]
            .as_str()
            .unwrap_or_else(|| panic!("{method}: no error message in {response}"))
            .to_string()
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn a_field_of_the_wrong_type_names_the_expected_type() {
    let mut sidecar = Sidecar::spawn();
    for (method, params, message) in [
        (
            "get_status",
            json!({ "repoPath": 5 }),
            "invalid type: integer `5`, expected a string",
        ),
        (
            "open_repo",
            json!({ "path": true }),
            "invalid type: boolean `true`, expected a string",
        ),
        (
            "get_commit_graph",
            json!({ "repoPath": "/r", "limit": "ten" }),
            "invalid type: string \"ten\", expected usize",
        ),
        (
            "get_commit_graph",
            json!({ "repoPath": "/r", "limit": -1 }),
            "invalid value: integer `-1`, expected usize",
        ),
        (
            "delete_branch",
            json!({ "repoPath": "/r", "name": "b", "force": "yes" }),
            "invalid type: string \"yes\", expected a boolean",
        ),
        (
            "cherry_pick_many",
            json!({ "repoPath": "/r", "commitIds": "abc" }),
            "invalid type: string \"abc\", expected a sequence",
        ),
        // A setter can't clear a saved graph selection by sending `null`: only
        // `clear_graph_branch_selection`/`clear_graph_remote_branch_selection` do (#402).
        (
            "set_graph_branch_selection",
            json!({ "repoPath": "/r", "local": null }),
            "invalid type: null, expected a sequence",
        ),
        (
            "set_graph_remote_branch_selection",
            json!({ "repoPath": "/r", "remote": null }),
            "invalid type: null, expected a sequence",
        ),
        // A `#[serde(default)]` field may be absent, but not `null`.
        (
            "continue_sequence",
            json!({ "repoPath": "/r", "noVerify": null }),
            "invalid type: null, expected a boolean",
        ),
    ] {
        assert_eq!(sidecar.error(method, params), message, "{method}");
    }
}

#[test]
fn a_missing_later_or_nested_field_is_named() {
    let mut sidecar = Sidecar::spawn();
    for (method, params, message) in [
        (
            "get_commit_graph",
            json!({ "repoPath": "/r" }),
            "missing field `limit`",
        ),
        // The remote selection is required: an old caller can't silently drop it (#385).
        (
            "get_commit_graph",
            json!({ "repoPath": "/r", "limit": 1, "selection": { "local": null } }),
            "missing field `remote`",
        ),
        (
            "search_commits",
            json!({ "repoPath": "/r", "query": "q", "selection": { "local": null }, "cursor": null }),
            "missing field `remote`",
        ),
        (
            "get_commit_diff",
            json!({ "repoPath": "/r", "commitId": "c", "path": "a.txt" }),
            "missing field `opts`",
        ),
        (
            "get_commit_diff",
            json!({ "repoPath": "/r", "commitId": "c", "path": "a.txt", "opts": {} }),
            "missing field `ignoreWhitespace`",
        ),
        (
            "start_merge",
            json!({ "repoPath": "/r", "branchName": "b" }),
            "missing field `options`",
        ),
    ] {
        assert_eq!(sidecar.error(method, params), message, "{method}");
    }
}

#[test]
fn an_unknown_enum_value_lists_the_variants() {
    let mut sidecar = Sidecar::spawn();
    assert_eq!(
        sidecar.error(
            "reset_to_commit",
            json!({ "repoPath": "/r", "commitId": "c", "mode": "Medium" })
        ),
        "unknown variant `Medium`, expected one of `soft`, `mixed`, `hard`"
    );
    assert_eq!(
        sidecar.error(
            "read_file_version",
            json!({ "repoPath": "/r", "path": "a.txt", "version": { "kind": "Tip" } })
        ),
        "unknown variant `Tip`, expected one of `Head`, `Index`, `Stage`, `Commit`, `FirstParent`"
    );
}

/// Well-formed params reach the handler, which answers for the unopened repo.
#[test]
fn extra_fields_absent_options_and_positional_arrays_are_accepted() {
    let mut sidecar = Sidecar::spawn();
    for (method, params) in [
        (
            "get_status",
            json!({ "repoPath": "/nope", "extra": [1, 2] }),
        ),
        (
            "get_commit_graph",
            json!({ "repoPath": "/nope", "limit": 1, "selection": { "local": null, "remote": [] } }),
        ),
        ("get_status", json!(["/nope"])),
    ] {
        let message = sidecar.error(method, params.clone());
        assert_eq!(message, "repo not open: /nope", "{method} {params}");
    }
}

#[test]
fn methods_without_params_ignore_whatever_is_sent() {
    let mut sidecar = Sidecar::spawn();
    for params in [
        Value::Null,
        json!({ "repoPath": 5, "anything": [true] }),
        json!([1, 2, 3]),
        json!(42),
    ] {
        for (method, result) in [
            ("list_recent_repos", json!([])),
            ("list_workspaces", json!([])),
            (
                "get_diff_settings",
                json!({"contextLines": 3, "ignoreWhitespace": false, "syntaxHighlight": true, "wordWrap": "Auto", "algorithm": "Default"}),
            ),
        ] {
            let response = sidecar.call(method, params.clone());
            assert_eq!(response["result"], result, "{method} {params}: {response}");
            assert!(response.get("error").is_none(), "{method} {params}");
        }
    }
}
