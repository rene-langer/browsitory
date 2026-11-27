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

/// Starts the sidecar with `stage_file` held for `delay_ms` by the test-only delay hook, opens a
/// fresh repository with an untracked `a.txt` and asks to stage it (request 2, still running
/// when this returns).
fn sidecar_staging_slowly(
    delay_ms: u64,
    extra_env: &[(&str, &str)],
) -> (
    tempfile::TempDir,
    std::process::Child,
    std::process::ChildStdin,
    BufReader<std::process::ChildStdout>,
) {
    let dir = tempfile::tempdir().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
    let repo_path = dir.path().to_string_lossy().into_owned();

    let mut command = Command::new(env!("CARGO_BIN_EXE_vscode-sidecar"));
    command
        .env("BROWSITORY_SIDECAR_TEST_DELAY_METHOD", "stage_file")
        .env("BROWSITORY_SIDECAR_TEST_DELAY_MS", delay_ms.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    for (name, value) in extra_env {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("spawn vscode-sidecar");
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));

    let open = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "open_repo", "params": {"path": repo_path}
    });
    writeln!(stdin, "{open}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).expect("read open_repo reply");
    let opened: serde_json::Value = serde_json::from_str(&line).expect("open_repo reply");
    assert!(opened.get("error").is_none(), "open_repo failed: {opened}");

    let stage = serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "stage_file",
        "params": {"repoPath": repo_path, "path": "a.txt"}
    });
    writeln!(stdin, "{stage}").unwrap();
    stdin.flush().unwrap();
    (dir, child, stdin, stdout)
}

/// Waits up to `limit` for `child` to exit; kills it and returns `None` when it doesn't.
fn wait_with_limit(
    child: &mut std::process::Child,
    limit: std::time::Duration,
) -> Option<std::process::ExitStatus> {
    let deadline = std::time::Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// #1129: when the extension host goes away (a window reload or close), the sidecar's stdout
/// has no reader any more. Another request's answer then fails to be written; that must not end
/// the process while a write (here the held `stage_file`) still runs, which cut checkouts and
/// index writes off halfway.
#[test]
fn a_running_request_finishes_after_the_reader_of_stdout_is_gone() {
    let (dir, mut child, mut stdin, stdout) = sidecar_staging_slowly(1000, &[]);
    drop(stdout);
    // Answered at once, and its answer can't be written.
    let quick = serde_json::json!({
        "jsonrpc": "2.0", "id": 3, "method": "no_such_method", "params": {}
    });
    writeln!(stdin, "{quick}").unwrap();
    stdin.flush().unwrap();
    drop(stdin);

    let status = wait_with_limit(&mut child, std::time::Duration::from_secs(30));
    assert!(
        status.is_some(),
        "the sidecar didn't exit after stdin ended"
    );

    let repo = git2::Repository::open(dir.path()).unwrap();
    let index = repo.index().unwrap();
    assert!(
        index.get_path(std::path::Path::new("a.txt"), 0).is_some(),
        "the sidecar exited before the running stage_file staged a.txt"
    );
    assert!(!dir.path().join(".git").join("index.lock").exists());
}

/// #1129: the extension's kill after its grace period is a timer in the extension host, which
/// never fires when the host itself exits. So the sidecar bounds its wait for the running
/// requests after stdin ends itself, and exits once the grace period is over.
#[test]
fn the_sidecar_exits_after_its_grace_period_when_a_request_keeps_running() {
    let (_dir, mut child, stdin, _stdout) =
        sidecar_staging_slowly(60_000, &[("BROWSITORY_SIDECAR_TEST_EXIT_GRACE_MS", "200")]);
    drop(stdin);

    let status = wait_with_limit(&mut child, std::time::Duration::from_secs(10));
    assert!(
        status.is_some(),
        "the sidecar still ran 10 s after stdin ended, with a grace period of 0.2 s"
    );
}
