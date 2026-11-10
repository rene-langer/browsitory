use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use git2::Repository;
use tempfile::TempDir;

use super::state_lock::{Acquire, StateGuard, StateLock, CONTENDED_WAIT, LOCK_WAIT};
use super::undo::{DISCARD_BUSY, UNDO_BUSY};
use super::{UndoOutcome, UndoState, Worker};

fn init_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_bool("commit.gpgSign", false).unwrap();
    }
    (dir, repo)
}

fn write_file(dir: &Path, name: &str, contents: &str) {
    std::fs::write(dir.join(name), contents).unwrap();
}

fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap()
}

fn hold(repo: &Repository) -> StateGuard {
    match StateLock::open(repo).acquire(Duration::ZERO) {
        Acquire::Held(guard) => guard,
        other => panic!("{other:?}"),
    }
}

fn no_undo() -> UndoState {
    UndoState {
        undo: None,
        redo: None,
    }
}

/// `main` and `feature` both change `a.txt`; HEAD on main.
fn conflicting_branches() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    let base_branch = repo.head().unwrap().name().unwrap().to_string();
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
    }
    repo.set_head("refs/heads/feature").unwrap();
    write_file(dir.path(), "a.txt", "feature\n");
    commit_all(&repo, "feature change");
    repo.set_head(&base_branch).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "a.txt", "main\n");
    commit_all(&repo, "main change");
    (dir, repo)
}

/// Starts a cherry-pick sequence of `side_commits_with_a_conflict` through git-core, as the
/// other window's worker would, and stores it. Returns the branch it runs on.
fn start_sequence_outside(repo: &Repository, ids: Vec<String>) -> String {
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let store = super::sequence_store::SequenceStore::open(repo);
    let cancelled = super::CancelRegistry::default();
    assert!(matches!(
        super::history::pick_many(
            repo,
            &store,
            git_core::pick::SequenceKind::CherryPick,
            ids,
            super::remote::TransferOp::new(cancelled.allocate("pick")),
            mpsc::channel().0,
        )
        .unwrap(),
        git_core::pick::SequenceOutcome::Conflicted { .. }
    ));
    branch
}

fn in_progress() -> String {
    git_core::pick::PickError::OperationInProgress.to_string()
}

/// The worker waits on the lock (`PauseControl::wait_reached`, #274) before the outside
/// change lands, and its wait starts only after `go`, so neither a slow runner nor a slow
/// outside change lets the command run before the change is complete.
#[test]
fn stash_save_waits_for_the_lock_and_sees_a_sequence_stored_meanwhile() {
    let (dir, repo, ids) = super::tests::side_commits_with_a_conflict();
    let (worker, pause) = Worker::spawn_with_pause_on_lock_wait(dir.path().to_path_buf());
    let handle = worker.handle();

    let guard = hold(&repo);
    pause.arm();
    let saving = {
        let handle = handle.clone();
        std::thread::spawn(move || handle.save_stash(Default::default()))
    };
    pause.wait_reached();
    start_sequence_outside(&repo, ids);
    write_file(dir.path(), "b.txt", "edited\n");
    drop(guard);
    pause.go();
    assert_eq!(
        saving.join().unwrap(),
        Err(crate::wire::ErrorDto::from(in_progress()))
    );
    assert!(repo.find_reference("refs/stash").is_err());
}

#[test]
fn worktree_add_waits_for_the_lock_and_sees_a_sequence_stored_meanwhile() {
    let (dir, repo, ids) = super::tests::side_commits_with_a_conflict();
    let (worker, pause) = Worker::spawn_with_pause_on_lock_wait(dir.path().to_path_buf());
    let handle = worker.handle();
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let path = dir.path().with_extension("wt");

    let guard = hold(&repo);
    pause.arm();
    let adding = {
        let (handle, path) = (handle.clone(), path.clone());
        std::thread::spawn(move || handle.create_worktree("wt".into(), path, branch, None))
    };
    pause.wait_reached();
    start_sequence_outside(&repo, ids);
    drop(guard);
    pause.go();
    assert_eq!(
        adding.join().unwrap(),
        Err(crate::wire::ErrorDto::from(in_progress()))
    );
    assert!(!path.exists());
}

/// #274: with both a stored sequence and index conflicts, a stash save names the operation
/// (the worker's gate runs before git-core's conflict check), not the conflicts.
#[test]
fn stash_save_names_the_operation_when_it_also_has_conflicts() {
    let (dir, repo, ids) = super::tests::side_commits_with_a_conflict();
    start_sequence_outside(&repo, ids);
    assert!(repo.index().unwrap().has_conflicts());
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

    assert_eq!(
        worker.handle().save_stash(Default::default()),
        Err(crate::wire::ErrorDto::from(in_progress()))
    );
    assert!(repo.find_reference("refs/stash").is_err());
}

#[test]
fn a_rebase_continued_while_another_worker_reads_is_still_one_undo_entry() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "base\n");
    let base = commit_all(&repo, "base");
    write_file(dir.path(), "a.txt", "a\n");
    let a = commit_all(&repo, "a");
    // `onto` is base + h.txt, off the branch.
    let onto = {
        let base = repo.find_commit(base).unwrap();
        let mut tree = repo.treebuilder(Some(&base.tree().unwrap())).unwrap();
        tree.insert("h.txt", repo.blob(b"h\n").unwrap(), 0o100644)
            .unwrap();
        let tree = repo.find_tree(tree.write().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(None, &sig, &sig, "h", &tree, &[&base]).unwrap()
    };

    let (worker_a, pause) = Worker::spawn_with_pause_before_finish(dir.path().to_path_buf());
    let worker_b = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let (handle_a, handle_b) = (worker_a.handle(), worker_b.handle());
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: a.to_string(),
        action: git_core::rebase::RebaseAction::Edit,
        combined_message: None,
    }];
    assert_eq!(
        handle_a.start_rebase(onto.to_string(), plan, None).unwrap(),
        git_core::rebase::RebaseStepResult::PausedForEdit
    );
    assert_eq!(handle_b.get_undo_state().unwrap(), no_undo());

    pause.arm();
    let continuing = {
        let handle_a = handle_a.clone();
        std::thread::spawn(move || handle_a.rebase_continue())
    };
    pause.wait_reached();
    assert!(!repo.path().join("browsitory/rebase.json").exists());
    // The file watcher in the other window triggers reads in this gap.
    handle_b.get_status().unwrap();
    handle_b.get_head_info().unwrap();
    pause.go();
    assert_eq!(
        continuing.join().unwrap().unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );

    let label = format!("Rebase onto {}", &onto.to_string()[..7]);
    assert_eq!(handle_a.get_undo_state().unwrap().undo, Some(label.clone()));
    assert_eq!(handle_b.get_undo_state().unwrap().undo, Some(label));
}

#[test]
fn a_merge_restarted_outside_with_the_same_heads_does_not_reuse_the_old_pending_entry() {
    let (dir, repo) = conflicting_branches();
    {
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let outcome = worker
            .handle()
            .start_merge(
                "feature".into(),
                git_core::merge::MergeOptions::default(),
                std::sync::mpsc::channel().0,
            )
            .unwrap();
        assert!(matches!(
            outcome,
            git_core::merge::MergeOutcome::Conflicted { .. }
        ));
    }
    // `git merge --abort` in a terminal while the app is closed...
    repo.cleanup_state().unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.reset(head.as_object(), git2::ResetType::Hard, None)
        .unwrap();
    // ...and the same merge started again there.
    let feature = repo.find_reference("refs/heads/feature").unwrap();
    let theirs = repo.reference_to_annotated_commit(&feature).unwrap();
    repo.merge(&[&theirs], None, None).unwrap();
    let merge_head = std::fs::File::options()
        .write(true)
        .open(repo.path().join("MERGE_HEAD"))
        .unwrap();
    merge_head
        .set_modified(std::time::SystemTime::now() + Duration::from_secs(2))
        .unwrap();
    drop(merge_head);

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .resolve_conflict("a.txt".into(), "resolved\n".into())
        .unwrap();
    handle
        .commit("Merge feature".into(), false, Vec::new(), mpsc::channel().0)
        .unwrap();
    assert_eq!(handle.get_undo_state().unwrap(), no_undo());
}

#[test]
fn a_merge_committed_outside_while_the_other_window_held_the_lock_is_not_recorded() {
    let (dir, repo) = conflicting_branches();
    let (worker, pause) = Worker::spawn_with_pause_before_finish(dir.path().to_path_buf());
    let handle = worker.handle();
    assert!(matches!(
        handle
            .start_merge(
                "feature".into(),
                git_core::merge::MergeOptions::default(),
                std::sync::mpsc::channel().0
            )
            .unwrap(),
        git_core::merge::MergeOutcome::Conflicted { .. }
    ));
    // `git commit` in a terminal ends the merge.
    write_file(dir.path(), "a.txt", "resolved\n");
    {
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let theirs = repo
            .find_reference("refs/heads/feature")
            .unwrap()
            .peel_to_commit()
            .unwrap();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Merge feature",
            &tree,
            &[&head, &theirs],
        )
        .unwrap();
        repo.cleanup_state().unwrap();
    }

    // The other window holds the lock while this one's file watcher reads the status: its
    // `settle` can't drop the stale entry, and the lock is free again by its `finish`.
    let guard = hold(&repo);
    pause.arm();
    let reading = {
        let handle = handle.clone();
        std::thread::spawn(move || handle.get_status())
    };
    pause.wait_reached();
    drop(guard);
    pause.go();
    reading.join().unwrap().unwrap();
    assert_eq!(handle.get_undo_state().unwrap(), no_undo());
}

/// A read while a merge is paused is tracked (an entry is pending) but doesn't hold its reply:
/// the reply arrives before the undo `finish`, so reads never wait for the "after" snapshot.
#[test]
fn a_read_during_a_paused_operation_replies_before_the_undo_finish() {
    let (dir, _repo) = conflicting_branches();
    let (worker, pause) = Worker::spawn_with_pause_before_finish(dir.path().to_path_buf());
    let handle = worker.handle();
    assert!(matches!(
        handle
            .start_merge(
                "feature".into(),
                git_core::merge::MergeOptions::default(),
                std::sync::mpsc::channel().0
            )
            .unwrap(),
        git_core::merge::MergeOutcome::Conflicted { .. }
    ));

    pause.arm();
    let (replied_tx, replied_rx) = mpsc::channel();
    let reading = {
        let handle = handle.clone();
        std::thread::spawn(move || {
            let _ = replied_tx.send(handle.get_status());
        })
    };
    pause.wait_reached();
    let replied = replied_rx.recv_timeout(Duration::from_secs(5));
    pause.go();
    reading.join().unwrap();
    assert!(
        replied
            .expect("the read's reply waited for the undo finish")
            .is_ok(),
        "the read failed"
    );
}

#[test]
fn a_locked_command_runs_after_the_wait_when_the_lock_stays_held() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "a");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    write_file(dir.path(), "a.txt", "edited\n");

    let _guard = hold(&repo);
    let start = Instant::now();
    handle.stage_file("a.txt".into()).unwrap();
    assert!(start.elapsed() >= LOCK_WAIT);
    let status = Repository::open(dir.path())
        .unwrap()
        .status_file(Path::new("a.txt"))
        .unwrap();
    assert!(status.contains(git2::Status::INDEX_MODIFIED));
    assert_eq!(handle.get_undo_state().unwrap(), no_undo());
}

#[test]
fn only_the_first_locked_command_waits_while_the_lock_stays_held() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "a");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    write_file(dir.path(), "a.txt", "edited\n");
    write_file(dir.path(), "b.txt", "edited\n");

    let guard = hold(&repo);
    handle.stage_file("a.txt".into()).unwrap();
    let start = Instant::now();
    handle.stage_file("b.txt".into()).unwrap();
    let elapsed = start.elapsed();
    assert!(elapsed >= CONTENDED_WAIT, "{elapsed:?}");
    assert!(elapsed < LOCK_WAIT, "{elapsed:?}");
    drop(guard);
    // Once the lock is free again, commands wait for it (and record) as usual.
    handle.unstage_file("b.txt".into()).unwrap();
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Unstage b.txt".into())
    );
}

#[test]
fn after_a_timeout_a_locked_command_waits_briefly_for_the_lock() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "a");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    write_file(dir.path(), "a.txt", "edited\n");
    write_file(dir.path(), "b.txt", "edited\n");

    let guard = hold(&repo);
    handle.stage_file("a.txt".into()).unwrap(); // times out after LOCK_WAIT
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        drop(guard);
    });
    handle.stage_file("b.txt".into()).unwrap();
    releaser.join().unwrap();
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Stage b.txt".into())
    );
}

/// `a.txt`…`h.txt` committed and edited, a worker on it, and the lock held by "the other
/// window"; `a.txt` and `b.txt` already staged, so the first `LOCK_WAIT` and the first
/// `CONTENDED_WAIT` are behind. Also how long staging six other files took before, with the
/// lock free (and so recorded: an upper bound for the same work unrecorded).
fn contended_worker() -> (TempDir, Worker, StateGuard, Duration) {
    let (dir, repo) = init_repo();
    let names = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let free = ["i", "j", "k", "l", "m", "n"];
    for name in names.iter().chain(&free) {
        write_file(dir.path(), &format!("{name}.txt"), "x\n");
    }
    commit_all(&repo, "files");
    for name in names.iter().chain(&free) {
        write_file(dir.path(), &format!("{name}.txt"), "edited\n");
    }
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let start = Instant::now();
    for name in free {
        handle.stage_file(format!("{name}.txt")).unwrap();
    }
    let baseline = start.elapsed();
    let guard = hold(&repo);
    handle.stage_file("a.txt".into()).unwrap(); // LOCK_WAIT
    handle.stage_file("b.txt".into()).unwrap(); // CONTENDED_WAIT
    (dir, worker, guard, baseline)
}

/// #197: while the lock keeps being found busy, a run of locked commands waits about
/// `CONTENDED_WAIT` once, not once per command, however long each takes (a slow runner).
#[test]
fn a_burst_of_locked_commands_waits_once_while_the_lock_stays_held() {
    let (dir, worker, _guard, baseline) = contended_worker();
    let handle = worker.handle();
    let start = Instant::now();
    for name in ["c", "d", "e", "f", "g", "h"] {
        handle.stage_file(format!("{name}.txt")).unwrap();
    }
    let elapsed = start.elapsed();
    // Waiting once per command would take at least `6 * CONTENDED_WAIT` on top.
    assert!(
        elapsed < baseline + 2 * CONTENDED_WAIT,
        "{elapsed:?} (baseline {baseline:?})"
    );
    let repo = Repository::open(dir.path()).unwrap();
    for name in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        let status = repo.status_file(Path::new(&format!("{name}.txt"))).unwrap();
        assert!(status.contains(git2::Status::INDEX_MODIFIED), "{name}");
    }
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Stage n.txt".into())
    );
}

/// #197: inside a burst, a command that would refuse (Undo, a discard) still waits
/// `CONTENDED_WAIT` for the lock before it refuses.
#[test]
fn inside_a_burst_a_refusing_command_still_waits() {
    let (dir, worker, _guard, _) = contended_worker();
    let handle = worker.handle();
    handle.stage_file("c.txt".into()).unwrap(); // in the burst: tries once
    let start = Instant::now();
    assert_eq!(
        handle.discard_file("d.txt".into()),
        Err(crate::wire::ErrorDto::from(DISCARD_BUSY))
    );
    let elapsed = start.elapsed();
    assert!(elapsed >= CONTENDED_WAIT, "{elapsed:?}");
    assert!(elapsed < LOCK_WAIT, "{elapsed:?}");
    let start = Instant::now();
    assert!(matches!(
        handle.undo(),
        Err(error) if error.message == UNDO_BUSY
    ));
    assert!(start.elapsed() >= CONTENDED_WAIT, "{:?}", start.elapsed());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("d.txt")).unwrap(),
        "edited\n"
    );
}

/// #197: after a quiet `CONTENDED_WAIT`, the next locked command waits for the lock again,
/// so a lone click still waits out a brief hold.
#[test]
fn after_a_quiet_moment_a_locked_command_waits_again() {
    let (_dir, worker, _guard, _) = contended_worker();
    let handle = worker.handle();
    handle.stage_file("c.txt".into()).unwrap(); // in the burst: tries once
    std::thread::sleep(CONTENDED_WAIT + Duration::from_millis(100));
    let start = Instant::now();
    handle.stage_file("d.txt".into()).unwrap();
    let elapsed = start.elapsed();
    assert!(elapsed >= CONTENDED_WAIT, "{elapsed:?}");
    assert!(elapsed < LOCK_WAIT, "{elapsed:?}");
}

#[test]
fn work_tree_destroying_commands_are_refused_while_the_lock_stays_held() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    let first = commit_all(&repo, "a");
    write_file(dir.path(), "a.txt", "b\n");
    commit_all(&repo, "b");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    write_file(dir.path(), "a.txt", "my edit\n");
    let edited = || std::fs::read_to_string(dir.path().join("a.txt")).unwrap();

    let guard = hold(&repo);
    let busy = Err(crate::wire::ErrorDto::from(DISCARD_BUSY));
    assert_eq!(handle.discard_file("a.txt".into()), busy);
    assert_eq!(handle.discard_all_unstaged(vec!["a.txt".into()]), busy);
    assert_eq!(
        handle.reset_to_commit(first.to_string(), git_core::reset::ResetMode::Hard),
        busy
    );
    assert_eq!(edited(), "my edit\n");
    // A mixed reset keeps the work tree, so it runs (unrecorded, D6-A).
    handle
        .reset_to_commit(first.to_string(), git_core::reset::ResetMode::Mixed)
        .unwrap();
    assert_eq!(edited(), "my edit\n");
    assert_eq!(handle.get_undo_state().unwrap(), no_undo());
    drop(guard);
    handle.discard_file("a.txt".into()).unwrap();
    assert_eq!(edited(), "a\n");
}

#[test]
fn discard_busy_copy_is_exact() {
    assert_eq!(
        DISCARD_BUSY,
        "Another Browsitory window is changing this repository's undo history. Try again in \
         a moment, so this change can be undone."
    );
}

#[test]
fn reads_never_wait_for_the_lock() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "a");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.get_status().unwrap(); // warm up

    let _guard = hold(&repo);
    let start = Instant::now();
    handle.get_status().unwrap();
    handle.get_undo_state().unwrap();
    assert!(start.elapsed() < Duration::from_millis(200));
}

#[test]
fn undo_replies_busy_while_another_holder_keeps_the_lock() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "a");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .create_branch("x".into(), "HEAD".into(), true)
        .unwrap();

    let guard = hold(&repo);
    assert_eq!(handle.undo(), Err(crate::wire::ErrorDto::from(UNDO_BUSY)));
    assert_eq!(handle.redo(), Err(crate::wire::ErrorDto::from(UNDO_BUSY)));
    assert!(repo.find_branch("x", git2::BranchType::Local).is_ok());
    drop(guard);
    assert_eq!(
        handle.undo().unwrap(),
        UndoOutcome::Done {
            label: "Create branch x".into()
        }
    );
}
