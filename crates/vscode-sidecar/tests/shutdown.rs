// Hide the developer's / CI runner's global and system git config from every test (#111).
test_support::hermetic_git!();

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// #961: the extension ends the sidecar's stdin when its panel closes. A request already running
/// (here a `stage_file`, held for a moment by the test-only delay hook) must still finish and be
/// answered before the sidecar exits: exiting at once cut libgit2 off in the middle of a write
/// and left `.git/index.lock` behind, failing every later git command in the repository.
#[test]
fn a_running_request_finishes_and_is_answered_after_stdin_ends() {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
    let repo_path = dir.path().to_string_lossy().into_owned();

    let mut child = Command::new(env!("CARGO_BIN_EXE_vscode-sidecar"))
        .env("BROWSITORY_SIDECAR_TEST_DELAY_METHOD", "stage_file")
        .env("BROWSITORY_SIDECAR_TEST_DELAY_MS", "500")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn vscode-sidecar");
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    let mut read_line = || {
        let mut line = String::new();
        stdout.read_line(&mut line).expect("read response");
        line
    };

    let open = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "open_repo", "params": {"path": repo_path}
    });
    writeln!(stdin, "{open}").unwrap();
    stdin.flush().unwrap();
    let opened: serde_json::Value = serde_json::from_str(&read_line()).expect("open_repo reply");
    assert!(opened.get("error").is_none(), "open_repo failed: {opened}");

    let stage = serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "stage_file",
        "params": {"repoPath": repo_path, "path": "a.txt"}
    });
    writeln!(stdin, "{stage}").unwrap();
    stdin.flush().unwrap();
    drop(stdin);

    let line = read_line();
    assert!(
        !line.is_empty(),
        "the sidecar exited without answering the running stage_file"
    );
    let staged: serde_json::Value = serde_json::from_str(&line).expect("stage_file reply");
    assert_eq!(staged["id"], 2);
    assert!(staged.get("error").is_none(), "stage_file failed: {staged}");
    assert!(child.wait().unwrap().success());

    let index = repo.index().unwrap();
    assert!(index.get_path(std::path::Path::new("a.txt"), 0).is_some());
    assert!(!dir.path().join(".git").join("index.lock").exists());
}
