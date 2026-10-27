mod common;

use common::{commit_all, init_repo, write_file};
use git_core::stash::{StashError, StashOptions};

#[test]
fn save_stash_captures_tracked_and_untracked_changes_and_cleans_the_working_tree() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "v2");
    write_file(dir.path(), "untracked.txt", "new");

    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

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
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    write_file(dir.path(), "file.txt", "v3");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

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
    // Also cover an untracked file on the apply side — the save side already asserts it's
    // captured and removed (see the test above); this is the assertion that actually protects
    // "untracked files always included" on the round trip back.
    write_file(dir.path(), "untracked.txt", "new");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

    git_core::stash::apply_stash(&mut repo, 0).unwrap();

    let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(contents, "v2");
    let untracked_contents = std::fs::read_to_string(dir.path().join("untracked.txt")).unwrap();
    assert_eq!(untracked_contents, "new");
    // apply, not pop — the stash entry stays in the list.
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
}

#[test]
fn apply_stash_is_blocked_by_a_conflicting_dirty_file() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2-stashed");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
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
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    write_file(dir.path(), "file.txt", "v3");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    let surviving_stash_commit_id = git_core::stash::list_stashes(&mut repo).unwrap()[1]
        .commit_id
        .clone();

    git_core::stash::drop_stash(&mut repo, 0).unwrap();

    let stashes = git_core::stash::list_stashes(&mut repo).unwrap();
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].index, 0);
    assert_eq!(stashes[0].commit_id, surviving_stash_commit_id);
}

fn with(f: impl FnOnce(&mut StashOptions)) -> StashOptions {
    let mut opts = StashOptions::default();
    f(&mut opts);
    opts
}

fn read(dir: &std::path::Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path)).unwrap()
}

fn stage(repo: &git2::Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(path)).unwrap();
    index.write().unwrap();
}

fn stash_commit(repo: &mut git2::Repository, index: usize) -> git2::Oid {
    let id = git_core::stash::list_stashes(repo).unwrap()[index]
        .commit_id
        .clone();
    git2::Oid::from_str(&id).unwrap()
}

#[test]
fn stash_options_default_to_stash_everything_without_a_message() {
    assert_eq!(
        StashOptions::default(),
        StashOptions {
            message: None,
            keep_index: false,
            include_untracked: true,
            paths: vec![],
        }
    );
}

#[test]
fn stash_with_a_message_lists_it() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "v2");

    git_core::stash::save_stash(&mut repo, &with(|o| o.message = Some(" half done ".into())))
        .unwrap();

    let list = git_core::stash::list_stashes(&mut repo).unwrap();
    assert!(list[0].message.starts_with("On "), "{}", list[0].message);
    assert!(
        list[0].message.ends_with(": half done"),
        "{}",
        list[0].message
    );
}

#[test]
fn blank_message_uses_the_default_wip_message() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "v2");

    git_core::stash::save_stash(&mut repo, &with(|o| o.message = Some("   ".into()))).unwrap();

    let list = git_core::stash::list_stashes(&mut repo).unwrap();
    assert!(
        list[0].message.starts_with("WIP on "),
        "{}",
        list[0].message
    );
}

#[test]
fn keep_index_leaves_staged_changes_staged_and_stashes_them_too() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "staged");
    stage(&repo, "a.txt");

    git_core::stash::save_stash(&mut repo, &with(|o| o.keep_index = true)).unwrap();

    let status = git_core::status::status(&repo).unwrap();
    assert!(status.iter().any(|e| e.path == "a.txt" && e.staged));
    let stash_id = stash_commit(&mut repo, 0);
    let stash = repo.find_commit(stash_id).unwrap();
    let index_commit = stash.parent(1).unwrap();
    let blob = index_commit
        .tree()
        .unwrap()
        .get_path(std::path::Path::new("a.txt"))
        .unwrap();
    assert_eq!(repo.find_blob(blob.id()).unwrap().content(), b"staged");
}

#[test]
fn include_untracked_off_leaves_untracked_files() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "tracked.txt", "v2");
    write_file(dir.path(), "untracked.txt", "new");

    git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = false)).unwrap();

    assert_eq!(read(dir.path(), "tracked.txt"), "v1");
    assert_eq!(read(dir.path(), "untracked.txt"), "new");
    let stash_id = stash_commit(&mut repo, 0);
    let stash = repo.find_commit(stash_id).unwrap();
    assert_eq!(stash.parent_count(), 2, "no untracked-files parent");
}

#[test]
fn path_limited_stash_leaves_other_files() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    write_file(dir.path(), "b.txt", "v1");
    write_file(dir.path(), "s.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "mine");
    write_file(dir.path(), "b.txt", "other");
    write_file(dir.path(), "s.txt", "staged other");
    stage(&repo, "s.txt");
    write_file(dir.path(), "c.txt", "untracked");

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["a.txt".into()])).unwrap();

    assert_eq!(read(dir.path(), "a.txt"), "v1");
    assert_eq!(read(dir.path(), "b.txt"), "other");
    assert_eq!(read(dir.path(), "s.txt"), "staged other");
    assert_eq!(read(dir.path(), "c.txt"), "untracked");
    let status = git_core::status::status(&repo).unwrap();
    assert!(status.iter().any(|e| e.path == "s.txt" && e.staged));
    assert!(!status.iter().any(|e| e.path == "a.txt"));
    let stash_id = stash_commit(&mut repo, 0);
    let stash = repo.find_commit(stash_id).unwrap();
    let files = git_core::diff::commit_files(&repo, &stash.id().to_string()).unwrap();
    assert_eq!(files, vec!["a.txt".to_string()]);
}

#[test]
fn path_limited_stash_with_message_keeps_older_entries() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    write_file(dir.path(), "b.txt", "v1");
    commit_all(&repo, "initial");
    // Three older stashes, one of them with its own message.
    for (round, message) in [("one", None), ("two", Some("second")), ("three", None)] {
        write_file(dir.path(), "b.txt", round);
        git_core::stash::save_stash(&mut repo, &with(|o| o.message = message.map(Into::into)))
            .unwrap();
    }
    let older = git_core::stash::list_stashes(&mut repo).unwrap();
    assert_eq!(older.len(), 3);
    write_file(dir.path(), "a.txt", "mine");
    write_file(dir.path(), "b.txt", "keep");

    git_core::stash::save_stash(
        &mut repo,
        &with(|o| {
            o.message = Some("only a".into());
            o.paths = vec!["a.txt".into()];
        }),
    )
    .unwrap();

    let list = git_core::stash::list_stashes(&mut repo).unwrap();
    assert_eq!(list.len(), 4);
    assert!(list[0].message.starts_with("On ") && list[0].message.ends_with(": only a"));
    for (offset, entry) in older.iter().enumerate() {
        assert_eq!(list[offset + 1].index, offset + 1);
        assert_eq!(list[offset + 1].commit_id, entry.commit_id);
        assert_eq!(list[offset + 1].message, entry.message);
    }
    {
        let top = repo
            .find_commit(git2::Oid::from_str(&list[0].commit_id).unwrap())
            .unwrap();
        assert_eq!(top.message().unwrap(), format!("{}\n", list[0].message));
    }
    assert_eq!(read(dir.path(), "b.txt"), "keep");
    // The reflog chain stays consistent: each entry's old id is the next entry's new id.
    {
        let reflog = repo.reflog("refs/stash").unwrap();
        for i in 0..reflog.len() - 1 {
            assert_eq!(
                reflog.get(i).unwrap().id_old(),
                reflog.get(i + 1).unwrap().id_new()
            );
        }
    }
    assert_eq!(
        repo.refname_to_id("refs/stash").unwrap().to_string(),
        list[0].commit_id
    );

    // The restamped stash applies; the older ones are still intact underneath it.
    write_file(dir.path(), "b.txt", "v1");
    git_core::stash::apply_stash(&mut repo, 0).unwrap();
    assert_eq!(read(dir.path(), "a.txt"), "mine");
    git_core::stash::drop_stash(&mut repo, 0).unwrap();
    write_file(dir.path(), "a.txt", "v1");
    let after_drop = git_core::stash::list_stashes(&mut repo).unwrap();
    assert_eq!(after_drop, older);
    git_core::stash::apply_stash(&mut repo, 1).unwrap();
    assert_eq!(read(dir.path(), "b.txt"), "two");
}

#[test]
fn nothing_to_stash_is_a_clear_error() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");

    let error = git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap_err();

    assert!(matches!(error, StashError::NothingToStash), "{error:?}");
    assert_eq!(error.to_string(), "There are no changes to stash.");
}

#[test]
fn path_limited_stash_of_an_unchanged_file_is_nothing_to_stash() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    write_file(dir.path(), "b.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "b.txt", "changed");

    let error = git_core::stash::save_stash(
        &mut repo,
        &with(|o| {
            o.paths = vec!["a.txt".into()];
            o.message = Some("x".into());
        }),
    )
    .unwrap_err();

    assert!(matches!(error, StashError::NothingToStash), "{error:?}");
    assert_eq!(read(dir.path(), "b.txt"), "changed");
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
}

#[test]
fn path_limited_stash_of_an_untracked_file_stashes_only_it_and_applies_back() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "nested/dir/new.txt", "brand new\n");
    write_file(dir.path(), "other.txt", "other untracked");

    git_core::stash::save_stash(
        &mut repo,
        &with(|o| o.paths = vec!["nested/dir/new.txt".into()]),
    )
    .unwrap();

    assert!(!dir.path().join("nested").exists(), "emptied dirs removed");
    assert_eq!(read(dir.path(), "other.txt"), "other untracked");
    let stash_id = stash_commit(&mut repo, 0);
    {
        let stash = repo.find_commit(stash_id).unwrap();
        assert_eq!(stash.parent_count(), 3);
        let untracked = stash.parent(2).unwrap();
        assert_eq!(untracked.parent_count(), 0);
        let files = git_core::diff::commit_files(&repo, &untracked.id().to_string()).unwrap();
        assert_eq!(files, vec!["nested/dir/new.txt".to_string()]);
    }
    git_core::stash::apply_stash(&mut repo, 0).unwrap();
    assert_eq!(read(dir.path(), "nested/dir/new.txt"), "brand new\n");
}

#[test]
fn path_limited_stash_of_a_staged_new_file_unstages_and_removes_only_it() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "added.txt", "added\n");
    stage(&repo, "added.txt");
    write_file(dir.path(), "a.txt", "edited");

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["added.txt".into()])).unwrap();

    assert!(!dir.path().join("added.txt").exists());
    assert_eq!(read(dir.path(), "a.txt"), "edited");
    let status = git_core::status::status(&repo).unwrap();
    assert_eq!(status.len(), 1, "{status:?}");
    assert_eq!(status[0].path, "a.txt");
    git_core::stash::apply_stash(&mut repo, 0).unwrap();
    assert_eq!(read(dir.path(), "added.txt"), "added\n");
}

#[test]
fn path_limited_stash_with_keep_index_keeps_the_staged_version() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    write_file(dir.path(), "b.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "staged");
    stage(&repo, "a.txt");
    write_file(dir.path(), "a.txt", "staged then edited");
    write_file(dir.path(), "b.txt", "other");

    git_core::stash::save_stash(
        &mut repo,
        &with(|o| {
            o.paths = vec!["a.txt".into()];
            o.keep_index = true;
        }),
    )
    .unwrap();

    assert_eq!(read(dir.path(), "a.txt"), "staged");
    assert_eq!(read(dir.path(), "b.txt"), "other");
    let status = git_core::status::status(&repo).unwrap();
    assert!(status.iter().any(|e| e.path == "a.txt" && e.staged));
    assert!(!status.iter().any(|e| e.path == "a.txt" && !e.staged));
}

#[test]
fn path_limited_stash_without_a_message_uses_the_wip_format() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    let short = repo.head().unwrap().target().unwrap().to_string()[..7].to_string();
    write_file(dir.path(), "a.txt", "v2");

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["a.txt".into()])).unwrap();

    let entry = git_core::stash::list_stashes(&mut repo).unwrap()[0].clone();
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    assert_eq!(entry.message, format!("WIP on {branch}: {short} initial"));
}

#[test]
fn path_limited_stash_paths_are_literal() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    write_file(dir.path(), "[ab].txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "keep");
    write_file(dir.path(), "[ab].txt", "stash me");

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["[ab].txt".into()])).unwrap();

    assert_eq!(read(dir.path(), "[ab].txt"), "v1");
    assert_eq!(read(dir.path(), "a.txt"), "keep");
}
