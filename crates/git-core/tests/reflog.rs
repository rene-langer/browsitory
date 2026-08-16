#[allow(dead_code)]
mod common;

use common::{commit_all, init_repo, write_file};
use git_core::reflog::{list_reflog_refs, read_reflog, restore_reflog_entry};

#[test]
fn lists_local_reflogs_newest_first_and_restores_only_the_selected_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "history.txt", "first");
    commit_all(&repo, "first commit");
    let first = repo.head().unwrap().target().unwrap();

    write_file(dir.path(), "history.txt", "second");
    commit_all(&repo, "second commit");
    let second = repo.head().unwrap().target().unwrap();

    repo.branch("recovery", &repo.find_commit(first).unwrap(), false)
        .unwrap();
    repo.find_reference("refs/heads/recovery")
        .unwrap()
        .set_target(second, "advance recovery")
        .unwrap();
    repo.reference(
        "refs/remotes/origin/main",
        second,
        true,
        "create remote tracking ref",
    )
    .unwrap();

    let refs = list_reflog_refs(&repo).unwrap();
    assert!(refs.contains(&"HEAD".to_string()));
    assert!(refs.contains(&"refs/heads/recovery".to_string()));
    assert!(!refs.contains(&"refs/remotes/origin/main".to_string()));

    let entries = read_reflog(&repo, "refs/heads/recovery").unwrap();
    assert_eq!(entries[0].reference, "refs/heads/recovery");
    assert_eq!(entries[0].old_id, first.to_string());
    assert_eq!(entries[0].new_id, second.to_string());
    assert_eq!(entries[0].summary.as_deref(), Some("second commit"));
    assert_eq!(entries[0].committer_name, "Test User");
    assert_eq!(entries[0].committer_email, "test@example.com");
    assert_eq!(entries[0].message, "advance recovery");
    assert_eq!(entries[1].new_id, first.to_string());

    restore_reflog_entry(&repo, "refs/heads/recovery", &first.to_string()).unwrap();

    assert_eq!(
        repo.find_reference("refs/heads/recovery").unwrap().target(),
        Some(first)
    );
    assert_eq!(repo.head().unwrap().target(), Some(second));
    assert_eq!(
        read_reflog(&repo, "refs/heads/recovery").unwrap()[0].message,
        "browsitory: restore reflog entry"
    );
}
