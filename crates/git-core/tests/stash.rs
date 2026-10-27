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

#[cfg(unix)]
fn assert_l_is_heads_symlink(dir: &std::path::Path, target: &std::path::Path) {
    let meta = std::fs::symlink_metadata(dir.join("l")).unwrap();
    assert!(meta.file_type().is_symlink(), "l is HEAD's symlink again");
    assert_eq!(std::fs::read_link(dir.join("l")).unwrap(), target);
}

#[cfg(unix)]
#[test]
fn stashing_a_directory_that_replaced_a_symlink_never_deletes_the_links_target_outside() {
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("f"), "outside, unsaved\n").unwrap();
    let (dir, mut repo) = symlink_replaced_by_directory(outside.path());

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["l".into()])).unwrap();

    assert_eq!(
        std::fs::read_to_string(outside.path().join("f")).unwrap(),
        "outside, unsaved\n"
    );
    assert_l_is_heads_symlink(dir.path(), outside.path());

    let result = git_core::stash::pop_stash(&mut repo, 0, None, false);
    assert_eq!(
        std::fs::read_to_string(outside.path().join("f")).unwrap(),
        "outside, unsaved\n"
    );
    match result {
        Ok(StashApplyOutcome::Applied) => {
            assert!(std::fs::symlink_metadata(dir.path().join("l"))
                .unwrap()
                .is_dir());
            assert_eq!(read(dir.path(), "l/f"), "mine\n");
            assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
        }
        other => {
            assert!(
                matches!(other, Err(StashError::SymlinkReplacedByDirectory)),
                "{other:?}"
            );
            assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
        }
    }
}

#[cfg(unix)]
#[test]
fn stashing_a_directory_that_replaced_a_symlink_never_deletes_the_links_target_in_repo() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "real/f", "committed\n");
    std::os::unix::fs::symlink("real", dir.path().join("l")).unwrap();
    commit_all(&repo, "initial");
    write_file(dir.path(), "real/f", "unsaved edits\n");
    std::fs::remove_file(dir.path().join("l")).unwrap();
    write_file(dir.path(), "l/f", "mine\n");

    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["l".into()])).unwrap();

    assert_eq!(read(dir.path(), "real/f"), "unsaved edits\n");
    assert_l_is_heads_symlink(dir.path(), std::path::Path::new("real"));

    let result = git_core::stash::pop_stash(&mut repo, 0, None, false);
    assert_eq!(read(dir.path(), "real/f"), "unsaved edits\n");
    match result {
        Ok(StashApplyOutcome::Applied) => {
            assert!(std::fs::symlink_metadata(dir.path().join("l"))
                .unwrap()
                .is_dir());
            assert_eq!(read(dir.path(), "l/f"), "mine\n");
        }
        other => {
            assert!(
                matches!(other, Err(StashError::SymlinkReplacedByDirectory)),
                "{other:?}"
            );
            assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
        }
    }
}
