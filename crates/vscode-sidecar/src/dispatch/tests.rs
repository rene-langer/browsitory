use std::path::Path;

use super::*;
use git_core::remote::{TransferErrorKind, TransferOperation, TransferPhase, TransferProgress};
use repo_service::wire::{
    transfer_progress_dto, OpenRepoEntryDto, PickOutcomeDto, PullOutcomeDto, SequenceOutcomeDto,
};
use repo_service::worker::{HookSource, TransferEvent};

/// A handler's typed params, written as the request's JSON.
macro_rules! params {
    ($($json:tt)+) => {
        serde_json::from_value(serde_json::json!($($json)+)).expect("well-formed params")
    };
}

/// #306: the TypeScript shape of the sidecar's own wire DTOs (the shared ones are in
/// `repo_service::wire`), checked in as `frontend/src/ipc/generated/sidecar-dto.ts` and
/// checked against `RepoClient.ts` by `frontend/src/ipc/dtoContract.ts`. Regenerate with
/// `UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar dto_bindings_are_current`.
#[test]
fn dto_bindings_are_current() {
    let bindings = test_support::ts_declarations!(
        "// Generated from the VSCode sidecar's own wire DTOs (crates/vscode-sidecar/src/dispatch.rs) by ts-rs.\n\
         // The shared ones are in wire-dto.ts.\n\
         // Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar dto_bindings_are_current\n";
        import "./wire-dto" { OpenRepoEntryDto };
        FileVersionDto,
        OpenReposDto,
    );
    test_support::assert_generated_file(
        &test_support::repo_root().join("frontend/src/ipc/generated/sidecar-dto.ts"),
        &bindings,
    );
}

/// #306. Regenerate with
/// `UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar method_list_is_current`.
#[test]
fn method_list_is_current() {
    let mut unique = METHODS.to_vec();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), METHODS.len(), "a method is listed twice");
    test_support::assert_generated_file(
        &test_support::repo_root().join("frontend/src/ipc/generated/sidecar-methods.json"),
        &test_support::json_name_list(METHODS),
    );
}

/// A repository with a subfolder `sub`, opened as `opened(root)`; returns the temp dir.
fn repo_opened_as(repos: &Repos, opened: impl Fn(&Path) -> String) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    open_repo(params!({ "path": opened(dir.path()) }), repos).unwrap();
    dir
}

/// #223: a request through another spelling of an open repository reaches its worker.
#[test]
fn a_repository_opened_with_a_trailing_slash_answers_another_spelling() {
    let repos: Repos = Arc::default();
    let dir = repo_opened_as(&repos, |path| {
        format!("{}{}", path.display(), std::path::MAIN_SEPARATOR)
    });
    let plain = dir.path().to_str().unwrap().to_string();
    open_repo(params!({ "path": plain }), &repos).unwrap();

    let status = get_status(params!({ "repoPath": plain }), &repos).unwrap();

    assert_eq!(status, serde_json::json!([]));
    assert_eq!(repos.len(), 1);
}

/// #223: closing one spelling keeps the repository open for the other.
#[test]
fn closing_one_spelling_keeps_the_other_open() {
    let repos: Repos = Arc::default();
    let dir = repo_opened_as(&repos, |path| path.to_str().unwrap().to_string());
    let plain = dir.path().to_str().unwrap().to_string();
    let sub = dir.path().join("sub").to_str().unwrap().to_string();
    open_repo(params!({ "path": sub }), &repos).unwrap();

    close_repo(params!({ "repoPath": sub }), &repos).unwrap();

    assert!(get_status(params!({ "repoPath": plain }), &repos).is_ok());
    assert_eq!(
        get_status(params!({ "repoPath": sub }), &repos)
            .unwrap_err()
            .message,
        format!("repo not open: {sub}")
    );
    close_repo(params!({ "repoPath": plain }), &repos).unwrap();
    assert!(repos.is_empty());
}

fn lines_of(buffer: &Arc<Mutex<Vec<u8>>>) -> Vec<Value> {
    let written = buffer.lock().expect("buffer lock").clone();
    String::from_utf8(written)
        .expect("relay output is utf-8")
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("incomplete line {line:?}: {error}"))
        })
        .collect()
}

#[test]
fn progress_relay_writes_one_notification_line_per_event_then_exits_when_the_channel_closes() {
    let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let (event_tx, event_rx) = mpsc::channel();
    let relay = spawn_progress_relay(event_rx, Arc::clone(&buffer));

    event_tx
        .send(TransferEvent::Started {
            operation_id: "fetch-1".to_string(),
            operation: TransferOperation::Fetch,
        })
        .unwrap();
    event_tx
        .send(TransferEvent::Progress(TransferProgress {
            operation_id: "fetch-1".to_string(),
            operation: TransferOperation::Fetch,
            phase: TransferPhase::Receiving,
            current: 3,
            total: 10,
            received_bytes: 128,
            message: Some("remote: sideband text from the server".to_string()),
        }))
        .unwrap();
    event_tx
        .send(TransferEvent::Completed {
            operation_id: "fetch-1".to_string(),
            operation: TransferOperation::Fetch,
            error: Some(TransferErrorKind::TransferFailed),
            hook_output: None,
        })
        .unwrap();

    // Dropping the last sender is the relay's only shutdown signal — exactly what
    // `repo-service` does once the worker thread finishes the operation. Nothing joins this
    // thread in production, so if it did not exit on its own it would leak one thread per
    // transfer; this join is the test that it does.
    drop(event_tx);
    relay
        .join()
        .expect("relay thread exits once its channel's senders are dropped");

    let lines = lines_of(&buffer);
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|line| line["jsonrpc"] == "2.0"
        && line["method"] == "transferProgress"
        && line.get("id").is_none()));
    assert_eq!(
        lines[0]["params"],
        serde_json::json!({
            "operationId": "fetch-1",
            "operation": "Fetch",
            "phase": "Starting",
            "errorKind": null,
            "current": 0,
            "total": 0,
            "receivedBytes": 0,
            "message": null,
            "hookOutput": null,
        })
    );
    // The sideband text above is dropped, not forwarded — same redaction
    // `crates/tauri-app/src/commands/mod.rs` applies on the Tauri transport.
    assert_eq!(
        lines[1]["params"],
        serde_json::json!({
            "operationId": "fetch-1",
            "operation": "Fetch",
            "phase": "Receiving",
            "errorKind": null,
            "current": 3,
            "total": 10,
            "receivedBytes": 128,
            "message": null,
            "hookOutput": null,
        })
    );
    assert_eq!(
        lines[2]["params"],
        serde_json::json!({
            "operationId": "fetch-1",
            "operation": "Fetch",
            "phase": "Failed",
            "errorKind": "TransferFailed",
            "current": 0,
            "total": 0,
            "receivedBytes": 0,
            "message": null,
            "hookOutput": null,
        })
    );
}

#[test]
fn concurrent_relays_sharing_one_writer_never_interleave_a_line() {
    // Two transfers can be in flight at once, each with its own relay thread, all writing to
    // the one shared stdout. The mutex is held across write-plus-flush, so every line must
    // still arrive whole and attributable to exactly one operation.
    let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let (first_tx, first_rx) = mpsc::channel();
    let (second_tx, second_rx) = mpsc::channel();
    let first = spawn_progress_relay(first_rx, Arc::clone(&buffer));
    let second = spawn_progress_relay(second_rx, Arc::clone(&buffer));

    let senders = [(first_tx, "fetch-1"), (second_tx, "push-2")];
    let feeders: Vec<_> = senders
        .into_iter()
        .map(|(tx, operation_id)| {
            std::thread::spawn(move || {
                for index in 0..200 {
                    tx.send(TransferEvent::Progress(TransferProgress {
                        operation_id: operation_id.to_string(),
                        operation: TransferOperation::Fetch,
                        phase: TransferPhase::Receiving,
                        current: index,
                        total: 200,
                        received_bytes: index * 16,
                        message: None,
                    }))
                    .unwrap();
                }
            })
        })
        .collect();
    for feeder in feeders {
        feeder.join().expect("feeder thread");
    }
    first.join().expect("first relay exits");
    second.join().expect("second relay exits");

    // `lines_of` parses every line, so a torn write fails here.
    let lines = lines_of(&buffer);
    assert_eq!(lines.len(), 400);
    assert_eq!(
        lines
            .iter()
            .filter(|line| line["params"]["operationId"] == "fetch-1")
            .count(),
        200
    );
    assert_eq!(
        lines
            .iter()
            .filter(|line| line["params"]["operationId"] == "push-2")
            .count(),
        200
    );
}

#[test]
fn remote_failed_event_carries_the_remote_name_and_other_events_omit_it() {
    let failed = serde_json::to_value(transfer_progress_dto(TransferEvent::RemoteFailed {
        operation_id: "fetch-all-7".into(),
        operation: TransferOperation::FetchAll,
        remote_name: "backup".into(),
        error: TransferErrorKind::TransferFailed,
    }))
    .unwrap();
    assert_eq!(failed["phase"], "RemoteFailed");
    assert_eq!(failed["operation"], "FetchAll");
    assert_eq!(failed["remoteName"], "backup");
    assert_eq!(failed["errorKind"], "TransferFailed");

    let completed = serde_json::to_value(transfer_progress_dto(TransferEvent::Completed {
        operation_id: "fetch-all-7".into(),
        operation: TransferOperation::FetchAll,
        error: None,
        hook_output: None,
    }))
    .unwrap();
    assert!(!completed.as_object().unwrap().contains_key("remoteName"));
    assert_eq!(completed["hookOutput"], serde_json::Value::Null);

    let hook_failed = serde_json::to_value(transfer_progress_dto(TransferEvent::Completed {
        operation_id: "push-8".into(),
        operation: TransferOperation::PushBranch,
        error: Some(TransferErrorKind::HookFailed),
        hook_output: Some("tests failed\n".into()),
    }))
    .unwrap();
    assert_eq!(hook_failed["phase"], "Failed");
    assert_eq!(hook_failed["errorKind"], "HookFailed");
    assert_eq!(hook_failed["hookOutput"], "tests failed\n");
    assert_eq!(hook_failed["message"], serde_json::Value::Null);
}

#[test]
fn pull_outcome_serializes_with_a_kind_tag_and_camel_case_fields() {
    assert_eq!(
        serde_json::to_value(PullOutcomeDto::from(
            git_core::remote::PullOutcome::UpToDate
        ))
        .unwrap(),
        serde_json::json!({"kind": "UpToDate"})
    );
    assert_eq!(
        serde_json::to_value(PullOutcomeDto::from(
            git_core::remote::PullOutcome::FastForwarded {
                upstream_ref: "refs/remotes/origin/main".to_string(),
            }
        ))
        .unwrap(),
        serde_json::json!({"kind": "FastForwarded", "upstreamRef": "refs/remotes/origin/main"})
    );
    assert_eq!(
        serde_json::to_value(PullOutcomeDto::from(
            git_core::remote::PullOutcome::Diverged {
                upstream_ref: "refs/remotes/origin/main".to_string(),
            }
        ))
        .unwrap(),
        serde_json::json!({"kind": "Diverged", "upstreamRef": "refs/remotes/origin/main"})
    );
    assert_eq!(
        serde_json::to_value(PullOutcomeDto::from(
            git_core::remote::PullOutcome::NoFastForward {
                upstream_ref: "refs/remotes/origin/main".to_string(),
            }
        ))
        .unwrap(),
        serde_json::json!({"kind": "NoFastForward", "upstreamRef": "refs/remotes/origin/main"})
    );
}

/// base: a.txt="base\n"; `feature` adds b.txt, changes a.txt, adds c.txt; the checked-out
/// branch changed a.txt too, so a three-commit cherry-pick pauses on step 2.
#[cfg(unix)]
fn paused_pick_repo() -> (tempfile::TempDir, Vec<String>) {
    use git2::{IndexAddOption, Repository};
    let dir = tempfile::TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_bool("commit.gpgSign", false).unwrap();
        let hooks = repo.path().join("hooks");
        config
            .set_str("core.hooksPath", hooks.to_str().unwrap())
            .unwrap();
    }
    let commit_all = |message: &str| {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap()
            .to_string()
    };
    std::fs::write(dir.path().join("a.txt"), "base\n").unwrap();
    commit_all("base");
    let base_branch = repo.head().unwrap().name().unwrap().to_string();
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
    }
    repo.set_head("refs/heads/feature").unwrap();
    let mut ids = Vec::new();
    for (path, contents, message) in [
        ("b.txt", "b\n", "f one"),
        ("a.txt", "feature\n", "f two"),
        ("c.txt", "c\n", "f three"),
    ] {
        std::fs::write(dir.path().join(path), contents).unwrap();
        ids.push(commit_all(message));
    }
    repo.set_head(&base_branch).unwrap();
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new()
            .force()
            .remove_untracked(true),
    ))
    .unwrap();
    std::fs::write(dir.path().join("a.txt"), "main\n").unwrap();
    commit_all("main change");
    (dir, ids)
}

#[test]
fn hook_progress_dto_serializes_every_source() {
    for (source, name) in [
        (HookSource::Commit, "Commit"),
        (HookSource::Checkout, "Checkout"),
        (HookSource::Merge, "Merge"),
        (HookSource::Pick, "Pick"),
    ] {
        let dto = hook_progress_dto(
            "/repo",
            HookEvent::Finished {
                operation_id: "op-1".into(),
                hook: "post-checkout".into(),
                source,
            },
        );
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({"repoPath": "/repo", "operationId": "op-1", "hook": "post-checkout", "phase": "Finished", "source": name})
        );
    }
}

#[test]
fn pick_outcome_dto_serializes_a_hook_failure() {
    assert_eq!(
        serde_json::to_value(PickOutcomeDto::from(
            git_core::pick::PickOutcome::HookFailed {
                hook: "prepare-commit-msg".into(),
                output: "no ticket\n".into()
            }
        ))
        .unwrap(),
        serde_json::json!({"kind": "HookFailed", "hook": "prepare-commit-msg", "output": "no ticket\n"})
    );
}

#[test]
fn sequence_outcome_dto_serializes_a_stop_after_cancel() {
    let value = serde_json::to_value(SequenceOutcomeDto::from(
        git_core::pick::SequenceOutcome::Stopped {
            kept: "f one".into(),
            progress: git_core::pick::SequenceProgress {
                kind: git_core::pick::SequenceKind::Revert,
                current_step: 2,
                total_steps: 3,
                current_commit: "c2".into(),
            },
        },
    ))
    .unwrap();
    assert_eq!(value["kind"], "Stopped");
    assert_eq!(value["kept"], "f one");
    assert_eq!(value["progress"]["currentStep"], 2);
}

/// Spawns a worker for `dir`, installs `hook` (exit 0) and returns the repos map and a
/// relay buffer.
#[cfg(unix)]
fn relay_fixture(dir: &std::path::Path, hook: &str) -> (Repos, Arc<Mutex<Vec<u8>>>) {
    use std::os::unix::fs::PermissionsExt;
    let repos: Repos = Arc::default();
    repos.open(dir.to_str().unwrap()).unwrap();
    let hooks = dir.join(".git").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let path = hooks.join(hook);
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    (repos, Arc::new(Mutex::new(Vec::new())))
}

/// The two `hookProgress` lines (Started, Finished) the relay wrote, waiting for them.
#[cfg(unix)]
fn two_relayed_lines(buffer: &Arc<Mutex<Vec<u8>>>) -> Vec<Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while lines_of(buffer).len() < 2 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let lines = lines_of(buffer);
    assert_eq!(lines.len(), 2, "{lines:?}");
    lines
}

/// #185: `post-checkout` after a switch reaches the webview with `source: "Checkout"`.
#[cfg(unix)]
#[test]
fn switch_branch_relays_post_checkout_with_source_checkout() {
    let (dir, _ids) = paused_pick_repo();
    let path = dir.path().to_str().unwrap().to_string();
    let (repos, buffer) = relay_fixture(dir.path(), "post-checkout");

    switch_branch(
        params!({ "repoPath": path, "name": "feature" }),
        &repos,
        &buffer,
    )
    .unwrap();

    let lines = two_relayed_lines(&buffer);
    assert!(lines.iter().all(|line| line["method"] == "hookProgress"
        && line["params"]["hook"] == "post-checkout"
        && line["params"]["source"] == "Checkout"));
    assert!(lines[0]["params"]["operationId"]
        .as_str()
        .unwrap()
        .starts_with("switch-"));
}

/// #185: `post-merge` after a fast-forward reaches the webview with `source: "Merge"`.
#[cfg(unix)]
#[test]
fn start_merge_relays_post_merge_with_source_merge() {
    let (dir, _ids) = paused_pick_repo();
    {
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(
            Some("refs/heads/ahead"),
            &sig,
            &sig,
            "ahead",
            &head.tree().unwrap(),
            &[&head],
        )
        .unwrap();
    }
    let path = dir.path().to_str().unwrap().to_string();
    let (repos, buffer) = relay_fixture(dir.path(), "post-merge");

    let outcome = start_merge(
        params!({
            "repoPath": path,
            "branchName": "ahead",
            "options": { "fastForward": "Allow", "squash": false }
        }),
        &repos,
        &buffer,
    )
    .unwrap();

    assert_eq!(outcome["kind"], "FastForwarded");
    let lines = two_relayed_lines(&buffer);
    assert!(lines.iter().all(|line| line["method"] == "hookProgress"
        && line["params"]["hook"] == "post-merge"
        && line["params"]["source"] == "Merge"));
    assert!(lines[0]["params"]["operationId"]
        .as_str()
        .unwrap()
        .starts_with("merge-"));
}

/// #183/#185: a clean pick's hooks reach the webview as `hookProgress` with
/// `source: "Pick"`.
#[cfg(unix)]
#[test]
fn cherry_pick_many_relays_its_hooks_with_source_pick() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, ids) = paused_pick_repo();
    let path = dir.path().to_str().unwrap().to_string();
    let repos: Repos = Arc::default();
    repos.open(&path).unwrap();
    let hooks = dir.path().join(".git").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("post-commit");
    std::fs::write(&hook, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

    let outcome = cherry_pick_many(
        params!({ "repoPath": path, "commitIds": [ids[0]] }),
        &repos,
        &buffer,
    )
    .unwrap();

    assert_eq!(outcome["kind"], "Completed");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while lines_of(&buffer).len() < 2 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let lines = lines_of(&buffer);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines.iter().all(|line| line["method"] == "hookProgress"
        && line["params"]["hook"] == "post-commit"
        && line["params"]["source"] == "Pick"));
    assert!(lines[0]["params"]["operationId"]
        .as_str()
        .unwrap()
        .starts_with("pick-"));
}

#[cfg(unix)]
#[test]
fn continue_sequence_accepts_no_verify_and_relays_hook_progress() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, ids) = paused_pick_repo();
    let path = dir.path().to_str().unwrap().to_string();
    let repos: Repos = Arc::default();
    repos.open(&path).unwrap();
    let handle = worker_handle(&repos, &path).unwrap();
    handle.cherry_pick_many(ids, mpsc::channel().0).unwrap();
    handle
        .resolve_conflict("a.txt".into(), "resolved\n".into())
        .unwrap();
    let hooks = dir.path().join(".git").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("commit-msg");
    std::fs::write(&hook, "#!/bin/sh\necho no change id; exit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

    let failed = continue_sequence(params!({ "repoPath": path }), &repos, &buffer).unwrap();

    assert_eq!(failed["kind"], "HookFailed");
    assert_eq!(failed["hook"], "commit-msg");
    assert_eq!(failed["output"], "no change id\n");
    assert_eq!(failed["progress"]["currentStep"], 2);
    // The relay thread may still be writing the last line.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while lines_of(&buffer).len() < 2 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let lines = lines_of(&buffer);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines.iter().all(|line| line["method"] == "hookProgress"));
    assert_eq!(lines[0]["params"]["hook"], "commit-msg");
    assert_eq!(lines[0]["params"]["source"], "Commit");
    assert!(lines[0]["params"]["operationId"]
        .as_str()
        .unwrap()
        .starts_with("continue-"));

    let completed = continue_sequence(
        params!({ "repoPath": path, "noVerify": true }),
        &repos,
        &buffer,
    )
    .unwrap();
    assert_eq!(completed["kind"], "Completed");
    assert_eq!(completed["applied"], 3);
}
