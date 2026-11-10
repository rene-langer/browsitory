//! Through the worker: stash and switch as one undo step, creating a branch without switching,
//! the commit identity and the pull strategy from git config (#652, #653, #656, #689).
use super::*;

fn current(repo_path: &Path) -> String {
    Repository::open(repo_path)
        .unwrap()
        .head()
        .unwrap()
        .shorthand()
        .unwrap()
        .to_string()
}

/// `other` changes `file.txt`; HEAD on the first branch, whose name is returned.
fn two_branches() -> (TempDir, Repository, String) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "base\n");
    commit_all(&repo, "base");
    let main = current(dir.path());
    git_core::branch::create_branch(&repo, "other", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "other\n");
    commit_all(&repo, "other");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    (dir, repo, main)
}

#[test]
fn a_switch_over_local_changes_is_local_changes_and_names_the_file() {
    let (dir, _repo, _main) = two_branches();
    write_file(dir.path(), "file.txt", "mine\n");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let error = handle
        .switch_branch("other".into(), no_events())
        .unwrap_err();

    assert_eq!(error.kind, crate::wire::ErrorKind::LocalChanges);
    assert_eq!(
        handle.switch_conflicts("other".into()).unwrap(),
        vec!["file.txt".to_string()]
    );
}

#[test]
fn stash_and_switch_is_one_undo_step_back_to_the_changes() {
    let (dir, _repo, main) = two_branches();
    write_file(dir.path(), "file.txt", "mine\n");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let outcome = handle
        .stash_and_switch("other".into(), false, no_events())
        .unwrap();

    assert_eq!(outcome, git_core::branch::StashSwitchOutcome::Stashed);
    assert_eq!(current(dir.path()), "other");
    assert_eq!(
        handle.get_undo_state().unwrap().undo.as_deref(),
        Some("Switch to other with local changes")
    );
    assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(current(dir.path()), main);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("file.txt")).unwrap(),
        "mine\n"
    );
}

#[test]
fn stash_and_switch_reapplies_changes_that_fit() {
    let (dir, _repo, _main) = two_branches();
    write_file(dir.path(), "notes.txt", "untracked\n");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let outcome = handle
        .stash_and_switch("other".into(), true, no_events())
        .unwrap();

    assert_eq!(outcome, git_core::branch::StashSwitchOutcome::Reapplied);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes.txt")).unwrap(),
        "untracked\n"
    );
    assert!(handle.list_stashes().unwrap().is_empty());
}

#[test]
fn create_branch_without_checkout_stays_on_the_current_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let main = current(dir.path());
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    handle
        .create_branch("feature".into(), "HEAD".into(), false)
        .unwrap();

    assert_eq!(current(dir.path()), main);
    assert!(handle
        .list_branches()
        .unwrap()
        .iter()
        .any(|branch| branch.name == "feature" && !branch.is_current));
}

#[test]
fn a_commit_without_identity_is_missing_identity_until_set_identity() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "first");
    {
        let mut local = repo
            .config()
            .unwrap()
            .open_level(git2::ConfigLevel::Local)
            .unwrap();
        local.remove("user.name").unwrap();
        local.remove("user.email").unwrap();
    }
    write_file(dir.path(), "a.txt", "b\n");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("a.txt".into()).unwrap();

    let error = handle
        .commit("second".into(), false, vec![], no_events())
        .unwrap_err();
    assert_eq!(error.kind, crate::wire::ErrorKind::MissingIdentity);

    handle
        .set_identity("Ada".into(), "ada@example.com".into(), false)
        .unwrap();
    handle
        .commit("second".into(), false, vec![], no_events())
        .unwrap();
    let reopened = Repository::open(dir.path()).unwrap();
    let head = reopened.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.author().email().unwrap(), "ada@example.com");
}

#[test]
fn pull_defaults_reads_the_repositorys_git_config() {
    let (dir, repo) = init_repo();
    repo.config()
        .unwrap()
        .set_str("pull.rebase", "true")
        .unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    assert_eq!(
        handle.pull_defaults().unwrap(),
        Some(git_core::remote::PullStrategy::Rebase)
    );
}
