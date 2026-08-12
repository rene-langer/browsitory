mod common;

use common::{commit_all, init_repo, write_file};

#[test]
fn save_stash_captures_tracked_and_untracked_changes_and_cleans_the_working_tree() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "v2");
    write_file(dir.path(), "untracked.txt", "new");

    git_core::stash::save_stash(&mut repo).unwrap();

    assert!(git_core::status::status(&repo).unwrap().is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("tracked.txt")).unwrap(),
        "v1"
    );
    assert!(!dir.path().join("untracked.txt").exists());
}

#[test]
fn list_stashes_reports_the_most_recently_saved_stash_first() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2");
    git_core::stash::save_stash(&mut repo).unwrap();
    write_file(dir.path(), "file.txt", "v3");
    git_core::stash::save_stash(&mut repo).unwrap();

    let stashes = git_core::stash::list_stashes(&mut repo).unwrap();

    assert_eq!(stashes.len(), 2);
    assert_eq!(stashes[0].index, 0);
    assert_eq!(stashes[1].index, 1);
    assert!(!stashes[0].message.is_empty());
    assert!(!stashes[0].commit_id.is_empty());
    assert_ne!(stashes[0].commit_id, stashes[1].commit_id);
}

#[test]
fn apply_stash_restores_the_stashed_changes() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2");
    git_core::stash::save_stash(&mut repo).unwrap();

    git_core::stash::apply_stash(&mut repo, 0).unwrap();

    let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(contents, "v2");
    // apply, not pop — the stash entry stays in the list.
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
}

#[test]
fn apply_stash_is_blocked_by_a_conflicting_dirty_file() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2-stashed");
    git_core::stash::save_stash(&mut repo).unwrap();
    // Working tree is back to "v1" after the stash; dirty it with content that conflicts with
    // what applying the stash would restore.
    write_file(dir.path(), "file.txt", "uncommitted local edit");

    let result = git_core::stash::apply_stash(&mut repo, 0);

    assert!(result.is_err());
    let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(contents, "uncommitted local edit");
    // The blocked apply must not have dropped the stash either.
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
}

#[test]
fn apply_stash_on_an_empty_list_returns_an_error() {
    let (_dir, mut repo) = init_repo();

    let result = git_core::stash::apply_stash(&mut repo, 0);

    assert!(result.is_err());
}

#[test]
fn drop_stash_removes_the_entry_and_shifts_remaining_indices() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2");
    git_core::stash::save_stash(&mut repo).unwrap();
    write_file(dir.path(), "file.txt", "v3");
    git_core::stash::save_stash(&mut repo).unwrap();
    let second_stash_commit_id = git_core::stash::list_stashes(&mut repo).unwrap()[1]
        .commit_id
        .clone();

    git_core::stash::drop_stash(&mut repo, 0).unwrap();

    let stashes = git_core::stash::list_stashes(&mut repo).unwrap();
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].index, 0);
    assert_eq!(stashes[0].commit_id, second_stash_commit_id);
}
