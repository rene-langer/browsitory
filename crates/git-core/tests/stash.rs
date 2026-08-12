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
