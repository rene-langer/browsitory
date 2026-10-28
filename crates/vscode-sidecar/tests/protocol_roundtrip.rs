use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

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

impl Sidecar {
    /// Spawns the sidecar with extra environment variables set — used to arm the
    /// `BROWSITORY_SIDECAR_TEST_DELAY_METHOD`/`BROWSITORY_SIDECAR_TEST_DELAY_MS` test-only hook
    /// (see `dispatch.rs`'s `test_delay_before_dispatch`) that lets a test deterministically
    /// simulate one slow method without a real large repository.
    fn spawn_with_envs(envs: &[(&str, &str)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vscode-sidecar"))
            .envs(envs.iter().copied())
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

    /// Writes a request without reading its response, so the caller can send a second request
    /// before the first one's response has necessarily arrived.
    fn send(&mut self, id: u64, method: &str, params: serde_json::Value) {
        let request =
            serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{request}").expect("write request");
        self.stdin.flush().expect("flush request");
    }

    fn read_response(&mut self) -> serde_json::Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("parse response")
    }
}

#[test]
fn unknown_method_returns_a_json_rpc_error_not_a_hang() {
    let mut sidecar = Sidecar::spawn();

    let response = sidecar.call(1, "does_not_exist", serde_json::json!({}));

    assert_eq!(
        response["error"]["message"],
        "unknown method: does_not_exist"
    );
    assert!(response.get("result").is_none());
}

#[test]
fn malformed_request_is_dropped_without_killing_the_sidecar() {
    let mut sidecar = Sidecar::spawn();

    writeln!(sidecar.stdin, "not json").expect("write malformed line");
    sidecar.stdin.flush().expect("flush malformed line");

    let response = sidecar.call(1, "does_not_exist", serde_json::json!({}));
    assert_eq!(
        response["error"]["message"],
        "unknown method: does_not_exist"
    );
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
    config.set_bool("commit.gpgSign", false).unwrap();
    // Pin the hooks dir (git's default) so a global `core.hooksPath` never runs here.
    let hooks = repo.path().join("hooks");
    config
        .set_str("core.hooksPath", hooks.to_str().expect("utf-8 temp path"))
        .unwrap();
    drop(config);
    (dir, repo)
}

#[test]
fn open_status_close_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "untracked.txt", "hello");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);

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

    let status = sidecar.call(
        1,
        "get_status",
        serde_json::json!({"repoPath": "/no/such/repo"}),
    );

    assert!(status["error"]["message"]
        .as_str()
        .unwrap()
        .contains("repo not open"));
    assert!(status.get("result").is_none());
}

fn commit_all(repo: &git2::Repository, message: &str) {
    let mut index = repo.index().expect("open index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("stage all");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("write tree");
    let tree = repo.find_tree(tree_id).expect("find tree");
    let signature = repo.signature().expect("signature");
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )
    .expect("commit");
}

#[test]
fn commit_graph_reflects_a_commit_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "hello");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let graph = sidecar.call(
        2,
        "get_commit_graph",
        serde_json::json!({"repoPath": repo_path, "limit": 10, "selectedBranches": null}),
    );

    let commits = graph["result"]
        .as_array()
        .expect("commit graph result array");
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["summary"], "initial commit");
    // Exercise camelCase-only field names (`shortId`, `parentIds`) so a stray removal of
    // `#[serde(rename_all = "camelCase")]` on `GraphCommitDto` would actually fail a test.
    assert!(commits[0]["shortId"].is_string());
    assert!(commits[0]["parentIds"].is_array());
    assert!(commits[0]["remoteBranchRefs"].is_array());
}

#[test]
fn search_commits_and_cancel_search_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "alpha commit");
    write_file(dir.path(), "b.txt", "b");
    commit_all(&repo, "beta commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let page = sidecar.call(
        2,
        "search_commits",
        serde_json::json!({"repoPath": repo_path, "query": "beta", "refs": null, "cursor": null}),
    );
    let result = &page["result"];
    assert_eq!(result["commits"].as_array().unwrap().len(), 1);
    assert_eq!(result["commits"][0]["summary"], "beta commit");
    assert_eq!(result["nextCursor"], serde_json::Value::Null);
    assert_eq!(result["scanned"], 2);

    let invalid = sidecar.call(
        3,
        "search_commits",
        serde_json::json!({"repoPath": repo_path, "query": "feat: x", "refs": null, "cursor": null}),
    );
    assert!(invalid["error"]["message"]
        .as_str()
        .unwrap()
        .starts_with("Unknown search key"));

    let cancelled = sidecar.call(
        4,
        "cancel_search",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(cancelled["result"], serde_json::Value::Null);
}

#[test]
fn working_and_commit_diff_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let working = sidecar.call(
        2,
        "get_working_diff",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "staged": false, "opts": {"ignoreWhitespace": false, "contextLines": 3}}),
    );
    let hunks = working["result"]
        .as_array()
        .expect("working diff result array");
    assert_eq!(hunks.len(), 1);
    // Exercise camelCase-only field names (`oldStart`, `newLines`) so a stray removal of
    // `#[serde(rename_all = "camelCase")]` on `DiffHunkDto` would actually fail a test. The
    // first changed line ("line one" -> "line one changed") is a removal of the old line.
    assert!(hunks[0]["oldStart"].is_number());
    assert!(hunks[0]["newLines"].is_number());
    assert_eq!(hunks[0]["lines"][0]["origin"], "Remove");

    let graph = sidecar.call(
        3,
        "get_commit_graph",
        serde_json::json!({"repoPath": repo_path, "limit": 1, "selectedBranches": null}),
    );
    let head_id = graph["result"][0]["id"]
        .as_str()
        .expect("head commit id")
        .to_string();

    let commit_diff = sidecar.call(
        4,
        "get_commit_diff",
        serde_json::json!({"repoPath": repo_path, "commitId": head_id, "path": "tracked.txt", "opts": {"ignoreWhitespace": false, "contextLines": 3}}),
    );
    let commit_hunks = commit_diff["result"]
        .as_array()
        .expect("commit diff result array");
    assert_eq!(commit_hunks.len(), 1);
}

#[test]
fn compare_methods_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "first");
    let first = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    write_file(dir.path(), "a.txt", "two\n");
    commit_all(&repo, "second");
    let second = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    write_file(dir.path(), "a.txt", "three\n");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let opts = serde_json::json!({"ignoreWhitespace": false, "contextLines": 3});
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let files = sidecar.call(
        2,
        "get_range_files",
        serde_json::json!({"repoPath": repo_path, "fromId": first, "toId": second}),
    );
    assert_eq!(
        files["result"],
        serde_json::json!([{"path": "a.txt", "oldPath": null}])
    );

    let range_diff = sidecar.call(
        3,
        "get_range_diff",
        serde_json::json!({"repoPath": repo_path, "fromId": first, "toId": second, "path": "a.txt", "oldPath": null, "opts": opts}),
    );
    assert_eq!(range_diff["result"].as_array().unwrap().len(), 1);

    let wt_files = sidecar.call(
        4,
        "get_working_tree_compare_files",
        serde_json::json!({"repoPath": repo_path, "commitId": first}),
    );
    assert_eq!(wt_files["result"][0]["path"], "a.txt");

    let wt_diff = sidecar.call(
        5,
        "get_working_tree_compare_diff",
        serde_json::json!({"repoPath": repo_path, "commitId": first, "path": "a.txt", "oldPath": null, "opts": opts}),
    );
    assert_eq!(wt_diff["result"][0]["lines"][1]["content"], "three\n");

    let range = sidecar.call(
        6,
        "get_branch_compare_range",
        serde_json::json!({"repoPath": repo_path, "refName": "refs/heads/nope"}),
    );
    assert_eq!(range["error"]["message"], "No branch named \"nope\".");
}

struct ConfigDirGuard {
    _dir: tempfile::TempDir,
}

impl ConfigDirGuard {
    fn new() -> (Self, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().expect("create config dir");
        let path = dir.path().to_path_buf();
        (Self { _dir: dir }, path)
    }
}

impl Sidecar {
    fn spawn_with_config_dir(config_dir: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vscode-sidecar"))
            .env("BROWSITORY_CONFIG_DIR", config_dir)
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
}

#[test]
fn stash_lifecycle_round_trips_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let saved = sidecar.call(
        2,
        "save_stash",
        serde_json::json!({
            "repoPath": repo_path,
            "options": {"message": null, "keepIndex": false, "includeUntracked": true, "paths": []}
        }),
    );
    assert_eq!(saved["result"], serde_json::Value::Null);

    let stashes = sidecar.call(
        3,
        "list_stashes",
        serde_json::json!({"repoPath": repo_path}),
    );
    let list = stashes["result"].as_array().expect("stash list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["index"], 0);
    let stash_id = list[0]["commitId"].clone();

    let stale = sidecar.call(
        40,
        "apply_stash",
        serde_json::json!({"repoPath": repo_path, "index": 0, "commitId": "0000000000000000000000000000000000000000", "restoreIndex": true}),
    );
    assert_eq!(
        stale["error"]["message"], "The stash list changed; refresh and try again.",
        "{stale}"
    );

    let applied = sidecar.call(
        4,
        "apply_stash",
        serde_json::json!({"repoPath": repo_path, "index": 0, "commitId": stash_id, "restoreIndex": true}),
    );
    assert_eq!(applied["result"], serde_json::json!({"kind": "Applied"}));
    let on_disk = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(on_disk, "v2");

    let dropped = sidecar.call(
        5,
        "drop_stash",
        serde_json::json!({"repoPath": repo_path, "index": 0, "commitId": stash_id}),
    );
    assert_eq!(dropped["result"], serde_json::Value::Null);
    let stashes_after = sidecar.call(
        6,
        "list_stashes",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(stashes_after["result"], serde_json::json!([]));
}

#[test]
fn opening_a_repo_adds_it_to_recent_repos() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, _repo) = init_repo();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let recent = sidecar.call(2, "list_recent_repos", serde_json::json!({}));

    let paths = recent["result"].as_array().expect("recent repos array");
    assert!(paths.iter().any(|p| p == &repo_path));
}

#[test]
fn persist_and_list_open_repos_round_trip() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);

    let persist = sidecar.call(
        1,
        "persist_open_repos",
        serde_json::json!({
            "entries": [{"path": "/repos/a", "workspaceId": null}],
            "activePath": "/repos/a",
        }),
    );
    assert_eq!(persist["result"], serde_json::Value::Null);

    let listed = sidecar.call(2, "list_open_repos", serde_json::json!({}));
    assert_eq!(listed["result"]["entries"][0]["path"], "/repos/a");
    assert_eq!(
        listed["result"]["entries"][0]["workspaceId"],
        serde_json::Value::Null
    );
    assert_eq!(listed["result"]["activePath"], "/repos/a");
}

#[test]
fn scan_repos_in_root_finds_a_nested_repo() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let root = tempfile::TempDir::new().expect("create root dir");
    let (repo_dir, _repo) = init_repo();
    std::fs::rename(repo_dir.path(), root.path().join("nested")).expect("move repo into root");
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);

    let scanned = sidecar.call(
        1,
        "scan_repos_in_root",
        serde_json::json!({"root": root.path().to_str().unwrap()}),
    );

    let paths = scanned["result"].as_array().expect("scanned repos array");
    assert_eq!(paths.len(), 1);
}

#[test]
fn save_update_delete_workspace_round_trip() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);

    let saved = sidecar.call(
        1,
        "save_workspace",
        serde_json::json!({"name": "Suite", "root": "/repos/suite", "members": ["/repos/suite/api"]}),
    );
    let id = saved["result"].as_str().expect("workspace id").to_string();

    let listed = sidecar.call(2, "list_workspaces", serde_json::json!({}));
    assert_eq!(listed["result"][0]["id"], id);
    assert_eq!(listed["result"][0]["rootPath"], "/repos/suite");

    let updated = sidecar.call(
        3,
        "update_workspace",
        serde_json::json!({"id": id, "name": "Suite Renamed", "members": ["/repos/suite/web"]}),
    );
    assert_eq!(updated["result"], serde_json::Value::Null);
    let listed_after_update = sidecar.call(4, "list_workspaces", serde_json::json!({}));
    assert_eq!(listed_after_update["result"][0]["name"], "Suite Renamed");

    let deleted = sidecar.call(5, "delete_workspace", serde_json::json!({"id": id}));
    assert_eq!(deleted["result"], serde_json::Value::Null);
    let listed_after_delete = sidecar.call(6, "list_workspaces", serde_json::json!({}));
    assert_eq!(listed_after_delete["result"], serde_json::json!([]));
}

#[test]
fn graph_branch_selection_round_trips() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, _repo) = init_repo();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);

    let before = sidecar.call(
        1,
        "get_graph_branch_selection",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(before["result"], serde_json::Value::Null);

    let set = sidecar.call(
        2,
        "set_graph_branch_selection",
        serde_json::json!({"repoPath": repo_path, "selectedBranches": ["main", "feature"]}),
    );
    assert_eq!(set["result"], serde_json::Value::Null);

    let after = sidecar.call(
        3,
        "get_graph_branch_selection",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(after["result"], serde_json::json!(["main", "feature"]));
}

#[test]
fn graph_remote_branch_selection_round_trips() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, _repo) = init_repo();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);

    let before = sidecar.call(
        1,
        "get_graph_remote_branch_selection",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(before["result"], serde_json::Value::Null);

    let set = sidecar.call(
        2,
        "set_graph_remote_branch_selection",
        serde_json::json!({"repoPath": repo_path, "selectedBranches": ["origin/main"]}),
    );
    assert_eq!(set["result"], serde_json::Value::Null);

    let after = sidecar.call(
        3,
        "get_graph_remote_branch_selection",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(after["result"], serde_json::json!(["origin/main"]));
}

#[test]
fn stage_commit_and_get_commit_files_round_trip_through_the_sidecar() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "new.txt", "hello");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let staged = sidecar.call(
        2,
        "stage_file",
        serde_json::json!({"repoPath": repo_path, "path": "new.txt"}),
    );
    assert_eq!(staged["result"], serde_json::Value::Null);

    let committed = sidecar.call(
        3,
        "commit",
        serde_json::json!({"repoPath": repo_path, "message": "add new.txt"}),
    );
    assert_eq!(committed["result"]["kind"], "Committed");
    let commit_id = committed["result"]["commitId"]
        .as_str()
        .expect("commit id")
        .to_string();
    assert_eq!(commit_id.len(), 40);

    let files = sidecar.call(
        4,
        "get_commit_files",
        serde_json::json!({"repoPath": repo_path, "commitId": commit_id}),
    );
    assert_eq!(files["result"], serde_json::json!(["new.txt"]));

    let message = sidecar.call(
        6,
        "get_commit_message",
        serde_json::json!({"repoPath": repo_path, "commitId": commit_id}),
    );
    assert_eq!(message["result"], serde_json::json!("add new.txt"));

    let signature = sidecar.call(
        7,
        "get_commit_signature",
        serde_json::json!({"repoPath": repo_path, "commitId": commit_id}),
    );
    assert_eq!(
        signature["result"],
        serde_json::json!({
            "status": "unsigned", "format": null, "signer": null, "key": null, "detail": null
        })
    );

    let unstaged = sidecar.call(
        5,
        "unstage_file",
        serde_json::json!({"repoPath": repo_path, "path": "new.txt"}),
    );
    assert_eq!(unstaged["result"], serde_json::Value::Null);
}

#[test]
fn stage_unstage_and_discard_hunk_round_trip_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let diff = sidecar.call(
        2,
        "get_working_diff",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "staged": false, "opts": {"ignoreWhitespace": false, "contextLines": 3}}),
    );
    let hunk = &diff["result"][0];
    let old_start = hunk["oldStart"].as_u64().unwrap();
    let new_start = hunk["newStart"].as_u64().unwrap();

    let staged = sidecar.call(
        3,
        "stage_hunk",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "oldStart": old_start, "newStart": new_start, "contextLines": 3}),
    );
    assert_eq!(staged["result"], serde_json::Value::Null);

    let status = sidecar.call(4, "get_status", serde_json::json!({"repoPath": repo_path}));
    assert!(status["result"][0]["staged"].as_bool().unwrap());

    let unstaged = sidecar.call(
        5,
        "unstage_hunk",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "oldStart": old_start, "newStart": new_start, "contextLines": 3}),
    );
    assert_eq!(unstaged["result"], serde_json::Value::Null);

    let discarded = sidecar.call(
        6,
        "discard_hunk",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "oldStart": old_start, "newStart": new_start, "contextLines": 3}),
    );
    assert_eq!(discarded["result"], serde_json::Value::Null);
    let on_disk = std::fs::read_to_string(dir.path().join("tracked.txt")).unwrap();
    assert_eq!(on_disk.replace("\r\n", "\n"), "line one\nline two\n");
}

#[test]
fn branch_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let created = sidecar.call(
        2,
        "create_branch",
        serde_json::json!({"repoPath": repo_path, "name": "feature", "startPoint": "HEAD"}),
    );
    assert_eq!(created["result"], serde_json::Value::Null);

    let branches = sidecar.call(
        3,
        "list_branches",
        serde_json::json!({"repoPath": repo_path}),
    );
    let list = branches["result"].as_array().expect("branch list");
    assert_eq!(list.len(), 2);
    let feature = list
        .iter()
        .find(|b| b["name"] == "feature")
        .expect("feature branch");
    assert_eq!(feature["isCurrent"], true);

    let initial_name = list
        .iter()
        .find(|b| b["name"] != "feature")
        .expect("initial branch")["name"]
        .as_str()
        .unwrap()
        .to_string();
    sidecar.call(
        4,
        "switch_branch",
        serde_json::json!({"repoPath": repo_path, "name": initial_name}),
    );

    let renamed = sidecar.call(
        5,
        "rename_branch",
        serde_json::json!({"repoPath": repo_path, "oldName": "feature", "newName": "renamed"}),
    );
    assert_eq!(renamed["result"], serde_json::Value::Null);

    let deleted = sidecar.call(
        6,
        "delete_branch",
        serde_json::json!({"repoPath": repo_path, "name": "renamed", "force": true}),
    );
    assert_eq!(deleted["result"], serde_json::Value::Null);

    let final_branches = sidecar.call(
        7,
        "list_branches",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(final_branches["result"].as_array().unwrap().len(), 1);
}

#[test]
fn worktree_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let linked = dir.path().join("feature-tree");
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let created = sidecar.call(
        2,
        "create_worktree",
        serde_json::json!({
            "repoPath": repo_path,
            "name": "feature-tree",
            "path": linked.to_str().unwrap(),
            "branch": "feature",
            "startPoint": "HEAD",
        }),
    );
    assert_eq!(created["result"], serde_json::Value::Null);

    let listed = sidecar.call(
        3,
        "list_worktrees",
        serde_json::json!({"repoPath": repo_path}),
    );
    let worktrees = listed["result"].as_array().expect("worktree list");
    assert!(worktrees
        .iter()
        .any(|w| w["name"] == "feature-tree" && w["isMain"] == false));

    let removed = sidecar.call(
        4,
        "remove_worktree",
        serde_json::json!({"repoPath": repo_path, "name": "feature-tree"}),
    );
    assert_eq!(removed["result"], serde_json::Value::Null);
    assert!(!linked.exists());

    let pruned = sidecar.call(
        5,
        "prune_worktrees",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(pruned["result"], serde_json::Value::Null);
}

#[test]
fn submodule_lifecycle_round_trips_through_the_sidecar() {
    let parent_dir = tempfile::TempDir::new().expect("create parent dir");
    let child_dir = tempfile::TempDir::new().expect("create child dir");
    let child = git2::Repository::init(child_dir.path()).expect("init child repo");
    {
        let mut config = child.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    write_file(child_dir.path(), "child.txt", "hello");
    commit_all(&child, "child commit");

    let parent = git2::Repository::init(parent_dir.path()).expect("init parent repo");
    {
        let mut config = parent.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    let mut submodule = parent
        .submodule(
            child_dir.path().to_str().unwrap(),
            std::path::Path::new("deps/child"),
            true,
        )
        .expect("configure submodule");
    submodule.clone(None).expect("clone submodule");
    submodule.add_to_index(true).expect("stage submodule");
    submodule.add_finalize().expect("finalize submodule");
    drop(submodule);
    commit_all(&parent, "add submodule");
    drop(parent);

    let checkout_dir = tempfile::TempDir::new().expect("create checkout dir");
    git2::Repository::clone(parent_dir.path().to_str().unwrap(), checkout_dir.path())
        .expect("clone parent");
    let repo_path = checkout_dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let before = sidecar.call(
        2,
        "list_submodules",
        serde_json::json!({"repoPath": repo_path}),
    );
    let list = before["result"].as_array().expect("submodule list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["path"], "deps/child");
    assert_eq!(list[0]["initialized"], false);

    let inited = sidecar.call(
        3,
        "init_submodule",
        serde_json::json!({"repoPath": repo_path, "path": "deps/child"}),
    );
    assert_eq!(inited["result"], serde_json::Value::Null);

    let updated = sidecar.call(
        4,
        "update_submodule",
        serde_json::json!({"repoPath": repo_path, "path": "deps/child", "recursive": false}),
    );
    assert_eq!(updated["result"], serde_json::Value::Null);

    let after = sidecar.call(
        5,
        "list_submodules",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(after["result"][0]["initialized"], true);
}

#[test]
fn reflog_lists_and_restores_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first commit");
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "second commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let refs = sidecar.call(
        2,
        "list_reflog_refs",
        serde_json::json!({"repoPath": repo_path}),
    );
    let refs_list = refs["result"].as_array().expect("reflog refs");
    assert!(refs_list.iter().any(|r| r == "HEAD"));

    let entries = sidecar.call(
        3,
        "get_reflog",
        serde_json::json!({"repoPath": repo_path, "reference": "HEAD"}),
    );
    let entries_list = entries["result"].as_array().expect("reflog entries");
    assert_eq!(entries_list[0]["summary"], "second commit");
    let first_commit_id = entries_list[1]["newId"].as_str().unwrap().to_string();

    let restored = sidecar.call(
        4,
        "restore_reflog_entry",
        serde_json::json!({"repoPath": repo_path, "reference": "HEAD", "newId": first_commit_id}),
    );
    assert_eq!(restored["result"], serde_json::Value::Null);
}

#[test]
fn remote_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    repo.remote("origin", "https://example.com/owner/repo.git")
        .unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let remotes = sidecar.call(
        2,
        "list_remotes",
        serde_json::json!({"repoPath": repo_path}),
    );
    let list = remotes["result"].as_array().expect("remote list");
    assert_eq!(list[0]["name"], "origin");
    assert_eq!(list[0]["fetchUrl"], "https://example.com/owner/repo.git");
    assert_eq!(list[0]["authMode"], serde_json::Value::Null);

    let renamed = sidecar.call(
        3,
        "rename_remote",
        serde_json::json!({"repoPath": repo_path, "oldName": "origin", "newName": "upstream"}),
    );
    assert_eq!(renamed["result"], serde_json::Value::Null);

    let updated = sidecar.call(
        4,
        "update_remote_urls",
        serde_json::json!({"repoPath": repo_path, "name": "upstream", "fetchUrl": "https://example.com/owner/repo2.git", "pushUrl": null}),
    );
    assert_eq!(updated["result"], serde_json::Value::Null);

    let auth_set = sidecar.call(
        5,
        "set_remote_auth_mode",
        serde_json::json!({"repoPath": repo_path, "remoteName": "upstream", "mode": "HttpsToken", "username": "alice"}),
    );
    assert_eq!(auth_set["result"], serde_json::Value::Null);

    let remotes_after = sidecar.call(
        6,
        "list_remotes",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(remotes_after["result"][0]["authMode"], "HttpsToken");
    assert_eq!(remotes_after["result"][0]["authUsername"], "alice");

    let removed = sidecar.call(
        7,
        "remove_remote",
        serde_json::json!({"repoPath": repo_path, "name": "upstream", "clearUpstreams": true}),
    );
    assert_eq!(removed["result"], serde_json::Value::Null);
    let remotes_final = sidecar.call(
        8,
        "list_remotes",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(remotes_final["result"], serde_json::json!([]));
}

#[test]
fn add_remote_https_credential_and_current_upstream_round_trip_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let added = sidecar.call(
        2,
        "add_remote",
        serde_json::json!({"repoPath": repo_path, "name": "origin", "fetchUrl": "https://example.com/owner/repo.git", "pushUrl": null}),
    );
    assert_eq!(added["result"], serde_json::Value::Null);

    let saved = sidecar.call(
        3,
        "save_https_credential",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin", "username": "alice", "token": "secret"}),
    );
    assert_eq!(saved["result"], serde_json::Value::Null);

    let forgotten = sidecar.call(
        4,
        "forget_https_credential",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin"}),
    );
    assert_eq!(forgotten["result"], serde_json::Value::Null);

    let no_upstream = sidecar.call(
        5,
        "get_current_upstream",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(no_upstream["result"], serde_json::Value::Null);

    let set = sidecar.call(
        6,
        "set_current_upstream",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin", "remoteBranch": "main"}),
    );
    assert_eq!(set["result"], serde_json::Value::Null);

    let upstream = sidecar.call(
        7,
        "get_current_upstream",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(upstream["result"]["remoteName"], "origin");
    assert_eq!(upstream["result"]["remoteBranch"], "main");

    let upstreams = sidecar.call(
        8,
        "get_remote_upstreams",
        serde_json::json!({"repoPath": repo_path, "name": "origin"}),
    );
    assert_eq!(upstreams["result"].as_array().unwrap().len(), 1);

    let cleared = sidecar.call(
        9,
        "clear_current_upstream",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(cleared["result"], serde_json::Value::Null);
    let after_clear = sidecar.call(
        10,
        "get_current_upstream",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(after_clear["result"], serde_json::Value::Null);

    let branches = sidecar.call(
        11,
        "list_remote_branches",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin"}),
    );
    assert_eq!(branches["result"], serde_json::json!([]));
}

#[test]
fn tag_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let created = sidecar.call(
        2,
        "create_tag",
        serde_json::json!({"repoPath": repo_path, "name": "v1.0.0", "message": "first release"}),
    );
    assert_eq!(created["result"], serde_json::Value::Null);

    let tags = sidecar.call(3, "list_tags", serde_json::json!({"repoPath": repo_path}));
    let list = tags["result"].as_array().expect("tag list");
    assert_eq!(list[0]["name"], "v1.0.0");
    assert_eq!(list[0]["annotated"], true);
    assert_eq!(list[0]["message"], "first release");

    let deleted = sidecar.call(
        4,
        "delete_tag",
        serde_json::json!({"repoPath": repo_path, "name": "v1.0.0"}),
    );
    assert_eq!(deleted["result"], serde_json::Value::Null);
    let tags_after = sidecar.call(5, "list_tags", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(tags_after["result"], serde_json::json!([]));
}

struct RemoteFixture {
    source_dir: tempfile::TempDir,
    _remote_dir: tempfile::TempDir,
    local_dir: tempfile::TempDir,
    source: git2::Repository,
}

impl RemoteFixture {
    /// Adds a commit to the source clone and pushes it to the bare remote, so the local clone
    /// has something to fetch/fast-forward to.
    fn remote_commit(&self, message: &str) {
        write_file(self.source_dir.path(), "remote.txt", message);
        commit_all(&self.source, message);
        self.source
            .find_remote("origin")
            .expect("find source remote")
            .push(&["refs/heads/main:refs/heads/main".to_string()], None)
            .expect("push source commit");
    }

    fn local_path(&self) -> String {
        self.local_dir.path().to_str().unwrap().to_string()
    }
}

/// A source repo, a bare remote it has pushed `main` to, and a fresh clone of that remote as the
/// "local" repo the sidecar operates on. `main` is forced explicitly rather than inherited from
/// the machine's `init.defaultBranch`, so the fixture is deterministic across git installations —
/// the same approach `crates/git-core/tests/remote.rs`'s own `local_and_bare_remote` takes.
fn local_and_bare_remote() -> RemoteFixture {
    let (source_dir, source) = init_repo();
    source.set_head("refs/heads/main").expect("set source head");
    write_file(source_dir.path(), "README.md", "initial commit\n");
    commit_all(&source, "initial commit");

    let remote_dir = tempfile::TempDir::new().expect("create bare remote dir");
    let remote = git2::Repository::init_bare(remote_dir.path()).expect("init bare remote");
    source
        .remote("origin", remote_dir.path().to_str().unwrap())
        .expect("add source remote");
    source
        .find_remote("origin")
        .expect("find source remote")
        .push(&["refs/heads/main:refs/heads/main".to_string()], None)
        .expect("push source commit");
    remote.set_head("refs/heads/main").expect("set remote head");
    drop(remote);

    let local_dir = tempfile::TempDir::new().expect("create local dir");
    let local = git2::Repository::clone(remote_dir.path().to_str().unwrap(), local_dir.path())
        .expect("clone local repo");
    {
        let mut config = local.config().expect("local config");
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    drop(local);

    RemoteFixture {
        source_dir,
        _remote_dir: remote_dir,
        local_dir,
        source,
    }
}

fn is_terminal_notification(line: &serde_json::Value) -> bool {
    matches!(
        line["params"]["phase"].as_str(),
        Some("Completed") | Some("Failed")
    )
}

impl Sidecar {
    fn read_line(&mut self) -> serde_json::Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read line");
        // A complete, self-contained JSON object per line is the whole framing contract: if a
        // notification written by the relay thread ever interleaved with a response written by
        // the dispatch loop, this parse would fail.
        serde_json::from_str(&line).unwrap_or_else(|error| panic!("parse line {line:?}: {error}"))
    }

    /// Like `call`, but also collects every JSON-RPC *notification* (no `id`) that arrives around
    /// the response, and — when `wait_for_terminal_notification` is set — keeps reading past the
    /// response until a `transferProgress` notification with a terminal phase
    /// (`Completed`/`Failed`) shows up. `fetch`/`push` reply with the operation id before the
    /// transfer finishes, so their terminal notification can legitimately arrive either side of
    /// the response line; this helper tolerates both orderings.
    fn call_and_collect_notifications(
        &mut self,
        id: u64,
        method: &str,
        params: serde_json::Value,
        wait_for_terminal_notification: bool,
    ) -> (serde_json::Value, Vec<serde_json::Value>) {
        let request =
            serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{request}").expect("write request");
        self.stdin.flush().expect("flush request");

        let mut notifications = Vec::new();
        let mut response = None;
        loop {
            let line = self.read_line();
            if line.get("id").is_some() {
                response = Some(line);
            } else {
                notifications.push(line);
            }
            if response.is_some()
                && (!wait_for_terminal_notification
                    || notifications.iter().any(is_terminal_notification))
            {
                break;
            }
        }
        (response.expect("response line"), notifications)
    }
}

#[test]
fn fetch_remote_streams_transfer_progress_notifications_through_the_sidecar() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit("remote change");
    let repo_path = fixture.local_path();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let (response, notifications) = sidecar.call_and_collect_notifications(
        2,
        "fetch_remote",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin"}),
        true,
    );

    let operation_id = response["result"]
        .as_str()
        .expect("operation id")
        .to_string();
    assert!(operation_id.starts_with("fetch-"));
    assert!(!notifications.is_empty());
    assert!(notifications.iter().all(|n| n["jsonrpc"] == "2.0"
        && n["method"] == "transferProgress"
        && n.get("id").is_none()
        && n["params"]["operationId"] == operation_id
        && n["params"]["operation"] == "Fetch"));
    assert!(notifications
        .iter()
        .any(|n| n["params"]["phase"] == "Starting"));
    assert!(notifications
        .iter()
        .any(|n| n["params"]["phase"] == "Completed" && n["params"]["errorKind"].is_null()));
    // Exercise camelCase-only field names so a stray removal of `#[serde(rename_all =
    // "camelCase")]` on `TransferProgressDto` would actually fail a test.
    assert!(notifications
        .iter()
        .all(|n| n["params"]["receivedBytes"].is_number()));
    // Sideband/message text is never forwarded, mirroring `crates/tauri-app/src/commands/mod.rs`'s
    // own redaction — see its `transfer_event_bridge_redacts_sideband_and_failure_messages` test.
    assert!(notifications
        .iter()
        .all(|n| n["params"]["message"].is_null()));

    // The relay for the finished operation has released stdout for good: the next request's
    // response is the very next line on the wire, with no trailing notification wedged in front
    // of it. A relay that outlived its operation would surface here.
    let status = sidecar.call(3, "get_status", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(status["id"], 3);
}

#[test]
fn push_current_branch_and_push_tags_stream_notifications_through_the_sidecar() {
    let (local_dir, repo) = init_repo();
    write_file(local_dir.path(), "README.md", "initial commit\n");
    commit_all(&repo, "initial commit");
    let head = repo
        .head()
        .unwrap()
        .peel(git2::ObjectType::Commit)
        .expect("peel head");
    repo.tag_lightweight("v1.0.0", &head, false)
        .expect("create tag");
    let remote_dir = tempfile::TempDir::new().expect("create bare remote dir");
    git2::Repository::init_bare(remote_dir.path()).expect("init bare remote");
    repo.remote("origin", remote_dir.path().to_str().unwrap())
        .expect("add origin");
    drop(head);
    drop(repo);
    let repo_path = local_dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let (push_response, push_notifications) = sidecar.call_and_collect_notifications(
        2,
        "push_current_branch",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin"}),
        true,
    );
    let push_id = push_response["result"].as_str().expect("operation id");
    assert!(push_id.starts_with("push-"));
    assert!(push_notifications
        .iter()
        .all(|n| n["method"] == "transferProgress" && n["params"]["operationId"] == push_id));
    assert!(push_notifications
        .iter()
        .any(|n| n["params"]["phase"] == "Completed" && n["params"]["operation"] == "PushBranch"));

    // A second transfer on the same sidecar process gets its own relay and its own operation id;
    // no notification from the first operation may still be arriving.
    let (tags_response, tags_notifications) = sidecar.call_and_collect_notifications(
        3,
        "push_tags",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin", "names": ["v1.0.0"]}),
        true,
    );
    let tags_id = tags_response["result"].as_str().expect("operation id");
    assert!(tags_id.starts_with("push-"));
    assert_ne!(tags_id, push_id);
    assert!(tags_notifications
        .iter()
        .all(|n| n["params"]["operationId"] == tags_id));
    assert!(tags_notifications
        .iter()
        .any(|n| n["params"]["phase"] == "Completed" && n["params"]["operation"] == "PushTags"));
}

#[test]
fn fetch_all_push_branch_and_delete_remote_branch_stream_notifications_through_the_sidecar() {
    let (local_dir, repo) = init_repo();
    write_file(local_dir.path(), "README.md", "initial commit\n");
    commit_all(&repo, "initial commit");
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let remote_dir = tempfile::TempDir::new().expect("create bare remote dir");
    git2::Repository::init_bare(remote_dir.path()).expect("init bare remote");
    repo.remote("origin", remote_dir.path().to_str().unwrap())
        .expect("add origin");
    drop(repo);
    let repo_path = local_dir.path().to_str().unwrap().to_string();
    let bare_has = |name: &str| {
        git2::Repository::open_bare(remote_dir.path())
            .unwrap()
            .find_reference(&format!("refs/heads/{name}"))
            .is_ok()
    };
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let (fetch_all, notes) = sidecar.call_and_collect_notifications(
        2,
        "fetch_all_remotes",
        serde_json::json!({"repoPath": repo_path, "prune": null}),
        true,
    );
    let fetch_id = fetch_all["result"].as_str().expect("operation id");
    assert!(fetch_id.starts_with("fetch-all-"));
    let last = notes.last().expect("terminal notification");
    assert_eq!(last["params"]["operation"], "FetchAll");
    assert_eq!(last["params"]["phase"], "Completed");

    let (push, notes) = sidecar.call_and_collect_notifications(
        3,
        "push_branch",
        serde_json::json!({
            "repoPath": repo_path, "remoteName": "origin", "localBranch": branch,
            "remoteBranch": "sidecar-published", "setUpstream": false
        }),
        true,
    );
    assert!(push["result"]
        .as_str()
        .expect("operation id")
        .starts_with("push-"));
    let last = notes.last().expect("terminal notification");
    assert_eq!(last["params"]["operation"], "PushBranch");
    assert_eq!(last["params"]["phase"], "Completed");
    assert!(bare_has("sidecar-published"));

    let (delete, notes) = sidecar.call_and_collect_notifications(
        4,
        "delete_remote_branch",
        serde_json::json!({
            "repoPath": repo_path, "remoteName": "origin", "branchName": "sidecar-published"
        }),
        true,
    );
    assert!(delete["result"]
        .as_str()
        .expect("operation id")
        .starts_with("delete-remote-"));
    let last = notes.last().expect("terminal notification");
    assert_eq!(last["params"]["operation"], "DeleteRemoteBranch");
    assert_eq!(last["params"]["phase"], "Completed");
    assert!(!bare_has("sidecar-published"));
}

#[test]
fn pull_current_upstream_streams_notifications_and_returns_an_outcome() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit("remote change");
    let repo_path = fixture.local_path();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));
    let upstream = sidecar.call(
        2,
        "set_current_upstream",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin", "remoteBranch": "main"}),
    );
    assert_eq!(upstream["result"], serde_json::Value::Null);

    // `pull_current_upstream` blocks until the whole pull finishes, so every notification up to
    // (but not necessarily including) the terminal one precedes the response line.
    let (response, notifications) = sidecar.call_and_collect_notifications(
        3,
        "pull_current_upstream",
        serde_json::json!({"repoPath": repo_path}),
        false,
    );

    assert_eq!(response["result"]["kind"], "FastForwarded");
    assert!(response["result"]["upstreamRef"]
        .as_str()
        .expect("upstream ref")
        .starts_with("refs/remotes/origin/"));
    assert!(notifications
        .iter()
        .any(|n| n["params"]["phase"] == "Starting" && n["params"]["operation"] == "Pull"));
}

#[test]
fn transfer_progress_notifications_carry_no_id_and_are_rejected_as_requests() {
    // Guards the wire distinction the TypeScript client relies on: a notification has a `method`
    // and no `id`, so it can never be mistaken for a response to a pending request.
    let fixture = local_and_bare_remote();
    let repo_path = fixture.local_path();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let (_response, notifications) = sidecar.call_and_collect_notifications(
        2,
        "fetch_remote",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin"}),
        true,
    );

    assert!(notifications.iter().all(|n| n.get("id").is_none()));
    assert!(notifications.iter().all(|n| n.get("result").is_none()));
    assert!(notifications.iter().all(|n| n.get("error").is_none()));
}

#[test]
fn cancel_transfer_round_trips_through_the_sidecar_without_its_own_notifications() {
    // `cancel_transfer` writes into the worker's shared cancel registry and returns immediately;
    // the cancellation itself surfaces on the relay the cancelled operation already owns, so
    // this call must answer with a plain `null` result and emit nothing of its own. An unknown
    // operation id is a deliberate no-op so the extension can fire this without first racing the
    // transfer to check whether it is still running.
    let fixture = local_and_bare_remote();
    let repo_path = fixture.local_path();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let response = sidecar.call(
        2,
        "cancel_transfer",
        serde_json::json!({"repoPath": repo_path, "operationId": "fetch-1"}),
    );

    assert_eq!(response["id"], 2);
    assert!(response["result"].is_null());
    assert!(response.get("error").is_none());
    // The very next response is the next line on the wire — no notification wedged in front.
    let status = sidecar.call(3, "get_status", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(status["id"], 3);
}

#[test]
fn cancel_transfer_on_an_unopened_repo_returns_an_error() {
    let mut sidecar = Sidecar::spawn();

    let response = sidecar.call(
        1,
        "cancel_transfer",
        serde_json::json!({"repoPath": "/no/such/repo", "operationId": "fetch-1"}),
    );

    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("repo not open"));
}

#[test]
fn fetch_remote_on_an_unopened_repo_returns_an_error_without_emitting_notifications() {
    let mut sidecar = Sidecar::spawn();

    let response = sidecar.call(
        1,
        "fetch_remote",
        serde_json::json!({"repoPath": "/no/such/repo", "remoteName": "origin"}),
    );

    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("repo not open"));
    // The relay spawned for the failed call must have shut down when its sender was dropped, so
    // the next response is the very next line.
    let next = sidecar.call(2, "does_not_exist", serde_json::json!({}));
    assert_eq!(next["id"], 2);
}

#[test]
fn get_blame_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "hello\n");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let blame = sidecar.call(
        2,
        "get_blame",
        serde_json::json!({"repoPath": repo_path, "commitId": "HEAD", "path": "file.txt"}),
    );
    let lines = blame["result"].as_array().expect("blame lines");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["content"], "hello");
}

#[test]
fn merge_conflict_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "base\n");
    commit_all(&repo, "base commit");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &base, false).unwrap();
    drop(base);

    write_file(dir.path(), "file.txt", "main change\n");
    commit_all(&repo, "main change");

    let feature_ref = repo
        .find_branch("feature", git2::BranchType::Local)
        .unwrap();
    repo.set_head(feature_ref.get().name().unwrap()).unwrap();
    drop(feature_ref);
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "file.txt", "feature change\n");
    commit_all(&repo, "feature change");

    let main_branch = repo
        .branches(Some(git2::BranchType::Local))
        .unwrap()
        .find_map(|b| {
            let (branch, _) = b.unwrap();
            let name = branch.name().unwrap().unwrap().to_string();
            (name != "feature").then_some(name)
        })
        .unwrap();
    repo.set_head(&format!("refs/heads/{main_branch}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    drop(repo);

    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let merged = sidecar.call(
        2,
        "start_merge",
        serde_json::json!({
            "repoPath": repo_path,
            "branchName": "feature",
            "options": {"fastForward": "Allow", "squash": false}
        }),
    );
    assert_eq!(merged["result"]["kind"], "Conflicted");
    let conflicted_files = merged["result"]["files"].as_array().unwrap();
    assert!(conflicted_files.iter().any(|f| f == "file.txt"));

    let hunks = sidecar.call(
        3,
        "get_conflict_hunks",
        serde_json::json!({"repoPath": repo_path, "path": "file.txt"}),
    );
    assert!(hunks["result"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["kind"] == "Conflict"));

    let message = sidecar.call(
        4,
        "get_merge_message",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert!(message["result"].as_str().unwrap().contains("feature"));

    let resolved = sidecar.call(
        5,
        "resolve_conflict",
        serde_json::json!({"repoPath": repo_path, "path": "file.txt", "resolvedContent": "resolved\n"}),
    );
    assert_eq!(resolved["result"], serde_json::Value::Null);

    let aborted = sidecar.call(6, "abort_merge", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(aborted["result"], serde_json::Value::Null);
}

#[test]
fn rebase_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first commit");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &base, false).unwrap();
    drop(base);
    let feature_ref = repo
        .find_branch("feature", git2::BranchType::Local)
        .unwrap();
    repo.set_head(feature_ref.get().name().unwrap()).unwrap();
    drop(feature_ref);
    write_file(dir.path(), "other.txt", "v1");
    commit_all(&repo, "second commit");
    let second = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    drop(repo);

    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let since = sidecar.call(
        2,
        "commits_since",
        serde_json::json!({"repoPath": repo_path, "onto": "HEAD~1"}),
    );
    let commits = since["result"].as_array().expect("commits since");
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["id"], second);

    let started = sidecar.call(
        3,
        "start_rebase",
        serde_json::json!({
            "repoPath": repo_path,
            "onto": "HEAD~1",
            "plan": [{"commitId": second, "action": {"kind": "Pick"}, "combinedMessage": null}],
        }),
    );
    assert_eq!(started["result"]["kind"], "Done");

    let progress = sidecar.call(
        4,
        "get_rebase_progress",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(progress["result"], serde_json::Value::Null);
}

#[test]
fn detect_forge_repository_and_forge_token_lifecycle_round_trip_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    repo.remote("origin", "https://github.com/acme/widget.git")
        .unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let detected = sidecar.call(
        2,
        "detect_forge_repository",
        serde_json::json!({"repoPath": repo_path}),
    );
    let repos = detected["result"].as_array().expect("forge repositories");
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0]["provider"], "GitHub");
    assert_eq!(repos[0]["owner"], "acme");
    assert_eq!(repos[0]["name"], "widget");

    let saved = sidecar.call(
        3,
        "save_forge_token",
        serde_json::json!({"repoPath": repo_path, "provider": "GitHub", "account": "alice", "token": "secret"}),
    );
    assert_eq!(saved["result"], serde_json::Value::Null);

    let forgotten = sidecar.call(
        4,
        "forget_forge_token",
        serde_json::json!({"repoPath": repo_path, "provider": "GitHub", "account": "alice"}),
    );
    assert_eq!(forgotten["result"], serde_json::Value::Null);
}

#[test]
fn list_pull_requests_on_a_non_forge_remote_fails_before_any_http_call() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    repo.remote("origin", "https://example.com/not-a-forge/repo.git")
        .unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let listed = sidecar.call(
        2,
        "list_pull_requests",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin", "account": "alice"}),
    );

    assert!(listed.get("result").is_none());
    assert!(listed["error"]["message"].as_str().is_some());
}

/// AUD-2026-09-05-VSCE-003 regression test: two repos are open in the same sidecar process. A
/// slow request against repo A must not delay a concurrent, unrelated fast request against
/// repo B — the whole point of dispatching each request on its own thread (see `main.rs`'s module
/// doc comment) rather than the old single-threaded `for line in stdin.lock().lines()` loop that
/// only returned to read the next line once the previous request's `dispatch` call had returned.
///
/// The "slow" request is a `get_commit_graph` call against repo A with the
/// `BROWSITORY_SIDECAR_TEST_DELAY_METHOD`/`BROWSITORY_SIDECAR_TEST_DELAY_MS` test-only hook armed
/// (see `dispatch.rs`'s `test_delay_before_dispatch`), rather than a large synthetic history:
/// deterministic and fast to set up, where a real large repo's timing would be far less reliable
/// across machines/CI.
#[test]
fn a_slow_request_on_one_repo_does_not_delay_a_fast_request_on_another() {
    const SLOW_DELAY_MS: u64 = 500;
    const SLOW_DELAY: &str = "500";

    let (dir_a, _repo_a) = init_repo();
    let (dir_b, _repo_b) = init_repo();
    let repo_path_a = dir_a.path().to_str().unwrap().to_string();
    let repo_path_b = dir_b.path().to_str().unwrap().to_string();

    let mut sidecar = Sidecar::spawn_with_envs(&[
        ("BROWSITORY_SIDECAR_TEST_DELAY_METHOD", "get_commit_graph"),
        ("BROWSITORY_SIDECAR_TEST_DELAY_MS", SLOW_DELAY),
    ]);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path_a}));
    sidecar.call(2, "open_repo", serde_json::json!({"path": repo_path_b}));

    let start = Instant::now();
    // Slow request against repo A — sent but not waited on, so the fast request below can be
    // sent immediately after it while it's still (artificially) in flight.
    sidecar.send(
        3,
        "get_commit_graph",
        serde_json::json!({"repoPath": repo_path_a, "limit": 10, "selectedBranches": null}),
    );
    // Fast request against the unrelated repo B, sent right behind it on the same stdin pipe.
    sidecar.send(
        4,
        "get_status",
        serde_json::json!({"repoPath": repo_path_b}),
    );

    // Whichever response arrives first on stdout should be the fast one, and it should arrive
    // in well under the slow request's artificial delay — proving it was never queued behind it.
    let first = sidecar.read_response();
    let elapsed = start.elapsed();
    assert_eq!(
        first["id"], 4,
        "fast get_status response should arrive before the slow get_commit_graph response, got {first:?}"
    );
    assert_eq!(first["result"], serde_json::json!([]));
    assert!(
        elapsed < Duration::from_millis(SLOW_DELAY_MS / 2),
        "fast response took {elapsed:?}, which means it was delayed by the slow request"
    );

    let second = sidecar.read_response();
    assert_eq!(second["id"], 3);
    assert!(start.elapsed() >= Duration::from_millis(SLOW_DELAY_MS));
}

#[test]
fn history_rewrite_commands_round_trip_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first commit");
    let first = repo.head().unwrap().peel_to_commit().unwrap();
    let first_id = first.id().to_string();
    let main_ref = repo.head().unwrap().name().unwrap().to_string();
    repo.branch("side", &first, false).unwrap();
    drop(first);
    let side_ref = repo
        .find_branch("side", git2::BranchType::Local)
        .unwrap()
        .get()
        .name()
        .unwrap()
        .to_string();
    repo.set_head(&side_ref).unwrap();
    write_file(dir.path(), "side.txt", "side");
    commit_all(&repo, "side commit");
    let side_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    repo.set_head(&main_ref).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "main.txt", "main");
    commit_all(&repo, "second commit");
    drop(repo);

    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let info = sidecar.call(
        2,
        "get_head_info",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(info["result"]["headId"].as_str().unwrap().len(), 40);
    assert_eq!(info["result"]["operation"], serde_json::Value::Null);
    assert_eq!(info["result"]["detachedAt"], serde_json::Value::Null);

    let picked = sidecar.call(
        3,
        "cherry_pick",
        serde_json::json!({"repoPath": repo_path, "commitId": side_id}),
    );
    assert_eq!(picked["result"]["kind"], "Committed");
    assert!(picked["result"]["commitId"].is_string());

    let checked_out = sidecar.call(
        4,
        "checkout_commit",
        serde_json::json!({"repoPath": repo_path, "commitId": first_id}),
    );
    assert_eq!(checked_out["result"], serde_json::Value::Null);
    let detached = sidecar.call(
        5,
        "get_head_info",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert!(detached["result"]["detachedAt"].is_string());

    let reset = sidecar.call(
        6,
        "reset_to_commit",
        serde_json::json!({"repoPath": repo_path, "commitId": first_id, "mode": "hard"}),
    );
    assert_eq!(reset["result"], serde_json::Value::Null);
}

#[test]
fn line_staging_discard_and_ignore_round_trip_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "a\nb\nc\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "f.txt", "a\nB\nc\nd\n");
    write_file(dir.path(), "junk.log", "x\n");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let diff = sidecar.call(
        2,
        "get_working_diff",
        serde_json::json!({
            "repoPath": repo_path, "path": "f.txt", "staged": false,
            "opts": {"ignoreWhitespace": false, "contextLines": 3}
        }),
    );
    let hunk = &diff["result"][0];
    let shown: Vec<serde_json::Value> = hunk["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["content"].clone())
        .collect();
    let stale = sidecar.call(
        30,
        "stage_lines",
        serde_json::json!({
            "repoPath": repo_path, "path": "f.txt",
            "oldStart": hunk["oldStart"], "newStart": hunk["newStart"],
            "contextLines": 3, "lineIndices": [4], "expectedLines": ["something else\n"]
        }),
    );
    assert!(
        stale["error"].to_string().contains("hunk not found"),
        "{stale}"
    );
    let staged = sidecar.call(
        3,
        "stage_lines",
        serde_json::json!({
            "repoPath": repo_path, "path": "f.txt",
            "oldStart": hunk["oldStart"], "newStart": hunk["newStart"],
            "contextLines": 3, "lineIndices": [4], "expectedLines": shown
        }),
    );
    assert!(staged.get("error").is_none(), "{staged}");
    assert_eq!(staged["result"], serde_json::Value::Null, "{staged}");
    let staged_diff = sidecar.call(
        31,
        "get_working_diff",
        serde_json::json!({
            "repoPath": repo_path, "path": "f.txt", "staged": true,
            "opts": {"ignoreWhitespace": false, "contextLines": 3}
        }),
    );
    assert_eq!(
        staged_diff["result"].as_array().map(Vec::len),
        Some(1),
        "{staged_diff}"
    );

    let ignored = sidecar.call(
        4,
        "ignore_path",
        serde_json::json!({"repoPath": repo_path, "pattern": "*.log"}),
    );
    assert_eq!(ignored["result"], serde_json::Value::Null);
    assert!(std::fs::read_to_string(dir.path().join(".gitignore"))
        .unwrap()
        .contains("*.log"));

    let discarded = sidecar.call(
        5,
        "discard_file",
        serde_json::json!({"repoPath": repo_path, "path": "f.txt"}),
    );
    assert_eq!(discarded["result"], serde_json::Value::Null);
    write_file(dir.path(), "u.tmp", "u\n");
    write_file(dir.path(), "late.tmp", "late\n");
    let all = sidecar.call(
        6,
        "discard_all_unstaged",
        serde_json::json!({"repoPath": repo_path, "paths": ["u.tmp"]}),
    );
    assert!(all.get("error").is_none(), "{all}");
    assert!(!dir.path().join("u.tmp").exists());
    assert!(
        dir.path().join("late.tmp").exists(),
        "only the listed paths"
    );
}

#[test]
fn diff_settings_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    let set = sidecar.call(
        1,
        "set_diff_settings",
        serde_json::json!({
            "settings": {"ignoreWhitespace": true, "contextLines": 6, "syntaxHighlight": false}
        }),
    );
    assert_eq!(set["result"], serde_json::Value::Null, "{set}");
    let got = sidecar.call(2, "get_diff_settings", serde_json::json!({}));
    assert_eq!(
        got["result"],
        serde_json::json!({"ignoreWhitespace": true, "contextLines": 6, "syntaxHighlight": false})
    );
}

#[test]
fn file_history_and_rename_diff_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "old.txt", "l1\nl2\nl3\nl4\nl5\n");
    commit_all(&repo, "add");
    std::fs::remove_file(dir.path().join("old.txt")).unwrap();
    write_file(dir.path(), "new.txt", "l1\nl2\nl3\nl4\nCHANGED\n");
    {
        let mut index = repo.index().unwrap();
        index.remove_path(std::path::Path::new("old.txt")).unwrap();
        index.write().unwrap();
    }
    commit_all(&repo, "rename and edit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let history = sidecar.call(
        2,
        "get_file_history",
        serde_json::json!({"repoPath": repo_path, "path": "new.txt", "limit": 10}),
    );
    let entries = history["result"].as_array().expect("history result array");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["commit"]["summary"], "rename and edit");
    assert_eq!(entries[0]["oldPath"], "old.txt");
    assert_eq!(entries[1]["path"], "old.txt");
    assert!(entries[1]["oldPath"].is_null());

    let diff = sidecar.call(
        3,
        "get_commit_rename_diff",
        serde_json::json!({
            "repoPath": repo_path,
            "commitId": entries[0]["commit"]["id"],
            "oldPath": "old.txt",
            "newPath": "new.txt",
            "opts": {"ignoreWhitespace": false, "contextLines": 3},
        }),
    );
    let hunks = diff["result"].as_array().expect("rename diff result array");
    assert_eq!(hunks.len(), 1);
}

/// `feature` adds feature.txt; the checked-out main branch adds main.txt.
fn diverged_feature_repo() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    // Hermetic default for `get_merge_options` (issue #111): a global `merge.ff` must not leak.
    repo.config().unwrap().set_str("merge.ff", "true").unwrap();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let main_ref = repo.head().unwrap().name().unwrap().to_string();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    drop(head);
    repo.set_head("refs/heads/feature").unwrap();
    write_file(dir.path(), "feature.txt", "new\n");
    commit_all(&repo, "feature commit");
    repo.set_head(&main_ref).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "main.txt", "new\n");
    commit_all(&repo, "main commit");
    (dir, repo)
}

#[test]
fn merge_options_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = diverged_feature_repo();
    drop(repo);
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let defaults = sidecar.call(
        2,
        "get_merge_options",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(
        defaults["result"],
        serde_json::json!({"fastForward": "Allow", "squash": false})
    );
    let set = sidecar.call(
        3,
        "set_merge_options",
        serde_json::json!({"repoPath": repo_path, "options": {"fastForward": "Never", "squash": false}}),
    );
    assert_eq!(set["result"], serde_json::Value::Null);
    let remembered = sidecar.call(
        4,
        "get_merge_options",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(
        remembered["result"],
        serde_json::json!({"fastForward": "Never", "squash": false})
    );

    let squashed = sidecar.call(
        5,
        "start_merge",
        serde_json::json!({
            "repoPath": repo_path,
            "branchName": "feature",
            "options": {"fastForward": "Allow", "squash": true}
        }),
    );
    assert_eq!(squashed["result"], serde_json::json!({"kind": "Squashed"}));
    let head = sidecar.call(
        6,
        "get_head_info",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert!(head["result"]["squashMessage"]
        .as_str()
        .unwrap()
        .starts_with("Squashed commit of the following:"));
}

#[test]
fn stash_options_files_and_pop_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    write_file(dir.path(), "b.txt", "v1");
    commit_all(&repo, "initial commit");
    drop(repo);
    write_file(dir.path(), "a.txt", "mine");
    write_file(dir.path(), "b.txt", "other");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let saved = sidecar.call(
        2,
        "save_stash",
        serde_json::json!({
            "repoPath": repo_path,
            "options": {"message": "only a", "keepIndex": false, "includeUntracked": true, "paths": ["a.txt"]}
        }),
    );
    assert_eq!(saved["result"], serde_json::Value::Null, "{saved}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "other"
    );

    let stashes = sidecar.call(
        3,
        "list_stashes",
        serde_json::json!({"repoPath": repo_path}),
    );
    let entry = &stashes["result"][0];
    assert!(entry["message"].as_str().unwrap().ends_with(": only a"));
    let files = sidecar.call(
        4,
        "get_stash_files",
        serde_json::json!({"repoPath": repo_path, "commitId": entry["commitId"]}),
    );
    assert_eq!(
        files["result"],
        serde_json::json!({"tracked": ["a.txt"], "untracked": [], "untrackedCommitId": null})
    );

    let popped = sidecar.call(
        5,
        "pop_stash",
        serde_json::json!({"repoPath": repo_path, "index": 0, "commitId": entry["commitId"], "restoreIndex": true}),
    );
    assert_eq!(popped["result"], serde_json::json!({"kind": "Applied"}));
    let after = sidecar.call(
        6,
        "list_stashes",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(after["result"], serde_json::json!([]));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "mine"
    );
}

#[cfg(unix)]
#[test]
fn a_failing_pre_commit_streams_hook_progress_and_returns_hook_failed() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = init_repo();
    write_file(dir.path(), "new.txt", "hello");
    let hooks = repo.path().join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(
        hooks.join("pre-commit"),
        "#!/bin/sh\necho blocked\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(
        hooks.join("pre-commit"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));
    sidecar.call(
        2,
        "stage_file",
        serde_json::json!({"repoPath": repo_path, "path": "new.txt"}),
    );

    let (response, mut notifications) = sidecar.call_and_collect_notifications(
        3,
        "commit",
        serde_json::json!({"repoPath": repo_path, "message": "m", "noVerify": false}),
        false,
    );

    assert_eq!(
        response["result"],
        serde_json::json!({"kind": "HookFailed", "hook": "pre-commit", "output": "blocked\n"})
    );
    // The relay thread may write after the response line; both events are already queued.
    let is_finished =
        |n: &serde_json::Value| n["method"] == "hookProgress" && n["params"]["phase"] == "Finished";
    while !notifications.iter().any(is_finished) {
        notifications.push(sidecar.read_line());
    }
    let phases: Vec<&serde_json::Value> = notifications
        .iter()
        .filter(|n| n["method"] == "hookProgress")
        .map(|n| &n["params"]["phase"])
        .collect();
    assert_eq!(phases, ["Started", "Finished"]);
    let started = &notifications[0]["params"];
    assert_eq!(started["hook"], "pre-commit");
    assert_eq!(started["repoPath"], repo_path.as_str());
    assert!(started["operationId"]
        .as_str()
        .unwrap()
        .starts_with("commit-"));
}

impl Sidecar {
    /// Spawns with `home` as `HOME`/`USERPROFILE`/`XDG_CONFIG_HOME`, so libgit2's global git
    /// config is `home/.gitconfig` and never the developer's own.
    fn spawn_with_home(config_dir: &std::path::Path, home: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vscode-sidecar"))
            .env("BROWSITORY_CONFIG_DIR", config_dir)
            .env("HOME", home)
            .env("USERPROFILE", home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
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
}

#[test]
fn external_tool_settings_and_git_tools_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let home = tempfile::TempDir::new().unwrap();
    std::fs::write(home.path().join(".gitconfig"), "[diff]\n\ttool = meld\n").unwrap();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    repo.config()
        .unwrap()
        .set_str("merge.tool", "kdiff3")
        .unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_home(&config_dir, home.path());
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let set = sidecar.call(
        2,
        "set_external_tool_settings",
        serde_json::json!({"settings": {"editor": "code", "terminal": null}}),
    );
    assert_eq!(set["result"], serde_json::Value::Null);
    let invalid = sidecar.call(
        30,
        "set_external_tool_settings",
        serde_json::json!({"settings": {"editor": "code \"{file}", "terminal": null}}),
    );
    assert_eq!(
        invalid["error"]["message"],
        "Editor command: The command has an unclosed quote."
    );
    let got = sidecar.call(3, "get_external_tool_settings", serde_json::json!({}));
    assert_eq!(
        got["result"],
        serde_json::json!({"editor": "code", "terminal": null})
    );

    let tools = sidecar.call(
        4,
        "get_git_tools",
        serde_json::json!({"repoPath": repo_path}),
    );
    let diff = &tools["result"]["diffTool"];
    assert_eq!(diff["name"], "meld");
    assert_eq!(diff["source"], "Global");
    assert_eq!(diff["command"], "meld $LOCAL $REMOTE");
    assert_eq!(diff["viaMergeTool"], false);
    // The repository's own merge.tool is reported, but ignored.
    let merge = &tools["result"]["mergeTool"];
    assert_eq!(merge["name"], "kdiff3");
    assert_eq!(merge["source"], "Local");
    assert_eq!(merge["command"], serde_json::Value::Null);
    assert!(merge["problem"]
        .as_str()
        .unwrap()
        .contains("only in this repository's git config"));
}

#[test]
fn read_file_version_and_repo_workdir_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    write_file(dir.path(), "a.txt", "hello\n");
    write_file(dir.path(), "sub/b.txt", "b\n");
    commit_all(&repo, "base");
    let repo_path = dir.path().join("sub").to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let head = sidecar.call(
        2,
        "read_file_version",
        serde_json::json!({"repoPath": repo_path, "path": "a.txt", "version": {"kind": "Head"}}),
    );
    assert_eq!(head["result"], serde_json::json!({"text": "hello\n"}));
    let missing = sidecar.call(
        3,
        "read_file_version",
        serde_json::json!({"repoPath": repo_path, "path": "nope.txt", "version": {"kind": "Index"}}),
    );
    assert_eq!(missing["result"], serde_json::json!({"text": null}));
    let bad_stage = sidecar.call(
        4,
        "read_file_version",
        serde_json::json!({"repoPath": repo_path, "path": "a.txt", "version": {"kind": "Stage", "stage": 0}}),
    );
    assert_eq!(bad_stage["error"]["message"], "stage must be 1, 2 or 3");
    let escape = sidecar.call(
        5,
        "read_file_version",
        serde_json::json!({"repoPath": repo_path, "path": "../a.txt", "version": {"kind": "Head"}}),
    );
    assert_eq!(escape["error"]["message"], "Invalid path.");
    let workdir = sidecar.call(
        6,
        "repo_workdir",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(
        std::path::Path::new(workdir["result"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        dir.path().canonicalize().unwrap()
    );
}

#[test]
fn mark_resolved_round_trips_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    let main = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "a.txt", "theirs\n");
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    write_file(dir.path(), "a.txt", "ours\n");
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let check = sidecar.call(
        20,
        "check_merge_target",
        serde_json::json!({"repoPath": repo_path, "path": "a.txt"}),
    );
    assert_eq!(check["result"], serde_json::Value::Null);
    let no_conflict = sidecar.call(
        21,
        "check_merge_target",
        serde_json::json!({"repoPath": repo_path, "path": "missing.txt"}),
    );
    assert_eq!(
        no_conflict["error"]["message"],
        "no conflict found for path 'missing.txt'"
    );
    let blocked = sidecar.call(
        2,
        "mark_resolved",
        serde_json::json!({"repoPath": repo_path, "path": "a.txt", "force": false}),
    );
    assert_eq!(blocked["result"], "MarkersRemain");
    write_file(dir.path(), "a.txt", "merged\n");
    let outcome = sidecar.call(
        3,
        "mark_resolved",
        serde_json::json!({"repoPath": repo_path, "path": "a.txt", "force": false}),
    );
    assert_eq!(outcome["result"], "Resolved");
}

#[test]
fn cherry_pick_many_conflicts_then_continue_and_whole_file_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().shorthand().unwrap().to_string();
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
    }
    repo.set_head("refs/heads/feature").unwrap();
    write_file(dir.path(), "a.txt", "feature\n");
    commit_all(&repo, "feature edit");
    let feature_tip = repo.head().unwrap().target().unwrap().to_string();
    repo.set_head(&format!("refs/heads/{base}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "a.txt", "main\n");
    commit_all(&repo, "main edit");

    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({ "path": repo_path }));

    let started = sidecar.call(
        2,
        "cherry_pick_many",
        serde_json::json!({ "repoPath": repo_path, "commitIds": [feature_tip] }),
    );
    assert_eq!(started["result"]["kind"], "Conflicted");
    assert_eq!(started["result"]["progress"]["totalSteps"], 1);
    let head = sidecar.call(
        3,
        "get_head_info",
        serde_json::json!({ "repoPath": repo_path }),
    );
    assert_eq!(head["result"]["sequence"]["currentStep"], 1);
    assert_eq!(head["result"]["sequence"]["kind"], "CherryPick");

    let refused = sidecar.call(
        4,
        "cherry_pick",
        serde_json::json!({ "repoPath": repo_path, "commitId": feature_tip, "mainline": null }),
    );
    assert_eq!(
        refused["error"]["message"],
        "Finish or abort the current merge/cherry-pick/revert/rebase first."
    );
    sidecar.call(
        5,
        "resolve_whole_file",
        serde_json::json!({ "repoPath": repo_path, "path": "a.txt", "side": "Theirs" }),
    );
    let done = sidecar.call(
        6,
        "continue_sequence",
        serde_json::json!({ "repoPath": repo_path }),
    );
    assert_eq!(
        done["result"],
        serde_json::json!({ "kind": "Completed", "applied": 1, "skipped": [] })
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "feature\n"
    );
    let aborted = sidecar.call(
        7,
        "abort_sequence",
        serde_json::json!({ "repoPath": repo_path }),
    );
    assert_eq!(
        aborted["error"]["message"],
        "No cherry-pick or revert is in progress."
    );
    let reverted = sidecar.call(
        8,
        "revert_many",
        serde_json::json!({ "repoPath": repo_path, "commitIds": [feature_tip] }),
    );
    assert_eq!(reverted["result"]["kind"], "Completed");
    let head = sidecar.call(
        9,
        "get_head_info",
        serde_json::json!({ "repoPath": repo_path }),
    );
    assert_eq!(head["result"]["sequence"], serde_json::Value::Null);
}

#[test]
fn a_merge_commit_is_picked_with_a_mainline_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    let merge_id = {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = repo.signature().unwrap();
        let mut builder = repo.treebuilder(Some(&base.tree().unwrap())).unwrap();
        builder
            .insert("f.txt", repo.blob(b"f\n").unwrap(), 0o100644)
            .unwrap();
        let tree = repo.find_tree(builder.write().unwrap()).unwrap();
        let side = repo
            .commit(None, &sig, &sig, "side", &tree, &[&base])
            .unwrap();
        let side = repo.find_commit(side).unwrap();
        repo.commit(None, &sig, &sig, "merge side", &tree, &[&base, &side])
            .unwrap()
            .to_string()
    };
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({ "path": repo_path }));
    let refused = sidecar.call(
        2,
        "cherry_pick",
        serde_json::json!({ "repoPath": repo_path, "commitId": merge_id }),
    );
    assert_eq!(
        refused["error"]["message"],
        "This is a merge commit. Choose which parent to apply it against."
    );
    let picked = sidecar.call(
        3,
        "cherry_pick",
        serde_json::json!({ "repoPath": repo_path, "commitId": merge_id, "mainline": 1 }),
    );
    assert_eq!(picked["result"]["kind"], "Committed");
    assert!(dir.path().join("f.txt").exists());
}
