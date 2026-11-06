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

    let output = test_support::git_command(dir.path())
        .args(["stash", "apply", "--index"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let staged_a = test_support::git_command(dir.path())
        .args(["show", ":a"])
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

use test_support::git;

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

/// HEAD has `{folder}/f`; the worktree swapped the folder for the untracked file `file`, a name
/// a case-insensitive file system folds to the same as `folder`, with `core.ignorecase` set to
/// `ignore_case`. Saving without untracked files returns the error and the file stays.
fn save_over_folded_name(folder: &str, file: &str, ignore_case: bool) -> Result<(), StashError> {
    let (dir, mut repo) = init_repo();
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", ignore_case)
        .unwrap();
    write_file(dir.path(), &format!("{folder}/f"), "f\n");
    commit_all(&repo, "base");
    std::fs::remove_dir_all(dir.path().join(folder)).unwrap();
    write_file(dir.path(), file, "now a file\n");
    let status_before = git_core::status::status(&repo).unwrap();

    let result = git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = false));

    if result.is_err() {
        assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
        assert_eq!(git_core::status::status(&repo).unwrap(), status_before);
    }
    assert_eq!(read(dir.path(), file), "now a file\n");
    result
}

/// `result` is `UntrackedInTheWay` for one of `untracked` (macOS may report a name in its
/// precomposed form) in the way of `tracked`.
fn assert_in_the_way_of(result: Result<(), StashError>, untracked: &[&str], tracked: &str) {
    let error = result.unwrap_err();
    assert!(
        matches!(
            error,
            StashError::UntrackedInTheWay { untracked: ref u, tracked: ref t }
                if untracked.contains(&u.as_str()) && t == tracked
        ),
        "{error:?}"
    );
}

#[test]
fn stash_save_folds_sharp_s_like_the_file_system_when_ignorecase_is_on() {
    // `to_lowercase` keeps `ß`; the full case fold the guard uses makes it `ss` (#337).
    let result = save_over_folded_name("Straße", "STRASSE", true);
    assert_in_the_way_of(result, &["STRASSE"], "Straße/f");
}

#[test]
fn stash_save_treats_composed_and_decomposed_names_as_one_when_ignorecase_is_on() {
    // APFS and HFS+ compare names after Unicode normalization: `é` as U+00E9 and as `e` +
    // U+0301 are one name (#337).
    let result = save_over_folded_name("Caf\u{e9}", "Cafe\u{301}", true);
    assert_in_the_way_of(result, &["Cafe\u{301}", "Caf\u{e9}"], "Caf\u{e9}/f");
}

#[test]
fn stash_save_ignores_hfs_ignorable_code_points_when_ignorecase_is_on() {
    // HFS+ drops U+200C (zero-width non-joiner) from names, so there `do\u{200c}cs` is `docs`.
    // APFS doesn't ignore U+200C; this documents `fold_name`'s HFS+ behaviour, which errs
    // towards refusing more (#337).
    let result = save_over_folded_name("Docs", "do\u{200c}cs", true);
    assert_in_the_way_of(result, &["do\u{200c}cs"], "Docs/f");
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn stash_save_folds_case_on_macos_and_windows_even_if_ignorecase_is_off() {
    // A repository copied from elsewhere can carry a wrong `core.ignorecase = false`; the drive
    // still folds case, so the reset would delete `docs` to bring back `Docs/f` (#337, as the
    // apply/pop check since #234).
    let result = save_over_folded_name("Docs", "docs", false);
    assert_in_the_way_of(result, &["docs"], "Docs/f");
}

#[cfg(target_os = "linux")]
#[test]
fn stash_save_keeps_case_apart_when_ignorecase_is_off() {
    // A case-sensitive drive: `docs` and `Docs` are two names, nothing is in the way. A temp
    // dir on a case-insensitive file system (a casefold folder, a vfat mount) can't hold both,
    // so the test has nothing to check there.
    let probe = tempfile::TempDir::new().unwrap();
    std::fs::write(probe.path().join("a"), "").unwrap();
    if probe.path().join("A").exists() {
        eprintln!("skipped: the temp dir folds case");
        return;
    }
    save_over_folded_name("Docs", "docs", false).unwrap();
    save_over_folded_name("Straße", "STRASSE", false).unwrap();
}

#[test]
fn stash_all_with_keep_index_checks_untracked_files_against_head_and_the_index() {
    // A staged deletion of `d/f`: libgit2's keep-index stash resets once, to the index commit,
    // but with HEAD as the checkout baseline, so it still deletes the file `d` in the way of
    // HEAD's `d/f` (and writes nothing in its place).
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

    // git deletes the file too: its keep-index stash resets to HEAD (writing `d/f`) before
    // checking the index back out.
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
        "This stash can't be applied here: its untracked file 'd/x' needs a folder where 'd' is tracked. Commit the deletion of 'd' first, then apply it; the stash is kept."
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

/// HEAD has the file `D`; the worktree swapped it for the folder `d` holding the untracked
/// `d/x`, with `core.ignorecase` set to `ignore_case`. On a case-insensitive drive `d` is `D`'s
/// name, so this is the folder-replaced-a-file stash of #206 (on Linux both names fit).
fn file_replaced_by_case_variant_folder(
    ignore_case: bool,
) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", ignore_case)
        .unwrap();
    write_file(dir.path(), "D", "f\n");
    commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("D")).unwrap();
    write_file(dir.path(), "d/x", "x\n");
    (dir, repo)
}

#[test]
fn a_case_only_variant_of_a_tracked_file_blocks_a_stashed_folder_when_ignorecase_is_on() {
    // On a case-insensitive drive the untracked `d/x` needs the folder `d`, where the index has
    // the file `D`: libgit2's merge compares exact case and misses it (#234).
    let (dir, mut repo) = file_replaced_by_case_variant_folder(true);
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    assert_eq!(top_level(dir.path()), ["D"]);

    for restore_index in [false, true] {
        for pop in [false, true] {
            let error = if pop {
                git_core::stash::pop_stash(&mut repo, 0, None, restore_index)
            } else {
                git_core::stash::apply_stash(&mut repo, 0, None, restore_index)
            }
            .unwrap_err();
            assert!(
                matches!(
                    error,
                    StashError::UntrackedBelowTrackedFile { ref untracked, ref file }
                        if untracked == "d/x" && file == "D"
                ),
                "{error:?}"
            );
            assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
            assert_eq!(top_level(dir.path()), ["D"]);
            assert_eq!(read(dir.path(), "D"), "f\n");
            assert!(git_core::status::status(&repo).unwrap().is_empty());
        }
    }
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn a_case_only_variant_blocks_a_stashed_folder_on_macos_and_windows_even_if_ignorecase_is_off() {
    // A repository copied from elsewhere can carry a wrong `core.ignorecase = false`; the drive
    // still folds case, so `d` is `D` (#234).
    let (dir, mut repo) = file_replaced_by_case_variant_folder(false);
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

    let error = git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap_err();

    assert!(
        matches!(
            error,
            StashError::UntrackedBelowTrackedFile { ref untracked, ref file }
                if untracked == "d/x" && file == "D"
        ),
        "{error:?}"
    );
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
    assert_eq!(top_level(dir.path()), ["D"]);
    assert_eq!(read(dir.path(), "D"), "f\n");
}

#[cfg(target_os = "linux")]
#[test]
fn a_case_only_variant_of_a_tracked_file_does_not_block_a_stash_when_ignorecase_is_off() {
    // A case-sensitive drive: `d` and `D` are two names, so the pop creates `d/x` and deletes
    // `D`, as stashed.
    let (dir, mut repo) = file_replaced_by_case_variant_folder(false);
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

    assert_eq!(
        git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap(),
        StashApplyOutcome::Applied
    );

    assert_eq!(top_level(dir.path()), ["d"]);
    assert_eq!(read(dir.path(), "d/x"), "x\n");
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn git_pops_a_case_only_variant_under_ignorecase_on_a_case_sensitive_drive() {
    // git with `core.ignorecase` on a case-sensitive drive: nothing is in the way, so the pop
    // succeeds. On a case-insensitive drive it can't (see the next test). Browsitory refuses
    // whenever the repository says names fold, on purpose: a Linux casefold folder or a vfat
    // mount folds names too, and refusing is the safe direction.
    let (twin, _twin_repo) = file_replaced_by_case_variant_folder(true);
    assert!(git_stash_push(twin.path(), true));
    assert_eq!(top_level(twin.path()), ["D"]);

    assert!(git(twin.path(), &["stash", "pop"]).status.success());

    assert_eq!(top_level(twin.path()), ["d"]);
    assert_eq!(read(twin.path(), "d/x"), "x\n");
}

/// A stash with the untracked files `untracked` (base: HEAD with `a`), with `core.ignorecase`
/// on; then `committed` are committed as files, so the index has them when the stash is applied.
fn stash_untracked_then_commit(
    untracked: &[&str],
    committed: &[&str],
) -> (tempfile::TempDir, git2::Repository) {
    let (dir, mut repo) = init_repo();
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    write_file(dir.path(), "a", "a\n");
    commit_all(&repo, "base");
    for path in untracked {
        write_file(dir.path(), path, "untracked\n");
    }
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    for path in committed {
        write_file(dir.path(), path, "committed\n");
    }
    commit_all(&repo, "later");
    (dir, repo)
}

#[test]
fn a_folded_name_only_blocks_at_a_whole_path_component() {
    // `Dx` and `D2` merely start like `d`; `dx` merely starts like `D`. None is in the way.
    for (untracked, committed) in [(&["d/x"][..], &["Dx", "D2"][..]), (&["dx/y"], &["D"])] {
        let (dir, mut repo) = stash_untracked_then_commit(untracked, committed);

        assert_eq!(
            git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap(),
            StashApplyOutcome::Applied,
            "{untracked:?} over {committed:?}"
        );
        assert_eq!(read(dir.path(), untracked[0]), "untracked\n");
    }
}

#[test]
fn a_folded_tracked_file_blocks_files_at_any_depth_below_it() {
    for (committed, file) in [("D/E", "D/E"), ("D", "D")] {
        let (dir, mut repo) = stash_untracked_then_commit(&["d/e/x"], &[committed]);

        let error = git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap_err();

        assert!(
            matches!(
                error,
                StashError::UntrackedBelowTrackedFile { ref untracked, file: ref blocking }
                    if untracked == "d/e/x" && blocking == file
            ),
            "{error:?}"
        );
        assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
        assert_eq!(read(dir.path(), committed), "committed\n");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn of_two_index_files_that_fold_together_the_first_in_index_order_is_reported() {
    // Only a case-sensitive drive holds both `D` and `d`; the index has `D` first. (libgit2's
    // `add_all` keeps one of them while `core.ignorecase` is on, so commit them with it off,
    // through a fresh repository: an open one keeps its index's case mode.)
    let (dir, repo) = stash_untracked_then_commit(&["d/x"], &[]);
    let set_ignore_case = |on| {
        repo.config()
            .unwrap()
            .set_bool("core.ignorecase", on)
            .unwrap()
    };
    set_ignore_case(false);
    write_file(dir.path(), "D", "committed\n");
    write_file(dir.path(), "d", "committed\n");
    commit_all(&git2::Repository::open(dir.path()).unwrap(), "both");
    set_ignore_case(true);
    drop(repo);
    let mut repo = git2::Repository::open(dir.path()).unwrap();
    let paths: Vec<Vec<u8>> = repo.index().unwrap().iter().map(|e| e.path).collect();
    // With `core.ignorecase` the index iterates case-insensitively.
    assert_eq!(paths, [b"a".to_vec(), b"D".to_vec(), b"d".to_vec()]);

    let error = git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap_err();

    assert!(
        matches!(
            error,
            StashError::UntrackedBelowTrackedFile { ref untracked, ref file }
                if untracked == "d/x" && file == "D"
        ),
        "{error:?}"
    );
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn git_cannot_pop_a_case_only_variant_on_a_case_insensitive_drive() {
    // `d/x` needs the folder `d`, which is the tracked file `D` here, as in #206.
    let (twin, _twin_repo) = file_replaced_by_case_variant_folder(true);
    assert!(git_stash_push(twin.path(), true));

    assert!(!git(twin.path(), &["stash", "pop"]).status.success());
}

/// HEAD has `a.txt` (edited in the worktree); the untracked folder `vendor/` holds a repository
/// `vendor/lib` with one commit and, with `other`, the untracked file `vendor/other.txt`.
fn nested_repo_in_untracked_folder(other: bool) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let inner = git2::Repository::init(dir.path().join("vendor/lib")).unwrap();
    {
        let mut config = inner.config().unwrap();
        config.set_str("user.name", "Inner").unwrap();
        config.set_str("user.email", "inner@example.com").unwrap();
    }
    write_file(dir.path(), "vendor/lib/inner.txt", "inner\n");
    commit_all(&inner, "inner");
    if other {
        write_file(dir.path(), "vendor/other.txt", "other\n");
    }
    write_file(dir.path(), "a.txt", "edited\n");
    (dir, repo)
}

/// The files of a stash's untracked-files commit, sorted; empty without one.
fn untracked_files_of(repo: &git2::Repository, stash: git2::Oid) -> Vec<String> {
    let Ok(untracked) = repo.find_commit(stash).unwrap().parent(2) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    untracked
        .tree()
        .unwrap()
        .walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() != Some(git2::ObjectType::Tree) {
                files.push(format!("{root}{}", entry.name().unwrap()));
            }
            git2::TreeWalkResult::Ok
        })
        .unwrap();
    files.sort();
    files
}

#[test]
fn stash_all_with_untracked_files_keeps_a_repository_nested_in_an_untracked_folder() {
    // libgit2's REMOVE_UNTRACKED removes the untracked `vendor/` whole: it only protects a
    // folder that itself holds a `.git`. git's `stash -u` keeps the repository.
    for other in [false, true] {
        for keep_index in [false, true] {
            let (dir, mut repo) = nested_repo_in_untracked_folder(other);
            let inner_head = git2::Repository::open(dir.path().join("vendor/lib"))
                .unwrap()
                .head()
                .unwrap()
                .target()
                .unwrap();

            git_core::stash::save_stash(&mut repo, &with(|o| o.keep_index = keep_index)).unwrap();

            let context = format!("other {other}, keep_index {keep_index}");
            assert!(dir.path().join("vendor/lib/.git").is_dir(), "{context}");
            let inner = git2::Repository::open(dir.path().join("vendor/lib")).unwrap();
            assert_eq!(inner.head().unwrap().target().unwrap(), inner_head);
            assert_eq!(read(dir.path(), "vendor/lib/inner.txt"), "inner\n");
            assert!(!dir.path().join("vendor/other.txt").exists(), "{context}");
            assert_eq!(read(dir.path(), "a.txt"), "a\n");

            // The stash holds what git's does.
            let stash = stash_commit(&mut repo, 0);
            let expected: Vec<String> = if other {
                vec!["vendor/other.txt".into()]
            } else {
                Vec::new()
            };
            assert_eq!(untracked_files_of(&repo, stash), expected, "{context}");
            let a = repo
                .find_commit(stash)
                .unwrap()
                .tree()
                .unwrap()
                .get_path(std::path::Path::new("a.txt"))
                .unwrap()
                .id();
            assert_eq!(repo.find_blob(a).unwrap().content(), b"edited\n");
            let (twin, twin_repo) = nested_repo_in_untracked_folder(other);
            let mut args = vec!["stash", "push", "-u"];
            if keep_index {
                args.push("--keep-index");
            }
            assert!(git(twin.path(), &args).status.success());
            assert!(twin.path().join("vendor/lib/.git").is_dir());
            let twin_stash = twin_repo.refname_to_id("refs/stash").unwrap();
            assert_eq!(untracked_files_of(&twin_repo, twin_stash), expected);

            drop(inner);
            assert_eq!(
                git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap(),
                StashApplyOutcome::Applied
            );
            assert_eq!(read(dir.path(), "a.txt"), "edited\n");
            if other {
                assert_eq!(read(dir.path(), "vendor/other.txt"), "other\n");
            }
            assert!(dir.path().join("vendor/lib/.git").is_dir());
        }
    }
}

/// HEAD has `a.txt` (edited in the worktree) and ignores `*.o` and `build/`; the untracked folder
/// `vendor/` holds the untracked `vendor/other.txt`, the ignored `vendor/x.o` and the ignored
/// folder `vendor/build/`.
fn ignored_in_untracked_folder() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), ".gitignore", "*.o\nbuild/\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "vendor/other.txt", "other\n");
    write_file(dir.path(), "vendor/x.o", "object\n");
    write_file(dir.path(), "vendor/build/out", "built\n");
    write_file(dir.path(), "a.txt", "edited\n");
    (dir, repo)
}

#[test]
fn stash_all_with_untracked_files_keeps_ignored_files_in_an_untracked_folder() {
    // libgit2's REMOVE_UNTRACKED removes the untracked `vendor/` whole, ignored files
    // included. git's `stash -u` keeps them.
    for keep_index in [false, true] {
        let (dir, mut repo) = ignored_in_untracked_folder();

        git_core::stash::save_stash(&mut repo, &with(|o| o.keep_index = keep_index)).unwrap();

        assert_eq!(read(dir.path(), "vendor/x.o"), "object\n", "{keep_index}");
        assert_eq!(read(dir.path(), "vendor/build/out"), "built\n");
        assert!(!dir.path().join("vendor/other.txt").exists());
        assert_eq!(read(dir.path(), "a.txt"), "a\n");

        // The stash holds what git's does, and git keeps the ignored files too.
        let stash = stash_commit(&mut repo, 0);
        assert_eq!(untracked_files_of(&repo, stash), ["vendor/other.txt"]);
        let (twin, twin_repo) = ignored_in_untracked_folder();
        let mut args = vec!["stash", "push", "-u"];
        if keep_index {
            args.push("--keep-index");
        }
        assert!(git(twin.path(), &args).status.success());
        assert_eq!(read(twin.path(), "vendor/x.o"), "object\n");
        assert_eq!(read(twin.path(), "vendor/build/out"), "built\n");
        let twin_stash = twin_repo.refname_to_id("refs/stash").unwrap();
        assert_eq!(
            untracked_files_of(&twin_repo, twin_stash),
            ["vendor/other.txt"]
        );

        assert_eq!(
            git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap(),
            StashApplyOutcome::Applied
        );
        assert_eq!(read(dir.path(), "vendor/other.txt"), "other\n");
        assert_eq!(read(dir.path(), "a.txt"), "edited\n");
    }
}

/// HEAD has `.env` = "base\n" and `a.txt`; `.env` was removed from the index (`git rm --cached`),
/// is ignored, and was edited to "local secret\n".
fn ignored_file_removed_from_the_index() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), ".env", "base\n");
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new(".env")).unwrap();
    index.write().unwrap();
    write_file(dir.path(), ".git/info/exclude", ".env\n");
    write_file(dir.path(), ".env", "local secret\n");
    (dir, repo)
}

#[test]
fn stash_all_refuses_to_overwrite_an_ignored_file_removed_from_the_index() {
    // Status `INDEX_DELETED | IGNORED`: libgit2's stash doesn't record `.env`, its reset writes
    // HEAD's `.env` over the edit, and Pop deletes it.
    for opts in whole_stash_modes() {
        let (dir, mut repo) = ignored_file_removed_from_the_index();
        let status_before = git_core::status::status(&repo).unwrap();

        let error = git_core::stash::save_stash(&mut repo, &opts).unwrap_err();

        assert_ignored_overwritten(&mut repo, &opts, error, ".env", &status_before);
        assert_eq!(read(dir.path(), ".env"), "local secret\n");
    }
    // git records the edit in the stash and its pop brings it back.
    let (twin, _twin_repo) = ignored_file_removed_from_the_index();
    assert!(git_stash_push(twin.path(), false));
    assert_eq!(read(twin.path(), ".env"), "base\n");
    assert!(git(twin.path(), &["stash", "pop"]).status.success());
    assert_eq!(read(twin.path(), ".env"), "local secret\n");
}

/// The reset would write the committed version over the ignored `path`, which the index no
/// longer has; Browsitory refuses and writes nothing.
fn assert_ignored_overwritten(
    repo: &mut git2::Repository,
    opts: &StashOptions,
    error: StashError,
    path: &str,
    status_before: &[git_core::status::StatusEntry],
) {
    assert!(
        matches!(error, StashError::IgnoredOverwritten(ref p) if p == path),
        "{opts:?}: {error:?}"
    );
    assert_nothing_stashed(
        repo,
        &error,
        &format!(
            "Stashing would overwrite the ignored '{path}' with its committed version. Commit the removal of '{path}' first, or move it."
        ),
        status_before,
    );
}

/// HEAD has `build/a` and `build/b`; `build` was removed from the index (`git rm -r --cached`),
/// is ignored as a folder, and `build/a` was edited to "local\n".
fn ignored_folder_removed_from_the_index() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "build/a", "a\n");
    write_file(dir.path(), "build/b", "b\n");
    write_file(dir.path(), "keep.txt", "k\n");
    commit_all(&repo, "base");
    let mut index = repo.index().unwrap();
    index.remove_dir(std::path::Path::new("build"), 0).unwrap();
    index.write().unwrap();
    write_file(dir.path(), ".git/info/exclude", "build/\n");
    write_file(dir.path(), "build/a", "local\n");
    (dir, repo)
}

#[test]
fn stash_all_refuses_to_overwrite_an_ignored_folder_removed_from_the_index() {
    // Only the folder entry `build/` is reported as ignored; its files show up as plain
    // `INDEX_DELETED`. The reset wrote HEAD's `build/a` over the edit (or, with keep-index,
    // deleted `build/`).
    for opts in whole_stash_modes() {
        let (dir, mut repo) = ignored_folder_removed_from_the_index();
        let status_before = git_core::status::status(&repo).unwrap();

        let error = git_core::stash::save_stash(&mut repo, &opts).unwrap_err();

        assert_ignored_overwritten(&mut repo, &opts, error, "build", &status_before);
        assert_eq!(read(dir.path(), "build/a"), "local\n");
        assert_eq!(read(dir.path(), "build/b"), "b\n");
    }
    // git records the edit in the stash and its pop brings it back.
    let (twin, _twin_repo) = ignored_folder_removed_from_the_index();
    assert!(git_stash_push(twin.path(), false));
    assert_eq!(read(twin.path(), "build/a"), "a\n");
    assert!(git(twin.path(), &["stash", "pop"]).status.success());
    assert_eq!(read(twin.path(), "build/a"), "local\n");
}

/// HEAD has the file `d`, replaced by a folder with the untracked `d/x`; the new folder
/// `newpkg/` holds the ignored `newpkg/node_modules/` and, with `source`, the untracked
/// `newpkg/index.js`.
fn swap_next_to_an_untracked_package(source: bool) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = file_replaced_by_folder();
    write_file(dir.path(), ".git/info/exclude", "node_modules/\n");
    write_file(dir.path(), "newpkg/node_modules/pkg/i.js", "dependency\n");
    if source {
        write_file(dir.path(), "newpkg/index.js", "source\n");
    }
    (dir, repo)
}

#[test]
fn an_untracked_folder_holding_only_ignored_files_keeps_the_whole_stash() {
    // libgit2 doesn't remove an untracked folder that holds only ignored files, so Stash all
    // stays on libgit2's stash and handles the swap of `d`.
    let (dir, mut repo) = swap_next_to_an_untracked_package(false);

    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

    assert_eq!(read(dir.path(), "d"), "f\n");
    assert_eq!(
        read(dir.path(), "newpkg/node_modules/pkg/i.js"),
        "dependency\n"
    );
    let stash = stash_commit(&mut repo, 0);
    assert_eq!(untracked_files_of(&repo, stash), ["d/x"]);
}

#[test]
fn a_swap_next_to_ignored_files_in_an_untracked_folder_is_refused_as_on_the_path_limited_route() {
    // Keeping `node_modules/` needs the path-limited route, which can't stash the folder that
    // replaced the file `d`: it refuses, and nothing is written.
    let (dir, mut repo) = swap_next_to_an_untracked_package(true);
    let status_before = git_core::status::status(&repo).unwrap();

    let error = git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap_err();

    assert!(
        matches!(error, StashError::DirectoryReplacedNonDirectory { ref path, .. } if path == "d"),
        "{error:?}"
    );
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
    assert_eq!(git_core::status::status(&repo).unwrap(), status_before);
    assert_eq!(read(dir.path(), "d/x"), "x\n");
    assert_eq!(read(dir.path(), "newpkg/index.js"), "source\n");
    assert_eq!(
        read(dir.path(), "newpkg/node_modules/pkg/i.js"),
        "dependency\n"
    );
}

#[test]
fn a_staged_deletion_replaced_by_a_folder_is_not_an_ignored_file() {
    // `d` deleted from the index and the worktree, then a folder `d/x` created: `d` is
    // `INDEX_DELETED` with a folder at its path, nothing ignored is overwritten.
    let (dir, mut repo) = file_replaced_by_folder();
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new("d")).unwrap();
    index.write().unwrap();
    let status_before = git_core::status::status(&repo).unwrap();

    let result = git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = false));

    assert_untracked_in_the_way(&mut repo, result, "d/x", "d", &status_before);
    assert_eq!(read(dir.path(), "d/x"), "x\n");
}

/// HEAD has `path` = "head\n" and `a`; `path` was removed from the index (`git rm --cached`,
/// not ignored), then edited to "local\n" (#247). `path` is `d/f` (the folder form: status
/// without recursion reports `d/` untracked) or the top-level `f`.
fn removed_from_index(path: &str) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), path, "head\n");
    write_file(dir.path(), "a", "a\n");
    commit_all(&repo, "base");
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new(path)).unwrap();
    index.write().unwrap();
    write_file(dir.path(), path, "local\n");
    (dir, repo)
}

/// The same with the git CLI: `git rm -r -q --cached <path>`, then the edit.
fn removed_from_index_twin(path: &str) -> tempfile::TempDir {
    let (dir, repo) = init_repo();
    write_file(dir.path(), path, "head\n");
    write_file(dir.path(), "a", "a\n");
    commit_all(&repo, "base");
    assert!(git(dir.path(), &["rm", "-r", "-q", "--cached", path])
        .status
        .success());
    write_file(dir.path(), path, "local\n");
    dir
}

/// The content of `path` in `tree`, if it has it.
fn blob_in(repo: &git2::Repository, tree: &git2::Tree, path: &str) -> Option<String> {
    let entry = tree.get_path(std::path::Path::new(path)).ok()?;
    let blob = repo.find_blob(entry.id()).ok()?;
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

/// What `stash@{0}` holds for `path`: in its worktree tree, its index commit and its untracked
/// commit.
fn stashed(repo: &mut git2::Repository, path: &str) -> [Option<String>; 3] {
    let id = stash_commit(repo, 0);
    let stash = repo.find_commit(id).unwrap();
    let worktree = blob_in(repo, &stash.tree().unwrap(), path);
    let index = blob_in(repo, &stash.parent(1).unwrap().tree().unwrap(), path);
    let untracked = stash
        .parent(2)
        .ok()
        .and_then(|commit| blob_in(repo, &commit.tree().unwrap(), path));
    [worktree, index, untracked]
}

/// `git stash push [-u] [-k]` in `dir`; `git show stash@{0}:<path>`.
fn git_stash_push_shows(dir: &std::path::Path, opts: &StashOptions, path: &str) -> String {
    let mut args = vec!["stash", "push"];
    if opts.include_untracked {
        args.push("-u");
    }
    if opts.keep_index {
        args.push("-k");
    }
    assert!(git(dir, &args).status.success());
    String::from_utf8_lossy(&git(dir, &["show", &format!("stash@{{0}}:{path}")]).stdout)
        .into_owned()
}

/// The content of `path` in the index on disk, if it has it.
fn in_index(repo: &git2::Repository, path: &str) -> Option<String> {
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    let entry = index.get_path(std::path::Path::new(path), 0)?;
    let blob = repo.find_blob(entry.id).unwrap();
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

/// `git status --porcelain` in `dir`.
fn git_status(dir: &std::path::Path) -> String {
    String::from_utf8_lossy(&git(dir, &["status", "--porcelain"]).stdout).into_owned()
}

/// Both forms of #247: a file in a folder, and a top-level file.
const REMOVED_FROM_INDEX: [&str; 2] = ["d/f", "f"];

#[test]
fn stash_all_keeps_an_edit_in_a_folder_removed_from_the_index() {
    // #247: libgit2 missed the edit in a folder (and recorded a top-level one twice with
    // untracked files). The stash now records it once, in its worktree tree, and puts the
    // committed version back in the worktree and the index; Pop brings the edit back.
    for path in REMOVED_FROM_INDEX {
        for include_untracked in [false, true] {
            let opts = with(|o| o.include_untracked = include_untracked);
            let (dir, mut repo) = removed_from_index(path);
            // The rest is stashed as usual.
            write_file(dir.path(), "a", "a2\n");
            write_file(dir.path(), "u", "u\n");

            git_core::stash::save_stash(&mut repo, &opts).unwrap();

            assert_eq!(
                stashed(&mut repo, path),
                [Some("local\n".into()), None, None],
                "{path} {opts:?}"
            );
            assert_eq!(read(dir.path(), path), "head\n");
            assert_eq!(in_index(&repo, path).as_deref(), Some("head\n"));
            assert_eq!(read(dir.path(), "a"), "a\n");
            assert_eq!(dir.path().join("u").exists(), !include_untracked);
            if include_untracked {
                assert!(git_core::status::status(&repo).unwrap().is_empty());
            } else {
                assert_eq!(git_status(dir.path()), "?? u\n");
            }

            assert_eq!(
                git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap(),
                StashApplyOutcome::Applied,
                "{path} {opts:?}"
            );
            assert_eq!(read(dir.path(), path), "local\n");
            assert_eq!(in_index(&repo, path).as_deref(), Some("head\n"));
            assert_eq!(read(dir.path(), "a"), "a2\n");
            assert_eq!(read(dir.path(), "u"), "u\n");
            assert_eq!(git_status(dir.path()), format!(" M a\n M {path}\n?? u\n"));
            assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
            // git records the edit too; with -u also in its untracked commit, and then its own
            // pop fails ("could not restore untracked files"), keeping the edit and the stash.
            let twin = removed_from_index_twin(path);
            assert_eq!(git_stash_push_shows(twin.path(), &opts, path), "local\n");
            assert_eq!(read(twin.path(), path), "head\n");
            let popped = git(twin.path(), &["stash", "pop"]).status.success();
            assert_eq!(popped, !include_untracked, "{path} {opts:?}");
            assert_eq!(read(twin.path(), path), "local\n");
        }
    }
}

#[test]
fn stash_all_keeps_an_edit_removed_from_the_index_and_restores_the_removal_with_restore_staged_changes(
) {
    for path in REMOVED_FROM_INDEX {
        for include_untracked in [false, true] {
            let opts = with(|o| o.include_untracked = include_untracked);
            let (dir, mut repo) = removed_from_index(path);
            git_core::stash::save_stash(&mut repo, &opts).unwrap();

            assert_eq!(
                git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap(),
                StashApplyOutcome::Applied,
                "{path} {opts:?}"
            );

            assert_eq!(read(dir.path(), path), "local\n");
            assert_eq!(in_index(&repo, path), None);
            assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
        }
    }
}

#[test]
fn stash_all_with_keep_index_deletes_an_edit_removed_from_the_index_and_keeps_it_in_the_stash() {
    // Keep staged changes leaves the index's removal in place, so the file goes, as with git;
    // the edit is in the stash. Pop refuses while the removal is still staged and keeps the
    // stash; once it is unstaged, Pop brings the edit back.
    for path in REMOVED_FROM_INDEX {
        for include_untracked in [false, true] {
            let opts = with(|o| {
                o.include_untracked = include_untracked;
                o.keep_index = true;
            });
            let (dir, mut repo) = removed_from_index(path);

            git_core::stash::save_stash(&mut repo, &opts).unwrap();

            assert!(!dir.path().join(path).exists(), "{path} {opts:?}");
            assert_eq!(in_index(&repo, path), None);
            let twin = removed_from_index_twin(path);
            assert_eq!(git_stash_push_shows(twin.path(), &opts, path), "local\n");
            assert!(!twin.path().join(path).exists());
            assert_eq!(
                stashed(&mut repo, path),
                [Some("local\n".into()), None, None],
                "{path} {opts:?}"
            );
            let error = git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap_err();
            assert!(matches!(error, StashError::StagedChanges), "{error:?}");
            assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);

            {
                let head = repo.head().unwrap().peel_to_commit().unwrap();
                repo.reset_default(Some(head.as_object()), [path]).unwrap();
            }
            // Unstaged, the file is still deleted in the worktree: libgit2's safe checkout
            // won't write over a deletion, so Pop refuses and keeps the stash.
            let error = git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap_err();
            assert!(matches!(error, StashError::WouldOverwrite), "{error:?}");
            assert_eq!(
                stashed(&mut repo, path),
                [Some("local\n".into()), None, None]
            );
            // Discarding the deletion too (the committed version back), Pop brings the edit back.
            let mut checkout = git2::build::CheckoutBuilder::new();
            checkout.force().path(path);
            repo.checkout_head(Some(&mut checkout)).unwrap();
            assert_eq!(
                git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap(),
                StashApplyOutcome::Applied,
                "{path} {opts:?}"
            );
            assert_eq!(read(dir.path(), path), "local\n");
            assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
        }
    }
}

#[test]
fn git_stash_pop_restores_a_stashed_edit_removed_from_the_index() {
    // Browsitory's stash pops in git too: plain and with --index (git's own stash with untracked
    // files can't, see #247).
    for path in REMOVED_FROM_INDEX {
        for index in [false, true] {
            let (dir, mut repo) = removed_from_index(path);
            git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

            let mut args = vec!["stash", "pop"];
            if index {
                args.push("--index");
            }
            let output = git(dir.path(), &args);

            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(read(dir.path(), path), "local\n");
            let untracked = if path == "d/f" { "d/" } else { path };
            let expected = if index {
                format!("D  {path}\n?? {untracked}\n")
            } else {
                format!(" M {path}\n")
            };
            assert_eq!(git_status(dir.path()), expected);
        }
    }
}

/// HEAD has `committed` (each = "head\n") and `a`; `removed` were removed from the index, and
/// `edited` then set to "local\n".
fn removed_paths(
    committed: &[&str],
    removed: &[&str],
    edited: &[&str],
) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    for path in committed {
        write_file(dir.path(), path, "head\n");
    }
    write_file(dir.path(), "a", "a\n");
    commit_all(&repo, "base");
    let mut index = repo.index().unwrap();
    for path in removed {
        index.remove_path(std::path::Path::new(path)).unwrap();
    }
    index.write().unwrap();
    for path in edited {
        write_file(dir.path(), path, "local\n");
    }
    (dir, repo)
}

#[test]
fn stash_all_puts_back_the_index_entries_of_a_folder_removed_from_the_index() {
    // #247 review: `git rm -r --cached d` with only `d/f` edited. The revert's checkout writes
    // `d/f` (and adds it back to the index) but not the unchanged `d/g`, which stayed removed:
    // status kept `D  d/g` and Pop refused with `StagedChanges`.
    for include_untracked in [false, true] {
        for restore_index in [false, true] {
            let opts = with(|o| o.include_untracked = include_untracked);
            let (dir, mut repo) = removed_paths(&["d/f", "d/g"], &["d/f", "d/g"], &["d/f"]);

            git_core::stash::save_stash(&mut repo, &opts).unwrap();

            assert!(
                git_core::status::status(&repo).unwrap().is_empty(),
                "{opts:?}: {}",
                git_status(dir.path())
            );
            assert_eq!(in_index(&repo, "d/f").as_deref(), Some("head\n"));
            assert_eq!(in_index(&repo, "d/g").as_deref(), Some("head\n"));
            assert_eq!(
                git_core::stash::pop_stash(&mut repo, 0, None, restore_index).unwrap(),
                StashApplyOutcome::Applied,
                "{opts:?} restore_index={restore_index}"
            );
            assert_eq!(read(dir.path(), "d/f"), "local\n");
            assert_eq!(read(dir.path(), "d/g"), "head\n");
            if restore_index {
                assert_eq!(in_index(&repo, "d/f"), None);
                assert_eq!(in_index(&repo, "d/g"), None);
            } else {
                assert_eq!(git_status(dir.path()), " M d/f\n");
            }
        }
    }
}

#[test]
fn stash_all_of_an_unedited_file_removed_from_the_index_puts_its_entry_back() {
    // #248: `git rm --cached f` alone. The stash holds the removal; the index gets `f` back, so
    // Pop isn't refused, and "Restore staged changes" removes it again.
    for include_untracked in [false, true] {
        for restore_index in [false, true] {
            let opts = with(|o| o.include_untracked = include_untracked);
            let (dir, mut repo) = removed_paths(&["f"], &["f"], &[]);

            git_core::stash::save_stash(&mut repo, &opts).unwrap();

            assert!(
                git_core::status::status(&repo).unwrap().is_empty(),
                "{opts:?}: {}",
                git_status(dir.path())
            );
            assert_eq!(in_index(&repo, "f").as_deref(), Some("head\n"));
            assert_eq!(
                git_core::stash::pop_stash(&mut repo, 0, None, restore_index).unwrap(),
                StashApplyOutcome::Applied,
                "{opts:?} restore_index={restore_index}"
            );
            assert_eq!(read(dir.path(), "f"), "head\n");
            assert_eq!(in_index(&repo, "f").is_none(), restore_index);
        }
    }
}

#[test]
fn stashing_a_file_removed_from_the_index_puts_its_entry_back() {
    // #248: Stash file… on a file removed from the index, edited or not, and on a folder of
    // them. Other changes stay as they are.
    let cases: [(&[&str], &[&str], &str); 3] = [
        (&["f"], &["f"], "f"),
        (&[], &["f"], "f"),
        (&["d/f"], &["d/f", "d/g"], "d"),
    ];
    for (edited, removed, picked) in cases {
        for restore_index in [false, true] {
            let (dir, mut repo) = removed_paths(&["f", "d/f", "d/g"], removed, edited);
            write_file(dir.path(), "a", "a2\n");
            let opts = with(|o| o.paths = vec![picked.into()]);

            git_core::stash::save_stash(&mut repo, &opts).unwrap();

            assert_eq!(git_status(dir.path()), " M a\n", "{picked} {edited:?}");
            for path in removed {
                assert_eq!(in_index(&repo, path).as_deref(), Some("head\n"));
            }
            // Pop needs a clean index; the unrelated edit to `a` is only in the worktree.
            assert_eq!(
                git_core::stash::pop_stash(&mut repo, 0, None, restore_index).unwrap(),
                StashApplyOutcome::Applied,
                "{picked} {edited:?} restore_index={restore_index}"
            );
            for path in edited {
                assert_eq!(read(dir.path(), path), "local\n");
            }
            for path in removed {
                assert_eq!(in_index(&repo, path).is_none(), restore_index, "{path}");
            }
            assert_eq!(read(dir.path(), "a"), "a2\n");
        }
    }
}

#[test]
fn stash_all_with_a_file_removed_from_the_index_refuses_a_folder_that_replaced_a_file() {
    // Known limitation of the #247 route: the path-limited stash refuses a folder that replaced
    // a tracked file, which libgit2's whole stash would have stashed. Nothing is written.
    let (dir, mut repo) = removed_paths(&["f", "t"], &["f"], &["f"]);
    std::fs::remove_file(dir.path().join("t")).unwrap();
    write_file(dir.path(), "t/x", "x\n");
    let status_before = git_core::status::status(&repo).unwrap();

    let error = git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap_err();

    assert!(
        matches!(error, StashError::DirectoryReplacedNonDirectory { ref path, .. } if path == "t"),
        "{error:?}"
    );
    assert_nothing_stashed(
        &mut repo,
        &error,
        "Can't stash 't': it was a file and is now a directory. Commit or move its contents first.",
        &status_before,
    );
    assert_eq!(read(dir.path(), "f"), "local\n");
    assert_eq!(read(dir.path(), "t/x"), "x\n");
}

/// A merge stopped on a conflict in `c` (status `UU c`); HEAD also has `f` = "head\n". With
/// `remove_f`, `f` was then removed from the index (`git rm --cached f`).
fn merge_conflict(remove_f: bool) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "c", "base\n");
    write_file(dir.path(), "f", "head\n");
    commit_all(&repo, "base");
    let run = |args: &[&str]| {
        let output = git(dir.path(), args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["checkout", "-q", "-b", "other"]);
    write_file(dir.path(), "c", "other\n");
    run(&["commit", "-q", "-am", "other"]);
    run(&["checkout", "-q", "-"]);
    write_file(dir.path(), "c", "main\n");
    run(&["commit", "-q", "-am", "main"]);
    assert!(!git(dir.path(), &["merge", "-q", "other"]).status.success());
    if remove_f {
        run(&["rm", "-q", "--cached", "f"]);
    }
    (dir, repo)
}

#[test]
fn stashing_during_a_merge_conflict_is_refused() {
    // #255 review: with a file removed from the index, Stash all took the path-limited route,
    // which flattened the conflict to HEAD's entry and left MERGE_HEAD, so the next commit
    // dropped the other side. git and libgit2 refuse ("cannot create a tree from a not fully
    // merged index"); Browsitory now refuses first, on every route, Stash file… included.
    let cases: [(bool, Vec<String>); 4] = [
        (true, vec![]),
        (false, vec![]),
        (false, vec!["c".into()]),
        (true, vec!["f".into()]),
    ];
    for (remove_f, paths) in cases {
        let (dir, mut repo) = merge_conflict(remove_f);
        let status_before = git_status(dir.path());
        let conflicted = read(dir.path(), "c");

        let error =
            git_core::stash::save_stash(&mut repo, &with(|o| o.paths = paths.clone())).unwrap_err();

        assert!(
            matches!(error, StashError::ConflictsBeforeStash),
            "{remove_f} {paths:?}: {error:?}"
        );
        assert_eq!(
            error.to_string(),
            "Resolve the current conflicts before stashing."
        );
        assert!(repo.find_reference("refs/stash").is_err());
        assert_eq!(git_status(dir.path()), status_before);
        assert_eq!(read(dir.path(), "c"), conflicted);
        assert!(repo.path().join("MERGE_HEAD").exists());
    }
    // git refuses too.
    let (twin, _repo) = merge_conflict(true);
    assert!(!git(twin.path(), &["stash", "push"]).status.success());
}

#[test]
fn stash_all_reverting_a_path_that_head_has_as_a_folder_keeps_the_index_valid() {
    // #255 review: HEAD has `t/y`; the index has the staged file `t` instead, and `t` is gone
    // from the worktree. With a file removed from the index (the path-limited route) the revert
    // must not put HEAD's folder `t` into the index as an entry ("invalid entry mode").
    let (dir, mut repo) = removed_paths(&["f", "t/y"], &["f", "t/y"], &["f"]);
    {
        let mut index = repo.index().unwrap();
        let blob = repo.blob(b"staged\n").unwrap();
        let mut entry = index.get_path(std::path::Path::new("a"), 0).unwrap();
        entry.id = blob;
        entry.path = b"t".to_vec();
        entry.flags = 1;
        index.add(&entry).unwrap();
        index.write().unwrap();
    }
    std::fs::remove_dir_all(dir.path().join("t")).unwrap();

    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

    assert_eq!(in_index(&repo, "t/y").as_deref(), Some("head\n"));
    assert_eq!(in_index(&repo, "t"), None);
    assert_eq!(read(dir.path(), "t/y"), "head\n");
    assert!(
        git_core::status::status(&repo).unwrap().is_empty(),
        "{}",
        git_status(dir.path())
    );
    assert_eq!(
        git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap(),
        StashApplyOutcome::Applied
    );
    assert_eq!(read(dir.path(), "f"), "local\n");
}

/// HEAD has `old` (four lines) and `other`; `old` was renamed to `new` and the rename staged
/// (`git mv old new`), and `other` was edited.
fn staged_rename() -> (tempfile::TempDir, git2::Repository) {
    staged_rename_of("old", "new")
}

/// The same with the rename `old` → `new` (folders created as needed).
fn staged_rename_of(old: &str, new: &str) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), old, "a\nb\nc\nd\n");
    write_file(dir.path(), "other", "x\n");
    commit_all(&repo, "base");
    if let Some((folder, _)) = new.rsplit_once('/') {
        std::fs::create_dir_all(dir.path().join(folder)).unwrap();
    }
    assert!(git(dir.path(), &["mv", old, new]).status.success());
    // A different size: an edit of the same size within the commit's timestamp tick can look
    // unchanged to `git status` once libgit2 has rewritten the index (racy stat data).
    write_file(dir.path(), "other", "changed\n");
    (dir, repo)
}

#[test]
fn stashing_either_side_of_a_staged_rename_stashes_the_whole_rename() {
    // #152: Stash file… sends the rename's new path. The old path's deletion goes into the
    // stash too, so nothing of the rename stays staged. (git's `stash push -- new` leaves `D
    // old` staged, and `stash push -- old new` fails: `old` matches no file git knows.)
    const CONTENT: &str = "a\nb\nc\nd\n";
    for picked in ["new", "old"] {
        for restore_index in [false, true] {
            let (dir, mut repo) = staged_rename();
            let opts = with(|o| o.paths = vec![picked.into()]);

            git_core::stash::save_stash(&mut repo, &opts).unwrap();

            assert_eq!(git_status(dir.path()), " M other\n", "{picked}");
            assert_eq!(read(dir.path(), "old"), CONTENT);
            assert!(!dir.path().join("new").exists());
            assert_eq!(
                stashed(&mut repo, "new"),
                [Some(CONTENT.into()), Some(CONTENT.into()), None]
            );
            assert_eq!(stashed(&mut repo, "old"), [None, None, None]);

            assert_eq!(
                git_core::stash::pop_stash(&mut repo, 0, None, restore_index).unwrap(),
                StashApplyOutcome::Applied,
                "{picked} restore_index={restore_index}"
            );
            assert_eq!(read(dir.path(), "new"), CONTENT);
            assert!(!dir.path().join("old").exists());
            if restore_index {
                assert_eq!(git_status(dir.path()), "R  old -> new\n M other\n");
            }
        }
    }
    // git leaves the old path's deletion staged.
    let (twin, _repo) = staged_rename();
    assert!(git(twin.path(), &["stash", "push", "-q", "--", "new"])
        .status
        .success());
    assert_eq!(git_status(twin.path()), "D  old\n M other\n");
}

#[test]
fn stashing_an_ignored_file_removed_from_the_index_is_refused_like_stash_all() {
    // #259: `git rm --cached .env`, `.env` ignored, then edited. Stash file… said "There are no
    // changes to stash"; it refuses like Stash all (`IgnoredOverwritten`) and writes nothing.
    // git refuses too (`.env` matches no file it knows).
    let (dir, mut repo) = ignored_file_removed_from_the_index();
    let status_before = git_core::status::status(&repo).unwrap();
    for keep_index in [false, true] {
        let opts = with(|o| {
            o.paths = vec![".env".into()];
            o.keep_index = keep_index;
        });

        let error = git_core::stash::save_stash(&mut repo, &opts).unwrap_err();

        assert_ignored_overwritten(&mut repo, &opts, error, ".env", &status_before);
        assert_eq!(read(dir.path(), ".env"), "local secret\n");
    }
    let (twin, _repo) = ignored_file_removed_from_the_index();
    assert!(!git(twin.path(), &["stash", "push", "--", ".env"])
        .status
        .success());
    assert_eq!(read(twin.path(), ".env"), "local secret\n");
}

#[test]
fn stashing_a_file_in_an_ignored_folder_removed_from_the_index_is_refused_like_stash_all() {
    // #259, folder form: `git rm -r --cached build`, `build/` ignored, `build/a` edited.
    for picked in ["build/a", "build/b", "build"] {
        let (dir, mut repo) = ignored_folder_removed_from_the_index();
        let status_before = git_core::status::status(&repo).unwrap();
        let opts = with(|o| o.paths = vec![picked.into()]);

        let error = git_core::stash::save_stash(&mut repo, &opts).unwrap_err();

        assert_ignored_overwritten(&mut repo, &opts, error, "build", &status_before);
        assert_eq!(read(dir.path(), "build/a"), "local\n");
        assert_eq!(read(dir.path(), "build/b"), "b\n");
    }
    let (twin, _repo) = ignored_folder_removed_from_the_index();
    assert!(!git(twin.path(), &["stash", "push", "--", "build/a"])
        .status
        .success());
}

/// HEAD has `a.txt`; the repository `lib` (its own `.git` and one commit) was staged with `git
/// add lib` (a gitlink HEAD doesn't have), after `.gitmodules` registered it when `submodule`;
/// `a.txt` was then edited.
fn staged_repository(submodule: bool) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let lib = dir.path().join("lib");
    let inner = git2::Repository::init(&lib).unwrap();
    {
        let mut config = inner.config().unwrap();
        config.set_str("user.name", "Inner").unwrap();
        config.set_str("user.email", "inner@example.com").unwrap();
    }
    write_file(&lib, "l.txt", "l\n");
    commit_all(&inner, "inner");
    if submodule {
        write_file(
            dir.path(),
            ".gitmodules",
            "[submodule \"lib\"]\n\tpath = lib\n\turl = ./lib\n",
        );
        assert!(git(dir.path(), &["add", ".gitmodules"]).status.success());
    }
    assert!(git(dir.path(), &["add", "lib"]).status.success());
    write_file(dir.path(), "a.txt", "edited\n");
    (dir, repo)
}

/// Stash all, and Stash file… on `lib`, refuse the staged repository `lib` with `message`
/// (libgit2 failed with "cannot create blob from '…/lib': it is a directory") and write nothing.
fn assert_staged_repository_refused(submodule: bool, message: &str) {
    let mut modes = whole_stash_modes();
    modes.push(with(|o| o.paths = vec!["lib".into()]));
    modes.push(with(|o| o.paths = vec!["lib".into(), "a.txt".into()]));
    for opts in modes {
        let (dir, mut repo) = staged_repository(submodule);
        let status_before = git_core::status::status(&repo).unwrap();

        let error = git_core::stash::save_stash(&mut repo, &opts).unwrap_err();

        let expected = if submodule {
            matches!(error, StashError::NewSubmodule(ref path) if path == "lib")
        } else {
            matches!(error, StashError::EmbeddedRepository(ref path) if path == "lib")
        };
        assert!(expected, "{opts:?}: {error:?}");
        assert_nothing_stashed(&mut repo, &error, message, &status_before);
        assert_eq!(read(dir.path(), "a.txt"), "edited\n");
        assert_eq!(read(dir.path(), "lib/l.txt"), "l\n");
        assert!(dir.path().join("lib/.git").is_dir());
    }
}

#[test]
fn stashing_a_staged_embedded_repository_is_refused_with_a_clear_message() {
    // #244. git stashes the gitlink (and keeps the folder: "unable to rmdir 'lib'").
    assert_staged_repository_refused(
        false,
        "'lib' is a repository inside this one. Add it as a submodule or remove it from the index first.",
    );
    let (twin, _repo) = staged_repository(false);
    assert!(git(twin.path(), &["stash", "push", "-q"]).status.success());
    assert_eq!(read(twin.path(), "lib/l.txt"), "l\n");
}

#[test]
fn stashing_a_staged_new_submodule_is_refused_with_a_clear_message() {
    // #244: the same libgit2 failure for a submodule added but not committed yet.
    assert_staged_repository_refused(
        true,
        "Can't stash the new submodule 'lib'. Commit it first, or remove it from the index.",
    );
}

#[test]
fn stashing_another_file_next_to_a_staged_repository_still_works() {
    // Stash file… on `a.txt` doesn't touch `lib`, which stays staged.
    for submodule in [false, true] {
        let (dir, mut repo) = staged_repository(submodule);

        git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["a.txt".into()])).unwrap();

        assert_eq!(read(dir.path(), "a.txt"), "a\n");
        assert!(git_status(dir.path()).contains("A  lib\n"));
        assert_eq!(read(dir.path(), "lib/l.txt"), "l\n");
    }
}

#[test]
fn a_new_gitlink_without_its_repository_is_refused() {
    // #244 review: `git add lib`, then `lib` deleted (or its `.git`). libgit2's stash
    // succeeded but left `AD lib` staged, so Pop refused with "Commit or unstage your staged
    // changes". Browsitory refuses with a neutral message; nothing is written.
    let message = "Can't stash 'lib': the index links it to another repository, and that link isn't committed yet. Remove 'lib' from the index first.";
    for remove in ["lib", "lib/.git"] {
        let mut modes = whole_stash_modes();
        modes.push(with(|o| o.paths = vec!["lib".into()]));
        for opts in modes {
            let (dir, mut repo) = staged_repository(false);
            std::fs::remove_dir_all(dir.path().join(remove)).unwrap();
            let status_before = git_core::status::status(&repo).unwrap();

            let error = git_core::stash::save_stash(&mut repo, &opts).unwrap_err();

            assert!(
                matches!(error, StashError::NewGitlink(ref path) if path == "lib"),
                "{remove} {opts:?}: {error:?}"
            );
            assert_nothing_stashed(&mut repo, &error, message, &status_before);
            assert_eq!(read(dir.path(), "a.txt"), "edited\n");
        }
    }
}

/// The nested repository `lib`'s HEAD summary and the content of its `l.txt`.
fn nested_state(dir: &std::path::Path) -> (String, String) {
    let inner = git2::Repository::open(dir.join("lib")).unwrap();
    let head = inner.head().unwrap().peel_to_commit().unwrap();
    (
        head.summary()
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_string(),
        read(&dir.join("lib"), "l.txt"),
    )
}

#[test]
fn a_committed_gitlink_stashes_and_leaves_the_nested_repository_alone() {
    // Behind the #244 claim "a gitlink HEAD already has, changed or not, stashes fine": HEAD has
    // `lib` at `inner`; the nested repository then committed `inner2` and has an unsaved edit.
    // The stash records the outer gitlink only; it never moves the nested repository's HEAD or
    // touches its files, before or after Pop (git doesn't recurse into it either).
    for staged in [false, true] {
        for keep_index in [false, true] {
            let (dir, mut repo) = staged_repository(false);
            assert!(git(dir.path(), &["commit", "-qm", "lib"]).status.success());
            let inner = git2::Repository::open(dir.path().join("lib")).unwrap();
            write_file(&dir.path().join("lib"), "l.txt", "l2\n");
            commit_all(&inner, "inner2");
            write_file(&dir.path().join("lib"), "l.txt", "dirty\n");
            if staged {
                assert!(git(dir.path(), &["add", "lib"]).status.success());
            }
            let gitlink = if staged { "MM lib\n" } else { " M lib\n" };
            assert_eq!(git_status(dir.path()), format!(" M a.txt\n{gitlink}"));
            let opts = with(|o| o.keep_index = keep_index);

            git_core::stash::save_stash(&mut repo, &opts).unwrap();

            let label = format!("staged={staged} keep_index={keep_index}");
            assert_eq!(
                nested_state(dir.path()),
                ("inner2".into(), "dirty\n".into())
            );
            assert_eq!(read(dir.path(), "a.txt"), "a\n", "{label}");
            let after = if staged && keep_index {
                "MM lib\n"
            } else {
                " M lib\n"
            };
            assert_eq!(git_status(dir.path()), after, "{label}");
            if staged && keep_index {
                // Keep-index leaves the gitlink staged, so Pop waits for it, as for any file.
                continue;
            }
            assert_eq!(
                git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap(),
                StashApplyOutcome::Applied,
                "{label}"
            );
            assert_eq!(
                git_status(dir.path()),
                format!(" M a.txt\n{gitlink}"),
                "{label}"
            );
            assert_eq!(
                nested_state(dir.path()),
                ("inner2".into(), "dirty\n".into())
            );
        }
    }
}

/// HEAD has `a` and `committed` ("head\n" each); `removed` were removed from the index and
/// `edited` changed to "local\n"; `.git/info/exclude` holds `exclude`.
fn removed_with_ignore_rules(
    committed: &[&str],
    removed: &[&str],
    edited: &[&str],
    exclude: &str,
) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = removed_paths(committed, removed, edited);
    write_file(dir.path(), ".git/info/exclude", exclude);
    (dir, repo)
}

/// Stash file… on each of `picked`, with and without untracked files, refuses exactly when Stash
/// all does (every mode), naming the same entry (`refused`); otherwise each stash records every
/// edit and puts the committed version back.
fn assert_stash_file_refuses_like_stash_all(
    committed: &[&str],
    removed: &[&str],
    edited: &[&str],
    exclude: &str,
    picked: &[&str],
    refused: Option<&str>,
) {
    let mut modes = whole_stash_modes();
    for path in picked {
        for include_untracked in [false, true] {
            modes.push(with(|o| {
                o.paths = vec![path.to_string()];
                o.include_untracked = include_untracked;
            }));
        }
    }
    for opts in modes {
        let (dir, mut repo) = removed_with_ignore_rules(committed, removed, edited, exclude);
        let status_before = git_core::status::status(&repo).unwrap();

        let result = git_core::stash::save_stash(&mut repo, &opts);

        match refused {
            Some(name) => {
                let error = result.unwrap_err();
                // Status (index and worktree), `refs/stash` and the stash list unchanged.
                assert_ignored_overwritten(&mut repo, &opts, error, name, &status_before);
                for path in committed {
                    let expected = if edited.contains(path) {
                        "local\n"
                    } else {
                        "head\n"
                    };
                    assert_eq!(read(dir.path(), path), expected, "{opts:?}");
                    let in_index_now = in_index(&repo, path);
                    let expected_index = (!removed.contains(path)).then(|| "head\n".to_string());
                    assert_eq!(in_index_now, expected_index, "{path} {opts:?}");
                }
                assert_eq!(read(dir.path(), "a"), "a\n");
            }
            None => {
                assert!(result.is_ok(), "{exclude:?} {opts:?}: {result:?}");
                for path in edited {
                    assert_eq!(stashed(&mut repo, path)[0].as_deref(), Some("local\n"));
                }
            }
        }
    }
}

#[test]
fn stash_file_refuses_ignored_files_removed_from_the_index_by_the_same_rule_as_stash_all() {
    // #259 review: Stash file… and Stash all decide alike, from the status of the file.
    // A tracked file that matches an ignore rule is a plain change, never refused.
    assert_stash_file_refuses_like_stash_all(&[".env"], &[], &[".env"], ".env\n", &[".env"], None);
    // A `!` rule re-includes it: it is untracked, not ignored, and stashed as in #247.
    assert_stash_file_refuses_like_stash_all(
        &["keep.env"],
        &["keep.env"],
        &["keep.env"],
        "*.env\n!keep.env\n",
        &["keep.env"],
        None,
    );
    // A folder that holds a tracked file isn't ignored as a whole: the file itself is named.
    assert_stash_file_refuses_like_stash_all(
        &["build/t", "build/o"],
        &["build/o"],
        &["build/o"],
        "build/\n",
        &["build/o", "build"],
        Some("build/o"),
    );
    // A rule that ignores a parent folder: the folder is named.
    assert_stash_file_refuses_like_stash_all(
        &["d/sub/f"],
        &["d/sub/f"],
        &["d/sub/f"],
        "sub/\n",
        &["d/sub/f", "d"],
        Some("d/sub"),
    );
}

#[test]
fn stashing_a_rename_whose_old_path_exists_again_is_refused_unless_both_are_picked() {
    // #152 review: `git mv old new`, then a new file `old`. Picking only `new` would stash and
    // overwrite the unrelated `old`, even with untracked files left out: refused, nothing
    // written. Picking `old` stashes both, its new content included.
    for include_untracked in [false, true] {
        let (dir, mut repo) = staged_rename();
        write_file(dir.path(), "old", "recreated\n");
        let status_before = git_core::status::status(&repo).unwrap();
        let opts = with(|o| {
            o.paths = vec!["new".into()];
            o.include_untracked = include_untracked;
        });

        let error = git_core::stash::save_stash(&mut repo, &opts).unwrap_err();

        assert!(
            matches!(error, StashError::RenameSourceRecreated { ref path, ref old } if path == "new" && old == "old"),
            "{error:?}"
        );
        assert_nothing_stashed(
            &mut repo,
            &error,
            "Can't stash 'new' on its own: it was renamed from 'old', and 'old' exists again. Stash 'old' too, or move it first.",
            &status_before,
        );
        assert_eq!(read(dir.path(), "old"), "recreated\n");
    }
    let (dir, mut repo) = staged_rename();
    write_file(dir.path(), "old", "recreated\n");
    git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["old".into()])).unwrap();
    assert_eq!(git_status(dir.path()), " M other\n");
    assert_eq!(stashed(&mut repo, "old")[0].as_deref(), Some("recreated\n"));
    assert_eq!(
        git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap(),
        StashApplyOutcome::Applied
    );
    assert_eq!(read(dir.path(), "old"), "recreated\n");
    assert_eq!(read(dir.path(), "new"), "a\nb\nc\nd\n");
}

#[test]
fn stashing_a_case_only_rename_is_refused_where_names_fold() {
    // #152 review: HEAD `Old`, index `old` (`git mv Old old`) with `core.ignorecase` (always so
    // on macOS and Windows). libgit2's index then finds `Old` as `old`: the stash held both
    // names and the revert changed nothing, leaving a stash Pop refused. Refused, nothing
    // written. Without folding the rename is two different names and stashes normally.
    let setup = |ignore_case: bool| {
        let (dir, repo) = init_repo();
        repo.config()
            .unwrap()
            .set_bool("core.ignorecase", ignore_case)
            .unwrap();
        write_file(dir.path(), "Old", "a\nb\nc\nd\n");
        write_file(dir.path(), "other", "x\n");
        commit_all(&repo, "base");
        assert!(git(dir.path(), &["mv", "Old", "old"]).status.success());
        (dir, repo)
    };
    for picked in ["old", "Old"] {
        let (_dir, mut repo) = setup(true);
        let status_before = git_core::status::status(&repo).unwrap();

        let error =
            git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec![picked.into()]))
                .unwrap_err();

        assert!(
            matches!(error, StashError::CaseOnlyRename { .. }),
            "{picked}: {error:?}"
        );
        assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
        assert_eq!(git_core::status::status(&repo).unwrap(), status_before);
    }
    if !cfg!(any(target_os = "macos", windows)) {
        let (dir, mut repo) = setup(false);
        git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["old".into()])).unwrap();
        assert_eq!(top_level(dir.path()), ["Old", "other"]);
        assert!(git_status(dir.path()).is_empty());
    }
}

/// HEAD has `old` (four lines) and `other`, with `core.ignorecase` on; `git mv old new`, then
/// `other` edited.
fn rename_with_ignorecase(old: &str, new: &str) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    write_file(dir.path(), old, "a\nb\nc\nd\n");
    write_file(dir.path(), "other", "x\n");
    commit_all(&repo, "base");
    assert!(git(dir.path(), &["mv", old, new]).status.success());
    write_file(dir.path(), "other", "changed\n");
    (dir, repo)
}

#[test]
fn only_an_ascii_case_change_counts_as_a_case_only_rename() {
    // #152 review: libgit2's ignore-case index compares names with `strcasecmp` (ASCII letters
    // only, byte by byte), so only those renames are refused. `Straße` → `Strasse` and an NFC →
    // NFD change are two different names to libgit2 and stash normally.
    let (_dir, mut repo) = rename_with_ignorecase("README.md", "readme.MD");
    let error =
        git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["readme.MD".into()]))
            .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Can't stash 'readme.MD': it differs from 'README.md' only in case. Commit the rename first, or undo it."
    );
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());

    // APFS takes `Straße` and `Strasse`, and NFC and NFD spellings, for one file name (`git mv`
    // refuses there), so only elsewhere.
    let renames = if cfg!(target_os = "macos") {
        vec![]
    } else {
        vec![("Straße", "Strasse"), ("Caf\u{e9}", "Cafe\u{301}")]
    };
    for (old, new) in renames {
        let (dir, mut repo) = rename_with_ignorecase(old, new);

        git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec![new.into()])).unwrap();

        assert_eq!(git_status(dir.path()), " M other\n", "{old}");
        assert_eq!(read(dir.path(), old), "a\nb\nc\nd\n");
        assert!(!dir.path().join(new).exists(), "{new}");
        assert_eq!(
            git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap(),
            StashApplyOutcome::Applied
        );
        assert_eq!(read(dir.path(), new), "a\nb\nc\nd\n");
        assert!(!dir.path().join(old).exists(), "{old}");
    }
}

#[test]
fn a_case_only_rename_is_refused_before_the_ignored_file_check() {
    // #152 review: on a case-sensitive drive with `core.ignorecase`, an ignored file `Old` next
    // to the renamed `old` would make the staged deletion of `Old` look like an ignored file
    // removed from the index (`IgnoredOverwritten`); the case-only check comes first. (On macOS
    // and Windows the two names are one file, as CI showed.)
    if cfg!(any(target_os = "macos", windows)) {
        return;
    }
    let (dir, mut repo) = rename_with_ignorecase("Old", "old");
    write_file(dir.path(), ".git/info/exclude", "Old\n");
    write_file(dir.path(), "Old", "ignored\n");
    let status_before = git_core::status::status(&repo).unwrap();

    let error = git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["old".into()]))
        .unwrap_err();

    assert!(
        matches!(error, StashError::CaseOnlyRename { ref path, ref other } if path == "old" && other == "Old"),
        "{error:?}"
    );
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
    assert_eq!(git_core::status::status(&repo).unwrap(), status_before);
    assert_eq!(read(dir.path(), "Old"), "ignored\n");
}

#[test]
fn stash_all_stashes_a_rename_whose_old_path_exists_again() {
    // #152 review: Stash all takes the path-limited route here (the new `old` is a file removed
    // from the index, #247) and selects every change, so the rename isn't picked "on its own":
    // the new `old` is stashed like any edit to a file removed from the index and comes back
    // on Pop.
    for opts in whole_stash_modes() {
        let (dir, mut repo) = staged_rename();
        write_file(dir.path(), "old", "recreated\n");

        git_core::stash::save_stash(&mut repo, &opts).unwrap();

        assert_eq!(
            stashed(&mut repo, "old")[0].as_deref(),
            Some("recreated\n"),
            "{opts:?}"
        );
        assert_eq!(
            stashed(&mut repo, "new")[0].as_deref(),
            Some("a\nb\nc\nd\n"),
            "{opts:?}"
        );
        if opts.keep_index {
            continue;
        }
        assert!(git_status(dir.path()).is_empty(), "{opts:?}");
        assert_eq!(
            git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap(),
            StashApplyOutcome::Applied,
            "{opts:?}"
        );
        assert_eq!(read(dir.path(), "old"), "recreated\n");
        assert_eq!(read(dir.path(), "new"), "a\nb\nc\nd\n");
        assert_eq!(read(dir.path(), "other"), "changed\n");
    }
}

#[test]
fn stashing_a_rename_with_keep_index_keeps_it_staged_and_in_the_worktree() {
    let (dir, mut repo) = staged_rename();
    let opts = with(|o| {
        o.paths = vec!["new".into()];
        o.keep_index = true;
    });

    git_core::stash::save_stash(&mut repo, &opts).unwrap();

    assert_eq!(git_status(dir.path()), "R  old -> new\n M other\n");
    assert_eq!(read(dir.path(), "new"), "a\nb\nc\nd\n");
    assert!(!dir.path().join("old").exists());
    assert_eq!(
        stashed(&mut repo, "new")[1].as_deref(),
        Some("a\nb\nc\nd\n")
    );
    assert_eq!(stashed(&mut repo, "old"), [None, None, None]);
}

#[test]
fn stashing_a_renamed_file_edited_in_the_worktree_keeps_both_versions() {
    // `RM`: the staged rename plus an unstaged edit of `new`.
    for restore_index in [false, true] {
        let (dir, mut repo) = staged_rename();
        write_file(dir.path(), "new", "a\nb\nc\nd\ne\n");

        git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["new".into()])).unwrap();

        assert_eq!(git_status(dir.path()), " M other\n");
        assert_eq!(
            stashed(&mut repo, "new"),
            [
                Some("a\nb\nc\nd\ne\n".into()),
                Some("a\nb\nc\nd\n".into()),
                None
            ]
        );
        assert_eq!(
            git_core::stash::pop_stash(&mut repo, 0, None, restore_index).unwrap(),
            StashApplyOutcome::Applied
        );
        assert_eq!(read(dir.path(), "new"), "a\nb\nc\nd\ne\n");
        assert!(!dir.path().join("old").exists());
        if restore_index {
            assert_eq!(git_status(dir.path()), "RM old -> new\n M other\n");
        }
    }
}

#[test]
fn stashing_a_folder_takes_the_renames_in_it_and_their_partners_outside_it() {
    // Inside the picked folder `d`, and from `x/old` (outside it) into it.
    for (old, new) in [("d/old", "d/new"), ("x/old", "d/new")] {
        let (dir, mut repo) = staged_rename_of(old, new);

        git_core::stash::save_stash(&mut repo, &with(|o| o.paths = vec!["d".into()])).unwrap();

        assert_eq!(git_status(dir.path()), " M other\n", "{old}");
        assert_eq!(read(dir.path(), old), "a\nb\nc\nd\n");
        assert!(!dir.path().join(new).exists());
        assert_eq!(
            git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap(),
            StashApplyOutcome::Applied
        );
        assert_eq!(
            git_status(dir.path()),
            format!("R  {old} -> {new}\n M other\n")
        );
    }
}

/// A repository with every kind of change a stash takes: an edited file, a staged new file, an
/// untracked file and (Unix) a tracked file replaced by a link to a file outside it, which sends
/// Stash all down the path-limited route (#114).
fn every_kind_of_change() -> (tempfile::TempDir, git2::Repository, tempfile::TempDir) {
    let out = common::outside_dir();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "tc", "plain\n");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "edited\n");
    write_file(dir.path(), "b.txt", "staged\n");
    stage(&repo, "b.txt");
    write_file(dir.path(), "u.txt", "untracked\n");
    #[cfg(unix)]
    common::replace_with_link(dir.path(), "tc", &out.path().join("target.txt"));
    (dir, repo, out)
}

/// What a refused operation must leave as it was, in [`untouched_state`].
#[derive(Debug, PartialEq)]
struct Untouched {
    /// Every worktree entry outside `.git`, a link as its target.
    entries: Vec<(String, String)>,
    index: Vec<u8>,
    /// What `refs/stash` resolves to.
    stash: Option<git2::Oid>,
    /// The bytes of `HEAD`, the branch it names, `refs/stash`, `packed-refs` and their reflogs
    /// (`None` when missing).
    refs: Vec<(String, Option<Vec<u8>>)>,
    /// The number of loose objects: no stash commit was written.
    objects: usize,
}

fn untouched_state(dir: &std::path::Path) -> Untouched {
    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if rel == ".git" {
                continue;
            }
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                let target = std::fs::read_link(&path).unwrap();
                out.push((rel, format!("link {}", target.display())));
            } else if meta.is_dir() {
                out.push((rel, "dir".into()));
                walk(root, &path, out);
            } else {
                out.push((rel, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let mut entries = Vec::new();
    walk(dir, dir, &mut entries);
    entries.sort();
    let git_dir = dir.join(".git");
    let objects = std::fs::read_dir(git_dir.join("objects"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.file_name().is_some_and(|name| name.len() == 2))
        .map(|path| std::fs::read_dir(path).unwrap().count())
        .sum();
    let head = std::fs::read_to_string(git_dir.join("HEAD")).unwrap();
    let mut names = vec![
        "HEAD".to_string(),
        "refs/stash".into(),
        "packed-refs".into(),
    ];
    if let Some(branch) = head.trim().strip_prefix("ref: ") {
        names.push(branch.to_string());
    }
    for name in names.clone() {
        names.push(format!("logs/{name}"));
    }
    let refs = names
        .into_iter()
        .map(|name| {
            let bytes = std::fs::read(git_dir.join(&name)).ok();
            (name, bytes)
        })
        .collect();
    Untouched {
        entries,
        index: std::fs::read(git_dir.join("index")).unwrap(),
        stash: git2::Repository::open(dir)
            .unwrap()
            .refname_to_id("refs/stash")
            .ok(),
        refs,
        objects,
    }
}

/// Sets `core.ignorecase` in the repository's own config to `value`, or removes it.
fn set_ignorecase(repo: &git2::Repository, value: Option<&str>) {
    let mut config = repo
        .config()
        .unwrap()
        .open_level(git2::ConfigLevel::Local)
        .unwrap();
    match value {
        Some(value) => config.set_str("core.ignorecase", value).unwrap(),
        None => {
            let _ = config.remove("core.ignorecase");
        }
    }
}

/// Every stash route: each whole-stash mode, and Stash file… on each kind of change, with and
/// without keep-index.
fn every_stash_route() -> Vec<StashOptions> {
    let mut routes = whole_stash_modes();
    let mut picked = vec!["a.txt", "b.txt", "u.txt"];
    if cfg!(unix) {
        picked.push("tc");
    }
    for path in picked {
        for keep_index in [false, true] {
            routes.push(with(|o| {
                o.paths = vec![path.into()];
                o.keep_index = keep_index;
            }));
        }
    }
    routes
}

/// An invalid `core.ignorecase` stops every stash before anything is written: no stash commit,
/// no `refs/stash`, the index, the files and the link as they were (#369). Before, stash read it
/// as unset and skipped its case checks, and its forced reset could remove the link before
/// libgit2's checkout refused the value.
#[test]
fn stash_save_refuses_an_invalid_core_ignorecase_and_writes_nothing() {
    for opts in every_stash_route() {
        let (dir, mut repo, out) = every_kind_of_change();
        let status_before = git_core::status::status(&repo).unwrap();
        set_ignorecase(&repo, Some("maybe"));
        let before = untouched_state(dir.path());

        let error = git_core::stash::save_stash(&mut repo, &opts).unwrap_err();

        assert!(
            matches!(error, StashError::CaseConfig(_)),
            "{opts:?}: {error:?}"
        );
        assert!(
            error.to_string().starts_with("Can't read core.ignorecase"),
            "{error}"
        );
        assert_eq!(untouched_state(dir.path()), before, "{opts:?}");
        common::assert_outside_untouched(out.path());

        // Absent again: the same stash works as before.
        set_ignorecase(&repo, None);
        assert_eq!(git_core::status::status(&repo).unwrap(), status_before);
        git_core::stash::save_stash(&mut repo, &opts).unwrap();
        assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
        common::assert_outside_untouched(out.path());
    }
}

#[test]
fn stash_save_works_with_a_valid_core_ignorecase() {
    for value in ["true", "false"] {
        for opts in every_stash_route() {
            let (dir, mut repo, out) = every_kind_of_change();
            set_ignorecase(&repo, Some(value));

            git_core::stash::save_stash(&mut repo, &opts).unwrap();

            assert_eq!(
                git_core::stash::list_stashes(&mut repo).unwrap().len(),
                1,
                "{value} {opts:?}"
            );
            common::assert_outside_untouched(out.path());
            if opts.paths.is_empty() && !opts.keep_index {
                assert_eq!(read(dir.path(), "a.txt"), "a\n", "{value} {opts:?}");
                assert_eq!(
                    git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap(),
                    StashApplyOutcome::Applied,
                    "{value} {opts:?}"
                );
                assert_eq!(read(dir.path(), "a.txt"), "edited\n", "{value} {opts:?}");
            }
        }
    }
}

/// Apply and Pop stop too, with the stash kept and nothing written (#369). Drop only edits the
/// stash list and still works.
#[test]
fn apply_and_pop_refuse_an_invalid_core_ignorecase_and_write_nothing() {
    let (dir, mut repo, out) = every_kind_of_change();
    git_core::stash::save_stash(&mut repo, &with(|o| o.include_untracked = true)).unwrap();
    set_ignorecase(&repo, Some("maybe"));
    let before = untouched_state(dir.path());

    for restore_index in [false, true] {
        let error = git_core::stash::apply_stash(&mut repo, 0, None, restore_index).unwrap_err();
        assert!(matches!(error, StashError::CaseConfig(_)), "{error:?}");
        assert_eq!(untouched_state(dir.path()), before);
        let error = git_core::stash::pop_stash(&mut repo, 0, None, restore_index).unwrap_err();
        assert!(matches!(error, StashError::CaseConfig(_)), "{error:?}");
        assert_eq!(untouched_state(dir.path()), before);
    }
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
    common::assert_outside_untouched(out.path());

    // Absent again: Pop works.
    set_ignorecase(&repo, None);
    assert_eq!(
        git_core::stash::pop_stash(&mut repo, 0, None, true).unwrap(),
        StashApplyOutcome::Applied
    );
    assert_eq!(read(dir.path(), "a.txt"), "edited\n");
    assert_eq!(read(dir.path(), "u.txt"), "untracked\n");

    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    set_ignorecase(&repo, Some("maybe"));
    git_core::stash::drop_stash(&mut repo, 0, None).unwrap();
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
}
