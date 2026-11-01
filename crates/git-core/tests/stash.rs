mod common;

use common::{commit_all, init_repo, write_file};
use git_core::stash::{StashApplyOutcome, StashError, StashOptions};

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

    assert_eq!(
        git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap(),
        StashApplyOutcome::Applied
    );

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

    let result = git_core::stash::apply_stash(&mut repo, 0, None, false);

    assert!(
        matches!(result, Err(StashError::WouldOverwrite)),
        "{result:?}"
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "Applying this stash would overwrite local changes to the same files. Commit, stash or discard them first."
    );
    let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(contents, "uncommitted local edit");
    // The blocked apply must not have dropped the stash either.
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
}

#[test]
fn apply_stash_on_an_empty_list_returns_an_error() {
    let (_dir, mut repo) = init_repo();

    let result = git_core::stash::apply_stash(&mut repo, 0, None, false);

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

    git_core::stash::drop_stash(&mut repo, 0, None).unwrap();

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
    assert_eq!(
        git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap(),
        StashApplyOutcome::Applied
    );
    assert_eq!(read(dir.path(), "a.txt"), "mine");
    git_core::stash::drop_stash(&mut repo, 0, None).unwrap();
    write_file(dir.path(), "a.txt", "v1");
    let after_drop = git_core::stash::list_stashes(&mut repo).unwrap();
    assert_eq!(after_drop, older);
    git_core::stash::apply_stash(&mut repo, 1, None, false).unwrap();
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
    assert_eq!(
        git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap(),
        StashApplyOutcome::Applied
    );
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
    assert_eq!(
        git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap(),
        StashApplyOutcome::Applied
    );
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

/// file.txt stashed as "stashed\n", then a conflicting commit changed it to "committed\n".
fn conflicting_stash() -> (tempfile::TempDir, git2::Repository) {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "file.txt", "base\n");
    commit_all(&repo, "initial");
    write_file(dir.path(), "file.txt", "stashed\n");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    write_file(dir.path(), "file.txt", "committed\n");
    commit_all(&repo, "conflicting commit");
    (dir, repo)
}

#[test]
fn pop_drops_the_stash_on_success() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "v2");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

    let outcome = git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap();

    assert_eq!(outcome, StashApplyOutcome::Applied);
    assert_eq!(read(dir.path(), "a.txt"), "v2");
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
}

#[test]
fn pop_drops_only_the_popped_stash() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    for round in ["one", "two", "three"] {
        write_file(dir.path(), "a.txt", round);
        git_core::stash::save_stash(&mut repo, &with(|o| o.message = Some(round.into()))).unwrap();
    }
    let before = git_core::stash::list_stashes(&mut repo).unwrap();

    git_core::stash::pop_stash(&mut repo, 1, None, false).unwrap();

    assert_eq!(read(dir.path(), "a.txt"), "two");
    let after = git_core::stash::list_stashes(&mut repo).unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].commit_id, before[0].commit_id);
    assert_eq!(after[1].commit_id, before[2].commit_id);
}

#[test]
fn pop_keeps_the_stash_and_reports_conflicts() {
    let (_dir, mut repo) = conflicting_stash();

    let outcome = git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap();

    assert_eq!(
        outcome,
        StashApplyOutcome::Conflicted {
            files: vec!["file.txt".into()]
        }
    );
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
    assert!(repo.index().unwrap().has_conflicts());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn apply_with_unresolved_conflicts_is_refused() {
    let (_dir, mut repo) = conflicting_stash();
    git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap();

    let error = git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap_err();

    assert!(
        matches!(error, StashError::UnresolvedConflicts),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "Resolve the current conflicts before applying a stash."
    );
    let error = git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap_err();
    assert!(
        matches!(error, StashError::UnresolvedConflicts),
        "{error:?}"
    );
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
}

#[test]
fn apply_with_restore_index_restores_staged_state() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    write_file(dir.path(), "b.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "staged");
    stage(&repo, "a.txt");
    write_file(dir.path(), "b.txt", "unstaged");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

    let outcome = git_core::stash::apply_stash(&mut repo, 0, None, true).unwrap();

    assert_eq!(outcome, StashApplyOutcome::Applied);
    let status = git_core::status::status(&repo).unwrap();
    assert!(status.iter().any(|e| e.path == "a.txt" && e.staged));
    assert!(status.iter().any(|e| e.path == "b.txt" && !e.staged));
    assert!(!status.iter().any(|e| e.path == "b.txt" && e.staged));

    // Without restoring the index, a.txt comes back unstaged.
    let head = repo.head().unwrap().target().unwrap().to_string();
    git_core::reset::reset_to(&repo, &head, git_core::reset::ResetMode::Hard).unwrap();
    git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap();
    let status = git_core::status::status(&repo).unwrap();
    assert!(!status.iter().any(|e| e.staged), "{status:?}");
    assert_eq!(read(dir.path(), "a.txt"), "staged");
}

#[test]
fn apply_with_staged_changes_reports_staged_changes() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    write_file(dir.path(), "b.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "v2");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    write_file(dir.path(), "b.txt", "staged");
    stage(&repo, "b.txt");

    let error = git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap_err();

    assert!(matches!(error, StashError::StagedChanges), "{error:?}");
    assert_eq!(
        error.to_string(),
        "Commit or unstage your staged changes before applying a stash."
    );
    let error = git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap_err();
    assert!(matches!(error, StashError::StagedChanges), "{error:?}");
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
    assert_eq!(read(dir.path(), "a.txt"), "v1");
}

#[test]
fn stash_files_lists_tracked_and_untracked_files() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "tracked.txt", "v2");
    write_file(dir.path(), "new.txt", "brand new\n");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    let id = git_core::stash::list_stashes(&mut repo).unwrap()[0]
        .commit_id
        .clone();

    let files = git_core::stash::stash_files(&mut repo, &id).unwrap();

    assert_eq!(files.tracked, vec!["tracked.txt".to_string()]);
    assert_eq!(files.untracked, vec!["new.txt".to_string()]);
    let untracked_id = files.untracked_commit_id.unwrap();
    let hunks =
        git_core::diff::commit_diff(&repo, &untracked_id, "new.txt", &Default::default()).unwrap();
    assert_eq!(hunks[0].old_lines, 0);
    assert_eq!(hunks[0].lines[0].content, "brand new\n");
}

#[test]
fn stash_files_without_untracked_has_no_untracked_commit() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "tracked.txt", "v2");
    git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = false)).unwrap();
    let id = git_core::stash::list_stashes(&mut repo).unwrap()[0]
        .commit_id
        .clone();

    let files = git_core::stash::stash_files(&mut repo, &id).unwrap();

    assert_eq!(files.tracked, vec!["tracked.txt".to_string()]);
    assert!(files.untracked.is_empty());
    assert_eq!(files.untracked_commit_id, None);
}

#[test]
fn stash_files_rejects_a_commit_that_is_not_a_stash() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    let head = repo.head().unwrap().target().unwrap().to_string();

    let error = git_core::stash::stash_files(&mut repo, &head).unwrap_err();

    assert!(
        matches!(&error, StashError::NotAStash(id) if *id == head),
        "{error:?}"
    );
    assert_eq!(error.to_string(), format!("No stash with id {head}."));
}

#[test]
#[cfg(unix)]
fn path_limited_stash_of_a_file_replaced_by_a_symlink_never_writes_through_it() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tc", "plain\n");
    write_file(dir.path(), "other", "o1\n");
    commit_all(&repo, "initial");
    write_file(dir.path(), "other", "precious unstaged edit\n");
    std::fs::remove_file(dir.path().join("tc")).unwrap();
    std::os::unix::fs::symlink("other", dir.path().join("tc")).unwrap();

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["tc".into()])).unwrap();

    assert_eq!(read(dir.path(), "other"), "precious unstaged edit\n");
    let meta = std::fs::symlink_metadata(dir.path().join("tc")).unwrap();
    assert!(meta.file_type().is_file(), "tc is a regular file again");
    assert_eq!(read(dir.path(), "tc"), "plain\n");

    let outcome = git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap();
    assert_eq!(outcome, StashApplyOutcome::Applied);
    let meta = std::fs::symlink_metadata(dir.path().join("tc")).unwrap();
    assert!(meta.file_type().is_symlink(), "pop restores the symlink");
    assert_eq!(
        std::fs::read_link(dir.path().join("tc")).unwrap(),
        std::path::PathBuf::from("other")
    );
    assert_eq!(read(dir.path(), "other"), "precious unstaged edit\n");
}

#[test]
#[cfg(unix)]
fn path_limited_stash_of_a_symlink_to_an_outside_file_leaves_that_file_alone() {
    let outside = tempfile::TempDir::new().unwrap();
    let target = outside.path().join("outside.txt");
    std::fs::write(&target, "outside\n").unwrap();
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tc", "plain\n");
    commit_all(&repo, "initial");
    std::fs::remove_file(dir.path().join("tc")).unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join("tc")).unwrap();

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["tc".into()])).unwrap();

    assert_eq!(std::fs::read_to_string(&target).unwrap(), "outside\n");
    assert!(std::fs::symlink_metadata(dir.path().join("tc"))
        .unwrap()
        .file_type()
        .is_file());
    assert_eq!(read(dir.path(), "tc"), "plain\n");
}

#[test]
#[cfg(unix)]
fn path_limited_stash_refuses_a_path_beyond_a_symlinked_directory() {
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("f"), "outside\n").unwrap();
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "d/f", "inside\n");
    commit_all(&repo, "initial");
    std::fs::remove_dir_all(dir.path().join("d")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("d")).unwrap();

    let error = git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["d/f".into()]))
        .unwrap_err();

    assert!(
        matches!(error, StashError::BeyondSymlink(ref p) if p == "d/f"),
        "{error:?}"
    );
    assert_eq!(error.to_string(), "'d/f' is beyond a symbolic link.");
    assert_eq!(
        std::fs::read_to_string(outside.path().join("f")).unwrap(),
        "outside\n"
    );
    assert!(std::fs::symlink_metadata(dir.path().join("d"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
}

#[test]
fn restoring_staged_changes_of_a_path_limited_stash_touches_only_its_files() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a", "a1\n");
    write_file(dir.path(), "c", "c1\n");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a", "a2\n");
    write_file(dir.path(), "c", "c2\n");
    stage(&repo, "c");

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["a".into()])).unwrap();
    // The stash's index commit holds nothing of `c`.
    let stash_id = stash_commit(&mut repo, 0);
    {
        let stash = repo.find_commit(stash_id).unwrap();
        let index_tree = stash.parent(1).unwrap().tree().unwrap();
        let head_tree = repo.head().unwrap().peel_to_tree().unwrap();
        let c = |tree: &git2::Tree| tree.get_path(std::path::Path::new("c")).unwrap().id();
        assert_eq!(c(&index_tree), c(&head_tree));
    }
    // Discard `c` back to c1 in index and worktree.
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force().path("c");
    repo.checkout_head(Some(&mut checkout)).unwrap();
    assert!(git_core::status::status(&repo).unwrap().is_empty());

    git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap();

    let status = git_core::status::status(&repo).unwrap();
    assert!(!status.iter().any(|e| e.path == "c"), "{status:?}");
    assert_eq!(read(dir.path(), "a"), "a2\n");
    assert_eq!(read(dir.path(), "c"), "c1\n");
}

#[test]
fn git_stash_apply_index_restores_the_selected_files_of_a_path_limited_stash() {
    if !common::program_available("git") {
        return;
    }
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a", "a1\n");
    write_file(dir.path(), "c", "c1\n");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a", "a2\n");
    stage(&repo, "a");
    write_file(dir.path(), "a", "a3\n");
    write_file(dir.path(), "c", "c2\n");
    stage(&repo, "c");

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["a".into()])).unwrap();
    // Unstage c so git's own apply --index has a clean index to work on.
    let head = repo.head().unwrap().target().unwrap().to_string();
    git_core::reset::reset_to(&repo, &head, git_core::reset::ResetMode::Mixed).unwrap();

    let output = std::process::Command::new("git")
        .args(["stash", "apply", "--index"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let staged_a = std::process::Command::new("git")
        .args(["show", ":a"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&staged_a.stdout), "a2\n");
    assert_eq!(read(dir.path(), "a"), "a3\n");
}

#[test]
fn apply_pop_and_drop_refuse_when_the_stash_list_changed() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "v2");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    let old_top = git_core::stash::list_stashes(&mut repo).unwrap()[0]
        .commit_id
        .clone();
    write_file(dir.path(), "a.txt", "v3");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

    for result in [
        git_core::stash::apply_stash(&mut repo, 0, Some(&old_top), false).map(|_| ()),
        git_core::stash::pop_stash(&mut repo, 0, Some(&old_top), false).map(|_| ()),
        git_core::stash::drop_stash(&mut repo, 0, Some(&old_top)),
    ] {
        let error = result.unwrap_err();
        assert!(matches!(error, StashError::StashListChanged), "{error:?}");
        assert_eq!(
            error.to_string(),
            "The stash list changed; refresh and try again."
        );
    }
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 2);
    assert_eq!(read(dir.path(), "a.txt"), "v1");

    git_core::stash::drop_stash(&mut repo, 1, Some(&old_top)).unwrap();
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
}

/// HEAD has `l` as a symlink to `target`; the worktree replaced `l` by a real directory holding
/// an untracked `l/f` (status ` D l`, `?? l/f`).
#[cfg(unix)]
fn symlink_replaced_by_directory(
    target: &std::path::Path,
) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "keep.txt", "k\n");
    std::os::unix::fs::symlink(target, dir.path().join("l")).unwrap();
    commit_all(&repo, "initial");
    std::fs::remove_file(dir.path().join("l")).unwrap();
    write_file(dir.path(), "l/f", "mine\n");
    (dir, repo)
}

/// Asserts a stash of `l` was refused because `l` went from symlink to directory, and that
/// nothing was written: no stash, `l` still a directory holding `l/f`, the status unchanged.
#[cfg(unix)]
fn assert_directory_over_symlink_refused(
    repo: &mut git2::Repository,
    dir: &std::path::Path,
    result: Result<(), StashError>,
    status_before: &[git_core::status::StatusEntry],
) {
    let error = result.unwrap_err();
    assert!(
        matches!(error, StashError::DirectoryReplacedNonDirectory { ref path, .. } if path == "l"),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "Can't stash 'l': it was a symbolic link and is now a directory. Commit or move its contents first."
    );
    assert!(git_core::stash::list_stashes(repo).unwrap().is_empty());
    assert!(repo.find_reference("refs/stash").is_err());
    assert!(std::fs::symlink_metadata(dir.join("l")).unwrap().is_dir());
    assert_eq!(read(dir, "l/f"), "mine\n");
    assert_eq!(git_core::status::status(repo).unwrap(), status_before);
}

#[cfg(unix)]
#[test]
fn stashing_a_directory_that_replaced_a_symlink_is_refused_outside_target() {
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("f"), "outside, unsaved\n").unwrap();
    let (dir, mut repo) = symlink_replaced_by_directory(outside.path());
    let status_before = git_core::status::status(&repo).unwrap();

    let result = git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["l".into()]));

    assert_directory_over_symlink_refused(&mut repo, dir.path(), result, &status_before);
    assert_eq!(
        std::fs::read_to_string(outside.path().join("f")).unwrap(),
        "outside, unsaved\n"
    );
}

#[cfg(unix)]
#[test]
fn stashing_a_directory_that_replaced_a_symlink_is_refused_in_repo_target() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "real/f", "committed\n");
    std::os::unix::fs::symlink("real", dir.path().join("l")).unwrap();
    commit_all(&repo, "initial");
    write_file(dir.path(), "real/f", "unsaved edits\n");
    std::fs::remove_file(dir.path().join("l")).unwrap();
    write_file(dir.path(), "l/f", "mine\n");
    let status_before = git_core::status::status(&repo).unwrap();

    let result = git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["l".into()]));

    assert_directory_over_symlink_refused(&mut repo, dir.path(), result, &status_before);
    assert_eq!(read(dir.path(), "real/f"), "unsaved edits\n");
}

#[cfg(unix)]
#[test]
fn stashing_a_directory_over_a_symlink_without_untracked_files_is_refused() {
    // U1: with untracked files left out, the checkout would delete `l/f` for good.
    let outside = tempfile::TempDir::new().unwrap();
    let (dir, mut repo) = symlink_replaced_by_directory(outside.path());
    let status_before = git_core::status::status(&repo).unwrap();

    let result = git_core::stash::save_stash(
        &mut repo,
        &with(|o| {
            o.paths = vec!["l".into()];
            o.include_untracked = false;
        }),
    );

    assert_directory_over_symlink_refused(&mut repo, dir.path(), result, &status_before);
}

#[cfg(unix)]
#[test]
fn stashing_a_directory_over_a_symlink_holding_ignored_files_is_refused() {
    // I1: ignored files are never stashed, so the checkout would delete `l/build.log` for good.
    let outside = tempfile::TempDir::new().unwrap();
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), ".gitignore", "l/*.log\n");
    std::os::unix::fs::symlink(outside.path(), dir.path().join("l")).unwrap();
    commit_all(&repo, "initial");
    std::fs::remove_file(dir.path().join("l")).unwrap();
    write_file(dir.path(), "l/f", "mine\n");
    write_file(dir.path(), "l/build.log", "ignored output\n");
    let status_before = git_core::status::status(&repo).unwrap();

    let result = git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["l".into()]));

    assert_directory_over_symlink_refused(&mut repo, dir.path(), result, &status_before);
    assert_eq!(read(dir.path(), "l/build.log"), "ignored output\n");
}

#[cfg(unix)]
#[test]
fn popping_a_file_stashed_from_a_directory_that_replaced_a_symlink_is_a_clear_error() {
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("f"), "outside\n").unwrap();
    let (dir, mut repo) = symlink_replaced_by_directory(outside.path());

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["l/f".into()])).unwrap();
    assert_eq!(
        std::fs::read_to_string(outside.path().join("f")).unwrap(),
        "outside\n"
    );
    let status_before = git_core::status::status(&repo).unwrap();

    let error = git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap_err();

    // The untracked `l/f` needs a folder where the index has the symlink `l` (#206): refused
    // before libgit2 runs (its apply failed with "null OID", `SymlinkReplacedByDirectory`).
    assert!(
        matches!(
            error,
            StashError::UntrackedBelowTrackedFile { ref untracked, ref file }
                if untracked == "l/f" && file == "l"
        ),
        "{error:?}"
    );
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
    assert_eq!(
        std::fs::read_to_string(outside.path().join("f")).unwrap(),
        "outside\n"
    );
    assert_eq!(git_core::status::status(&repo).unwrap(), status_before);
    assert!(!dir.path().join("l").exists());
}

/// `tc` = "plain\n", `a.txt` = "a\n" and `d/f` = "inner\n" committed; `a.txt` edited and `tc`
/// replaced by a link to the outside directory's `target.txt`.
#[cfg(unix)]
fn typechanged_link_repo() -> (tempfile::TempDir, git2::Repository, tempfile::TempDir) {
    let out = common::outside_dir();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tc", "plain\n");
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "d/f", "inner\n");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "edited\n");
    common::replace_with_link(dir.path(), "tc", &out.path().join("target.txt"));
    (dir, repo, out)
}

#[test]
#[cfg(unix)]
fn stash_all_with_a_typechanged_symlink_stashes_the_link_and_restores_it_on_pop() {
    let (dir, mut repo, out) = typechanged_link_repo();

    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "tc"));
    assert_eq!(read(dir.path(), "tc"), "plain\n");
    assert_eq!(read(dir.path(), "a.txt"), "a\n");
    let list = git_core::stash::list_stashes(&mut repo).unwrap();
    assert!(
        list[0].message.starts_with("WIP on "),
        "{}",
        list[0].message
    );
    assert_eq!(
        git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap(),
        StashApplyOutcome::Applied
    );
    assert!(common::is_link(dir.path(), "tc"));
    assert_eq!(read(dir.path(), "a.txt"), "edited\n");
    common::assert_outside_untouched(out.path());
}

#[test]
#[cfg(unix)]
fn stash_all_keep_index_with_a_typechanged_symlink_does_not_write_through() {
    let (dir, mut repo, out) = typechanged_link_repo();
    stage(&repo, "a.txt");

    git_core::stash::save_stash(&mut repo, &with(|o| o.keep_index = true)).unwrap();

    common::assert_outside_untouched(out.path());
    assert_eq!(read(dir.path(), "tc"), "plain\n");
    assert_eq!(read(dir.path(), "a.txt"), "edited\n");
}

#[test]
#[cfg(unix)]
fn stash_all_tracked_only_with_a_typechanged_symlink_does_not_write_through() {
    let (dir, mut repo, out) = typechanged_link_repo();
    write_file(dir.path(), "untracked.txt", "u\n");

    git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = false)).unwrap();

    common::assert_outside_untouched(out.path());
    assert_eq!(read(dir.path(), "tc"), "plain\n");
    assert_eq!(read(dir.path(), "untracked.txt"), "u\n");
}

#[test]
#[cfg(unix)]
fn stash_all_refuses_when_a_directory_was_replaced_by_a_symlink() {
    let out = common::outside_dir();
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "d/f", "inner\n");
    commit_all(&repo, "initial");
    common::replace_with_link(dir.path(), "d", out.path());

    let result = git_core::stash::save_stash(&mut repo, &StashOptions::default());

    assert!(
        matches!(result, Err(StashError::BeyondSymlink(_))),
        "{result:?}"
    );
    common::assert_outside_untouched(out.path());
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
    assert!(common::is_link(dir.path(), "d"));
}

/// HEAD has `d/f`; the folder `d` was replaced by a regular (untracked) file `d` (#154).
fn folder_replaced_by_file() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "d/f", "f\n");
    commit_all(&repo, "base");
    std::fs::remove_dir_all(dir.path().join("d")).unwrap();
    std::fs::write(dir.path().join("d"), "now a file\n").unwrap();
    (dir, repo)
}

#[test]
fn stashing_a_file_whose_folder_is_now_a_file_is_refused_before_saving() {
    let (dir, mut repo) = folder_replaced_by_file();

    let err = git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["d/f".into()]))
        .unwrap_err();

    assert_eq!(
        err.to_string(),
        "Can't stash 'd/f': 'd' is now a file, not a folder. Move or delete 'd' first."
    );
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
    assert_eq!(read(dir.path(), "d"), "now a file\n");
}

/// HEAD has the file `d`; it was replaced by a folder holding the untracked `d/x`.
fn file_replaced_by_folder() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "d", "f\n");
    commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("d")).unwrap();
    write_file(dir.path(), "d/x", "x\n");
    (dir, repo)
}

fn git(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

/// The names of the worktree's top-level entries, sorted (catches libgit2's `d~…` side files).
fn top_level(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != ".git")
        .collect();
    names.sort();
    names
}

/// The worktree after a stash of the file `d` put HEAD's folder back.
fn assert_folder_back(dir: &std::path::Path) {
    assert!(std::fs::symlink_metadata(dir.join("d")).unwrap().is_dir());
    assert_eq!(read(dir, "d/f"), "f\n");
    assert_eq!(top_level(dir), ["d"]);
}

#[test]
fn path_limited_stash_of_a_file_that_replaced_a_folder_stashes_the_folders_deletion() {
    // #186: git stash push -u -- d stashes the untracked `d` with `d/f`'s deletion and puts the
    // folder back. Browsitory refused with "Can't stash 'd/f': 'd' is now a file…".
    let (dir, mut repo) = folder_replaced_by_file();

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["d".into()])).unwrap();

    assert_folder_back(dir.path());
    assert!(git_core::status::status(&repo).unwrap().is_empty());
    let stash_id = stash_commit(&mut repo, 0);
    let stash = repo.find_commit(stash_id).unwrap();
    assert!(stash
        .tree()
        .unwrap()
        .get_path(std::path::Path::new("d/f"))
        .is_err());
    let untracked = stash.parent(2).unwrap().tree().unwrap();
    let entry = untracked.get_path(std::path::Path::new("d")).unwrap();
    assert_eq!(entry.kind(), Some(git2::ObjectType::Blob));
    // git itself pops it back to the file.
    let output = git(dir.path(), &["stash", "pop"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(read(dir.path(), "d"), "now a file\n");
}

/// libgit2 checks a stash's untracked files out before its tracked changes, so it can't bring a
/// file back where the stash also deletes the folder of that name: its "Applied" left the file
/// as `d~Stashed changes` and the pop dropped the stash (#186).
fn assert_file_over_folder_apply_refused(
    repo: &mut git2::Repository,
    dir: &std::path::Path,
    error: StashError,
) {
    assert!(matches!(error, StashError::FileReplacedFolder), "{error:?}");
    assert_eq!(
        error.to_string(),
        "This stash can't be applied here because a file replaced a folder; use git stash apply from a terminal."
    );
    assert_eq!(git_core::stash::list_stashes(repo).unwrap().len(), 1);
    assert_folder_back(dir);
    assert!(git_core::status::status(repo).unwrap().is_empty());
}

#[test]
fn applying_or_popping_a_stashed_file_that_replaced_a_folder_is_a_clear_error() {
    for opts in [
        with(|o| o.paths = vec!["d".into()]),
        StashOptions::default(),
    ] {
        let (dir, mut repo) = folder_replaced_by_file();
        git_core::stash::save_stash(&mut repo, &opts).unwrap();
        assert_folder_back(dir.path());

        let error = git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap_err();
        assert_file_over_folder_apply_refused(&mut repo, dir.path(), error);
        let error = git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap_err();
        assert_file_over_folder_apply_refused(&mut repo, dir.path(), error);

        // The stash is intact: git applies it.
        let output = git(dir.path(), &["stash", "pop"]);
        assert!(
            output.status.success(),
            "{opts:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(read(dir.path(), "d"), "now a file\n");
    }
}

/// Stashing without untracked files would reset `tracked` and delete the untracked `untracked`
/// in its way, as git does; Browsitory refuses and writes nothing (#186).
fn assert_untracked_in_the_way(
    repo: &mut git2::Repository,
    result: Result<(), StashError>,
    untracked: &str,
    tracked: &str,
    status_before: &[git_core::status::StatusEntry],
) {
    let error = result.unwrap_err();
    assert!(
        matches!(
            error,
            StashError::UntrackedInTheWay { untracked: ref u, tracked: ref t }
                if u == untracked && t == tracked
        ),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        format!(
            "Stashing would delete the untracked file '{untracked}', which is in the way of '{tracked}'. Turn on Include untracked files, or move '{untracked}' first."
        )
    );
    assert!(git_core::stash::list_stashes(repo).unwrap().is_empty());
    assert!(repo.find_reference("refs/stash").is_err());
    assert_eq!(git_core::status::status(repo).unwrap(), status_before);
}

#[test]
fn stash_without_untracked_refuses_to_delete_a_file_that_replaced_a_folder() {
    for paths in [vec![], vec!["d".to_string()], vec!["d/f".to_string()]] {
        for keep_index in [false, true] {
            let (dir, mut repo) = folder_replaced_by_file();
            let status_before = git_core::status::status(&repo).unwrap();

            let result = git_core::stash::save_stash(
                &mut repo,
                &with(|o| {
                    o.paths = paths.clone();
                    o.include_untracked = false;
                    o.keep_index = keep_index;
                }),
            );

            assert_untracked_in_the_way(&mut repo, result, "d", "d/f", &status_before);
            assert_eq!(read(dir.path(), "d"), "now a file\n");
        }
    }
}

#[test]
fn stash_all_without_untracked_refuses_to_delete_files_in_a_folder_that_replaced_a_file() {
    let (dir, mut repo) = file_replaced_by_folder();
    let status_before = git_core::status::status(&repo).unwrap();

    let result = git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = false));

    assert_untracked_in_the_way(&mut repo, result, "d/x", "d", &status_before);
    assert_eq!(read(dir.path(), "d/x"), "x\n");
}

#[test]
fn stash_without_untracked_still_stashes_unrelated_changes_next_to_untracked_files() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "d/f", "f\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "a.txt", "edited\n");
    write_file(dir.path(), "d/new", "untracked\n");
    write_file(dir.path(), "e", "untracked\n");

    git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = false)).unwrap();

    assert_eq!(read(dir.path(), "a.txt"), "a\n");
    assert_eq!(read(dir.path(), "d/new"), "untracked\n");
    assert_eq!(read(dir.path(), "e"), "untracked\n");
}

#[test]
fn stash_all_without_untracked_refuses_a_case_only_swap_when_ignorecase_is_on() {
    // On a case-insensitive drive `docs` is the folder `Docs`: the reset that brings `Docs/f`
    // back would delete the untracked file.
    let (dir, mut repo) = init_repo();
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    write_file(dir.path(), "Docs/f", "f\n");
    commit_all(&repo, "base");
    std::fs::remove_dir_all(dir.path().join("Docs")).unwrap();
    write_file(dir.path(), "docs", "now a file\n");
    let status_before = git_core::status::status(&repo).unwrap();

    let result = git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = false));

    assert_untracked_in_the_way(&mut repo, result, "docs", "Docs/f", &status_before);
    assert_eq!(read(dir.path(), "docs"), "now a file\n");
}

#[test]
fn stash_all_with_keep_index_checks_untracked_files_against_head_and_the_index() {
    // A staged deletion of `d/f`: the keep-index stash still resets to HEAD first, which writes
    // `d/f` and deletes the file `d`, before checking the index out again.
    let (dir, mut repo) = folder_replaced_by_file();
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new("d/f")).unwrap();
    index.write().unwrap();
    let status_before = git_core::status::status(&repo).unwrap();

    let result = git_core::stash::save_stash(
        &mut repo,
        &with(|o| {
            o.include_untracked = false;
            o.keep_index = true;
        }),
    );

    assert_untracked_in_the_way(&mut repo, result, "d", "d/f", &status_before);
    assert_eq!(read(dir.path(), "d"), "now a file\n");

    // git deletes the file too: like libgit2, its keep-index stash resets to HEAD (writing `d/f`)
    // before checking the index back out.
    let (twin, _twin_repo) = folder_replaced_by_file();
    assert!(git(twin.path(), &["rm", "-q", "--cached", "d/f"])
        .status
        .success());
    assert!(git(twin.path(), &["stash", "push", "--keep-index"])
        .status
        .success());
    assert!(!twin.path().join("d").is_file());
}

/// HEAD has only `a`; `n/f` is staged as new, then deleted from the worktree and replaced by the
/// untracked file `n`.
fn staged_folder_replaced_by_file() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a", "a\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "n/f", "staged\n");
    stage(&repo, "n/f");
    std::fs::remove_dir_all(dir.path().join("n")).unwrap();
    write_file(dir.path(), "n", "now a file\n");
    (dir, repo)
}

#[test]
fn stash_all_with_keep_index_refuses_an_untracked_file_in_the_way_of_a_staged_folder() {
    let (dir, mut repo) = staged_folder_replaced_by_file();
    let status_before = git_core::status::status(&repo).unwrap();

    let result = git_core::stash::save_stash(
        &mut repo,
        &with(|o| {
            o.include_untracked = false;
            o.keep_index = true;
        }),
    );

    assert_untracked_in_the_way(&mut repo, result, "n", "n/f", &status_before);
    assert_eq!(read(dir.path(), "n"), "now a file\n");
}

#[test]
fn stash_all_without_keep_index_leaves_an_untracked_file_where_only_the_index_has_a_folder() {
    let (dir, mut repo) = staged_folder_replaced_by_file();

    git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = false)).unwrap();

    assert_eq!(read(dir.path(), "n"), "now a file\n");
}

#[cfg(unix)]
#[test]
fn stash_all_without_untracked_refuses_an_untracked_link_over_a_folder_as_beyond_a_link() {
    // The link takes the hazard route, which refuses it as before (#114).
    let out = common::outside_dir();
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "d/f", "inner\n");
    commit_all(&repo, "initial");
    common::replace_with_link(dir.path(), "d", out.path());

    let result = git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = false));

    assert!(
        matches!(result, Err(StashError::BeyondSymlink(_))),
        "{result:?}"
    );
    common::assert_outside_untouched(out.path());
    assert!(common::is_link(dir.path(), "d"));
}

#[test]
fn applying_a_stashed_untracked_file_over_a_new_folder_names_the_folder() {
    // Not a swap the stash recorded: the folder `d` appeared after stashing the file `d`.
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a", "a\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "d", "untracked\n");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    write_file(dir.path(), "d/x", "x\n");

    let error = git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap_err();

    assert!(
        matches!(error, StashError::UntrackedOverFolder(ref path) if path == "d"),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "The untracked file 'd' in this stash would overwrite the folder 'd'; move the folder first."
    );
    assert_eq!(read(dir.path(), "d/x"), "x\n");
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
}

#[test]
fn stash_all_without_untracked_stashes_an_edit_to_a_file_removed_from_the_index() {
    // `git rm --cached d` then an edit: status is INDEX_DELETED | WT_NEW. Like git, the stash
    // records the edited `d` in its worktree tree and resets `d` to HEAD's content; Pop brings
    // the edit back. Nothing is lost, so this is not refused.
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "d", "base\n");
    commit_all(&repo, "base");
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new("d")).unwrap();
    index.write().unwrap();
    write_file(dir.path(), "d", "edited\n");

    git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = false)).unwrap();

    assert_eq!(read(dir.path(), "d"), "base\n");
    let stash_id = stash_commit(&mut repo, 0);
    let stash = repo.find_commit(stash_id).unwrap();
    let entry = stash
        .tree()
        .unwrap()
        .get_path(std::path::Path::new("d"))
        .unwrap();
    assert_eq!(repo.find_blob(entry.id()).unwrap().content(), b"edited\n");
    // git does the same.
    let (twin, twin_repo) = init_repo();
    write_file(twin.path(), "d", "base\n");
    commit_all(&twin_repo, "base");
    assert!(git(twin.path(), &["rm", "-q", "--cached", "d"])
        .status
        .success());
    write_file(twin.path(), "d", "edited\n");
    assert!(git(twin.path(), &["stash"]).status.success());
    assert_eq!(read(twin.path(), "d"), "base\n");
    let shown = git(twin.path(), &["show", "stash@{0}:d"]);
    assert_eq!(String::from_utf8_lossy(&shown.stdout), "edited\n");

    drop(stash);
    assert_eq!(
        git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap(),
        StashApplyOutcome::Applied
    );
    assert_eq!(read(dir.path(), "d"), "edited\n");
}

/// HEAD has `d/f` and ignores `d`; the folder was replaced by the ignored file `d` (#208).
fn ignored_file_over_folder() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "d/f", "f\n");
    commit_all(&repo, "base");
    write_file(dir.path(), ".gitignore", "d\n");
    commit_all(&repo, "ignore d");
    std::fs::remove_dir_all(dir.path().join("d")).unwrap();
    write_file(dir.path(), "d", "ignored\n");
    (dir, repo)
}

/// HEAD has the file `build` and ignores the folder `build/`; the file was replaced by the
/// ignored folder `build/` holding `build/out.o` (#208).
fn ignored_folder_over_file() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "build", "script\n");
    write_file(dir.path(), ".gitignore", "build/\n");
    commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("build")).unwrap();
    write_file(dir.path(), "build/out.o", "object\n");
    (dir, repo)
}

/// HEAD has the file `d`; it was replaced by a repository with one commit of `d/inner.txt` (#211).
fn nested_repo_over_file() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "d", "f\n");
    commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("d")).unwrap();
    let inner = git2::Repository::init(dir.path().join("d")).unwrap();
    {
        let mut config = inner.config().unwrap();
        config.set_str("user.name", "Inner").unwrap();
        config.set_str("user.email", "inner@example.com").unwrap();
    }
    write_file(dir.path(), "d/inner.txt", "inner\n");
    commit_all(&inner, "inner");
    (dir, repo)
}

/// Runs `git stash push [-u]` in `dir` and reports whether it succeeded.
fn git_stash_push(dir: &std::path::Path, include_untracked: bool) -> bool {
    let mut args = vec!["stash", "push"];
    if include_untracked {
        args.push("-u");
    }
    git(dir, &args).status.success()
}

/// Stash all would delete an entry in the way of a restored path, as git does; Browsitory
/// refuses and writes nothing (#208, #211).
fn assert_nothing_stashed(
    repo: &mut git2::Repository,
    error: &StashError,
    message: &str,
    status_before: &[git_core::status::StatusEntry],
) {
    assert_eq!(error.to_string(), message);
    assert!(git_core::stash::list_stashes(repo).unwrap().is_empty());
    assert!(repo.find_reference("refs/stash").is_err());
    assert_eq!(git_core::status::status(repo).unwrap(), status_before);
}

/// Every whole-stash mode: with and without untracked files, with and without keep-index.
fn whole_stash_modes() -> Vec<StashOptions> {
    let mut modes = Vec::new();
    for include_untracked in [false, true] {
        for keep_index in [false, true] {
            modes.push(with(|o| {
                o.include_untracked = include_untracked;
                o.keep_index = keep_index;
            }));
        }
    }
    modes
}

#[test]
fn stash_all_refuses_to_delete_an_ignored_file_in_the_way() {
    for opts in whole_stash_modes() {
        let (dir, mut repo) = ignored_file_over_folder();
        let status_before = git_core::status::status(&repo).unwrap();

        let error = git_core::stash::save_stash(&mut repo, &opts).unwrap_err();

        assert!(
            matches!(
                error,
                StashError::IgnoredInTheWay { ref ignored, ref tracked }
                    if ignored == "d" && tracked == "d/f"
            ),
            "{opts:?}: {error:?}"
        );
        assert_nothing_stashed(
            &mut repo,
            &error,
            "Stashing would delete the ignored 'd', which is in the way of the tracked 'd/f'. Move 'd' first.",
            &status_before,
        );
        assert_eq!(read(dir.path(), "d"), "ignored\n");

        // git deletes it.
        let (twin, _twin_repo) = ignored_file_over_folder();
        assert!(git_stash_push(twin.path(), opts.include_untracked));
        assert_eq!(read(twin.path(), "d/f"), "f\n");
    }
}

#[test]
fn stash_all_refuses_to_delete_an_ignored_folder_where_head_has_a_file() {
    for opts in whole_stash_modes() {
        let (dir, mut repo) = ignored_folder_over_file();
        let status_before = git_core::status::status(&repo).unwrap();

        let error = git_core::stash::save_stash(&mut repo, &opts).unwrap_err();

        assert!(
            matches!(
                error,
                StashError::IgnoredInTheWay { ref ignored, ref tracked }
                    if ignored == "build" && tracked == "build"
            ),
            "{opts:?}: {error:?}"
        );
        assert_nothing_stashed(
            &mut repo,
            &error,
            "Stashing would delete the ignored 'build', which is in the way of the tracked 'build'. Move 'build' first.",
            &status_before,
        );
        assert_eq!(read(dir.path(), "build/out.o"), "object\n");

        // git deletes it.
        let (twin, _twin_repo) = ignored_folder_over_file();
        assert!(git_stash_push(twin.path(), opts.include_untracked));
        assert_eq!(read(twin.path(), "build"), "script\n");
    }
}

#[test]
fn stash_all_with_untracked_files_refuses_to_delete_an_ignored_file_in_a_folder_that_replaced_a_file(
) {
    // The untracked `d/x` goes into the stash; the ignored `d/y.o` would just be deleted.
    let (dir, mut repo) = file_replaced_by_folder();
    write_file(dir.path(), ".git/info/exclude", "*.o\n");
    write_file(dir.path(), "d/y.o", "object\n");
    let status_before = git_core::status::status(&repo).unwrap();

    let error = git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap_err();

    assert!(
        matches!(
            error,
            StashError::IgnoredInTheWay { ref ignored, ref tracked }
                if ignored == "d/y.o" && tracked == "d"
        ),
        "{error:?}"
    );
    assert_nothing_stashed(
        &mut repo,
        &error,
        "Stashing would delete the ignored 'd/y.o', which is in the way of the tracked 'd'. Move 'd/y.o' first.",
        &status_before,
    );
    assert_eq!(read(dir.path(), "d/x"), "x\n");
    assert_eq!(read(dir.path(), "d/y.o"), "object\n");
}

#[test]
fn stash_all_refuses_to_delete_a_nested_repository_where_head_has_a_file() {
    for opts in whole_stash_modes() {
        let (dir, mut repo) = nested_repo_over_file();
        let inner_head = git2::Repository::open(dir.path().join("d"))
            .unwrap()
            .head()
            .unwrap()
            .target()
            .unwrap();
        let status_before = git_core::status::status(&repo).unwrap();

        let error = git_core::stash::save_stash(&mut repo, &opts).unwrap_err();

        assert!(
            matches!(
                error,
                StashError::RepositoryInTheWay { ref path, ref tracked }
                    if path == "d" && tracked == "d"
            ),
            "{opts:?}: {error:?}"
        );
        assert_nothing_stashed(
            &mut repo,
            &error,
            "Stashing would delete the repository in 'd', which is in the way of the tracked 'd'. Move 'd' first.",
            &status_before,
        );
        assert!(dir.path().join("d/.git/HEAD").is_file());
        assert_eq!(read(dir.path(), "d/inner.txt"), "inner\n");
        let inner = git2::Repository::open(dir.path().join("d")).unwrap();
        assert_eq!(inner.head().unwrap().target().unwrap(), inner_head);

        // git deletes it, history included.
        let (twin, _twin_repo) = nested_repo_over_file();
        assert!(git_stash_push(twin.path(), opts.include_untracked));
        assert_eq!(read(twin.path(), "d"), "f\n");
    }
}

#[test]
fn ignored_folders_elsewhere_do_not_block_a_stash() {
    for opts in whole_stash_modes() {
        let (dir, mut repo) = init_repo();
        write_file(dir.path(), "a.txt", "a\n");
        write_file(dir.path(), ".gitignore", "target/\n*.log\n");
        commit_all(&repo, "base");
        write_file(dir.path(), "target/debug/out", "built\n");
        write_file(dir.path(), "run.log", "log\n");
        write_file(dir.path(), "a.txt", "edited\n");

        git_core::stash::save_stash(&mut repo, &opts).unwrap();

        assert_eq!(read(dir.path(), "a.txt"), "a\n", "{opts:?}");
        assert_eq!(read(dir.path(), "target/debug/out"), "built\n");
        assert_eq!(read(dir.path(), "run.log"), "log\n");
        assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
    }
}

#[test]
fn a_nested_repository_elsewhere_does_not_block_a_stash() {
    for opts in whole_stash_modes() {
        let (dir, mut repo) = init_repo();
        write_file(dir.path(), "a.txt", "a\n");
        write_file(dir.path(), "vendor/README", "vendored\n");
        commit_all(&repo, "base");
        for nested in ["lib", "vendor/lib"] {
            git2::Repository::init(dir.path().join(nested)).unwrap();
            write_file(dir.path(), &format!("{nested}/inner.txt"), "inner\n");
        }
        write_file(dir.path(), "a.txt", "edited\n");

        git_core::stash::save_stash(&mut repo, &opts).unwrap();

        assert_eq!(read(dir.path(), "a.txt"), "a\n", "{opts:?}");
        for nested in ["lib", "vendor/lib"] {
            assert_eq!(read(dir.path(), &format!("{nested}/inner.txt")), "inner\n");
            assert!(dir.path().join(nested).join(".git").is_dir());
        }
    }
}

/// libgit2 merges a stash's untracked files against the index, so an untracked `d/x` where the
/// index has the file `d` was left as `d~Updated upstream`, reported as applied, and Pop dropped
/// the stash (#206). Browsitory refuses and writes nothing.
fn assert_untracked_below_tracked_file_refused(
    repo: &mut git2::Repository,
    dir: &std::path::Path,
    error: StashError,
    d: &str,
) {
    assert!(
        matches!(
            error,
            StashError::UntrackedBelowTrackedFile { ref untracked, ref file }
                if untracked == "d/x" && file == "d"
        ),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "This stash can't be applied here: its untracked file 'd/x' needs a folder where 'd' is a tracked file. Commit the deletion of 'd' first, then apply it; the stash is kept."
    );
    assert_eq!(git_core::stash::list_stashes(repo).unwrap().len(), 1);
    assert_eq!(top_level(dir), ["d"]);
    assert_eq!(read(dir, "d"), d);
    assert!(git_core::status::status(repo).unwrap().is_empty());
}

#[test]
fn applying_or_popping_a_stash_whose_folder_replaced_a_tracked_file_is_refused() {
    let (dir, mut repo) = file_replaced_by_folder();
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    assert_eq!(top_level(dir.path()), ["d"]);

    for restore_index in [false, true] {
        let error = git_core::stash::apply_stash(&mut repo, 0, None, restore_index).unwrap_err();
        assert_untracked_below_tracked_file_refused(&mut repo, dir.path(), error, "f\n");
        let error = git_core::stash::pop_stash(&mut repo, 0, None, restore_index).unwrap_err();
        assert_untracked_below_tracked_file_refused(&mut repo, dir.path(), error, "f\n");
    }

    // git can't pop it either.
    let (twin, _twin_repo) = file_replaced_by_folder();
    assert!(git_stash_push(twin.path(), true));
    assert!(!git(twin.path(), &["stash", "pop"]).status.success());
}

#[test]
fn a_stashed_untracked_folder_below_a_file_committed_later_is_refused() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a", "a\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "d/x", "x\n");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    std::fs::remove_file(dir.path().join("a")).unwrap();
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new("a")).unwrap();
    index.write().unwrap();
    write_file(dir.path(), "d", "later\n");
    commit_all(&repo, "add the file d");

    let error = git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap_err();

    assert_untracked_below_tracked_file_refused(&mut repo, dir.path(), error, "later\n");
}

#[test]
fn the_refused_stash_applies_after_the_deletion_is_committed() {
    let (dir, mut repo) = file_replaced_by_folder();
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    std::fs::remove_file(dir.path().join("d")).unwrap();
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new("d")).unwrap();
    index.write().unwrap();
    commit_all(&repo, "delete d");

    assert_eq!(
        git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap(),
        StashApplyOutcome::Applied
    );

    assert_eq!(top_level(dir.path()), ["d"]);
    assert_eq!(read(dir.path(), "d/x"), "x\n");
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
}
