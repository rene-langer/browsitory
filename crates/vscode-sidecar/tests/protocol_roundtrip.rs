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
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "staged": false}),
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
        serde_json::json!({"repoPath": repo_path, "commitId": head_id, "path": "tracked.txt"}),
    );
    let commit_hunks = commit_diff["result"]
        .as_array()
        .expect("commit diff result array");
    assert_eq!(commit_hunks.len(), 1);
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
    let commit_id = committed["result"].as_str().expect("commit id").to_string();
    assert_eq!(commit_id.len(), 40);

    let files = sidecar.call(
        4,
        "get_commit_files",
        serde_json::json!({"repoPath": repo_path, "commitId": commit_id}),
    );
    assert_eq!(files["result"], serde_json::json!(["new.txt"]));

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
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "staged": false}),
    );
    let hunk = &diff["result"][0];
    let old_start = hunk["oldStart"].as_u64().unwrap();
    let new_start = hunk["newStart"].as_u64().unwrap();

    let staged = sidecar.call(
        3,
        "stage_hunk",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "oldStart": old_start, "newStart": new_start}),
    );
    assert_eq!(staged["result"], serde_json::Value::Null);

    let status = sidecar.call(4, "get_status", serde_json::json!({"repoPath": repo_path}));
    assert!(status["result"][0]["staged"].as_bool().unwrap());

    let unstaged = sidecar.call(
        5,
        "unstage_hunk",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "oldStart": old_start, "newStart": new_start}),
    );
    assert_eq!(unstaged["result"], serde_json::Value::Null);

    let discarded = sidecar.call(
        6,
        "discard_hunk",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "oldStart": old_start, "newStart": new_start}),
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

    let listed = sidecar.call(3, "list_worktrees", serde_json::json!({"repoPath": repo_path}));
    let worktrees = listed["result"].as_array().expect("worktree list");
    assert!(worktrees.iter().any(|w| w["name"] == "feature-tree" && w["isMain"] == false));

    let removed = sidecar.call(
        4,
        "remove_worktree",
        serde_json::json!({"repoPath": repo_path, "name": "feature-tree"}),
    );
    assert_eq!(removed["result"], serde_json::Value::Null);
    assert!(!linked.exists());

    let pruned = sidecar.call(5, "prune_worktrees", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(pruned["result"], serde_json::Value::Null);
}
