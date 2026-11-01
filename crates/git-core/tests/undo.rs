// Hide the developer's / CI runner's global and system git config from every test (#111).
test_support::hermetic_git!();

use std::fs;
use std::path::Path;

use git2::Repository;
use git_core::undo::{capture, restore, HeadState, RepoSnapshot, SnapshotScope, UndoError};
use tempfile::TempDir;

fn init_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_bool("core.autocrlf", false).unwrap();
        config.set_str("core.eol", "lf").unwrap();
        config.set_bool("core.safecrlf", false).unwrap();
        config.set_bool("commit.gpgSign", false).unwrap();
        let hooks = repo.path().join("hooks");
        config
            .set_str("core.hooksPath", hooks.to_str().unwrap())
            .unwrap();
    }
    repo.set_head("refs/heads/main").unwrap();
    (dir, repo)
}

fn write(repo: &Repository, path: &str, contents: &str) {
    let full = repo.workdir().unwrap().join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, contents).unwrap();
}

fn read(repo: &Repository, path: &str) -> Option<String> {
    fs::read_to_string(repo.workdir().unwrap().join(path)).ok()
}

fn stage(repo: &Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

fn commit(repo: &Repository, message: &str) -> git2::Oid {
    let mut index = repo.index().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap()
}

fn seeded() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write(&repo, "a.txt", "one\n");
    stage(&repo, "a.txt");
    commit(&repo, "initial");
    (dir, repo)
}

#[test]
fn capture_is_stable_when_nothing_changes() {
    let (_dir, repo) = seeded();
    write(&repo, "dirty.txt", "untracked\n");
    let first = capture(&repo, SnapshotScope::Full).unwrap();
    let second = capture(&repo, SnapshotScope::Full).unwrap();
    assert_eq!(first, second);
}

#[test]
fn refs_and_index_scope_has_no_worktree_tree() {
    let (_dir, repo) = seeded();
    let snapshot = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    assert_eq!(snapshot.worktree_tree, None);
    assert!(
        matches!(snapshot.head, HeadState::Branch(ref name) if name.starts_with("refs/heads/"))
    );
    assert_eq!(snapshot.refs.len(), 1);
}

#[test]
fn staging_changes_only_the_index_tree() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    stage(&repo, "a.txt");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(before.index_tree, after.index_tree);
    assert_eq!(before.worktree_tree, after.worktree_tree);
    assert_eq!(before.refs, after.refs);
}

#[test]
fn discarding_a_change_changes_the_worktree_tree() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "edited\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "a.txt", "one\n");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(before.worktree_tree, after.worktree_tree);
    assert_eq!(before.index_tree, after.index_tree);
}

#[test]
fn untracked_and_deleted_files_are_part_of_the_worktree_tree() {
    let (_dir, repo) = seeded();
    let clean = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "nested/new.txt", "new\n");
    let with_untracked = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(clean.worktree_tree, with_untracked.worktree_tree);
    fs::remove_file(repo.workdir().unwrap().join("nested/new.txt")).unwrap();
    fs::remove_file(repo.workdir().unwrap().join("a.txt")).unwrap();
    let with_deletion = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(clean.worktree_tree, with_deletion.worktree_tree);
    assert_eq!(read(&repo, "a.txt"), None);
}

#[test]
fn ignored_files_are_not_snapshotted() {
    let (_dir, repo) = seeded();
    write(&repo, ".gitignore", "*.log\n");
    stage(&repo, ".gitignore");
    commit(&repo, "ignore logs");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "build.log", "noise\n");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    assert_eq!(before, after);
}

#[test]
fn unborn_head_is_captured_as_a_branch() {
    let (_dir, repo) = init_repo();
    write(&repo, "a.txt", "one\n");
    let snapshot = capture(&repo, SnapshotScope::Full).unwrap();
    assert!(matches!(snapshot.head, HeadState::Branch(_)));
    assert!(snapshot.refs.is_empty());
    assert!(snapshot.worktree_tree.is_some());
}

#[test]
fn detached_head_is_captured_by_oid() {
    let (_dir, repo) = seeded();
    let oid = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(oid).unwrap();
    let snapshot = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    assert_eq!(snapshot.head, HeadState::Detached(oid.to_string()));
}

#[test]
fn a_conflicted_index_refuses_to_capture() {
    let (_dir, repo) = seeded();
    let mut index = repo.index().unwrap();
    let id = repo.blob(b"x\n").unwrap();
    for stage_number in [1u16, 2, 3] {
        index
            .add(&git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: 0o100644,
                uid: 0,
                gid: 0,
                file_size: 0,
                id,
                flags: (stage_number << 12) | "c.txt".len() as u16,
                flags_extended: 0,
                path: b"c.txt".to_vec(),
            })
            .unwrap();
    }
    index.write().unwrap();
    assert!(matches!(
        capture(&repo, SnapshotScope::RefsAndIndex),
        Err(UndoError::ConflictedIndex)
    ));
}

#[cfg(unix)]
#[test]
fn executable_bit_and_symlinks_change_the_worktree_tree() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, repo) = seeded();
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    let full = repo.workdir().unwrap().join("a.txt");
    fs::set_permissions(&full, fs::Permissions::from_mode(0o755)).unwrap();
    let exec = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(before.worktree_tree, exec.worktree_tree);
    std::os::unix::fs::symlink("a.txt", repo.workdir().unwrap().join("link")).unwrap();
    let linked = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(exec.worktree_tree, linked.worktree_tree);
}

/// Runs `op`, then checks undo (after -> before) and redo (before -> after) both round-trip.
fn assert_round_trip(
    repo: &Repository,
    scope: SnapshotScope,
    op: impl FnOnce(&Repository),
) -> (RepoSnapshot, RepoSnapshot) {
    let before = capture(repo, scope).unwrap();
    op(repo);
    let after = capture(repo, scope).unwrap();
    assert_ne!(before, after, "operation changed nothing");
    restore(repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(
        capture(repo, scope).unwrap(),
        before,
        "undo did not restore"
    );
    restore(repo, &before, &after, "browsitory: redo test").unwrap();
    assert_eq!(capture(repo, scope).unwrap(), after, "redo did not restore");
    (before, after)
}

#[test]
fn undoes_and_redoes_staging() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    assert_round_trip(&repo, SnapshotScope::RefsAndIndex, |repo| {
        stage(repo, "a.txt")
    });
}

#[test]
fn undoes_a_discard_of_a_modified_file() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "precious edit\n");
    assert_round_trip(&repo, SnapshotScope::Full, |repo| {
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().path("a.txt");
        repo.checkout_head(Some(&mut checkout)).unwrap();
    });
    // assert_round_trip ends after redo, so the discard is applied again.
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("one\n"));
}

#[test]
fn undoes_a_discard_of_an_untracked_file_in_a_new_directory() {
    let (_dir, repo) = seeded();
    write(&repo, "new/dir/file.txt", "keep me\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    fs::remove_dir_all(repo.workdir().unwrap().join("new")).unwrap();
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(
        read(&repo, "new/dir/file.txt").as_deref(),
        Some("keep me\n")
    );
    restore(&repo, &before, &after, "browsitory: redo test").unwrap();
    assert!(
        !repo.workdir().unwrap().join("new").exists(),
        "empty dirs are removed"
    );
}

#[test]
fn undoes_a_commit_leaving_changes_staged() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    let first = repo.head().unwrap().target().unwrap();
    let (before, after) = assert_round_trip(&repo, SnapshotScope::RefsAndIndex, |repo| {
        commit(repo, "second");
    });
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(repo.head().unwrap().target().unwrap(), first);
    let statuses = repo.statuses(None).unwrap();
    assert!(statuses
        .iter()
        .any(|e| e.status().contains(git2::Status::INDEX_MODIFIED)));
}

#[test]
fn undoes_the_first_commit_back_to_an_unborn_branch() {
    let (_dir, repo) = init_repo();
    write(&repo, "a.txt", "one\n");
    stage(&repo, "a.txt");
    assert_round_trip(&repo, SnapshotScope::RefsAndIndex, |repo| {
        commit(repo, "initial");
    });
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    assert_eq!(before.refs.len(), 1);
}

#[test]
fn undoes_a_hard_reset() {
    let (_dir, repo) = seeded();
    let first = repo.head().unwrap().target().unwrap();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    commit(&repo, "second");
    write(&repo, "a.txt", "uncommitted\n");
    write(&repo, "b.txt", "untracked survives\n");
    assert_round_trip(&repo, SnapshotScope::Full, |repo| {
        let target = repo.find_object(first, None).unwrap();
        repo.reset(&target, git2::ResetType::Hard, None).unwrap();
    });
    let after_redo = read(&repo, "a.txt");
    assert_eq!(after_redo.as_deref(), Some("one\n"));
}

#[test]
fn undoes_deleting_a_branch_and_an_annotated_tag() {
    let (_dir, repo) = seeded();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let sig = repo.signature().unwrap();
    repo.tag("v1", head.as_object(), &sig, "release", false)
        .unwrap();
    assert_round_trip(&repo, SnapshotScope::RefsAndIndex, |repo| {
        repo.find_branch("feature", git2::BranchType::Local)
            .unwrap()
            .delete()
            .unwrap();
        repo.find_reference("refs/tags/v1")
            .unwrap()
            .delete()
            .unwrap();
    });
    assert!(
        repo.find_reference("refs/tags/v1").is_err(),
        "redo deleted it again"
    );
}

#[test]
fn undoes_checking_out_a_commit() {
    let (_dir, repo) = seeded();
    let first = repo.head().unwrap().target().unwrap();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    commit(&repo, "second");
    let (before, after) = assert_round_trip(&repo, SnapshotScope::Full, |repo| {
        let target = repo.find_object(first, None).unwrap();
        repo.checkout_tree(&target, None).unwrap();
        repo.set_head_detached(first).unwrap();
    });
    assert!(matches!(before.head, HeadState::Branch(_)));
    assert_eq!(after.head, HeadState::Detached(first.to_string()));
}

#[test]
fn refuses_when_a_touched_file_was_edited_and_writes_nothing() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "precious edit\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "a.txt", "one\n"); // the discard
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "a.txt", "typed again after the discard\n");
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(matches!(result, Err(UndoError::RepositoryChanged)));
    assert_eq!(
        read(&repo, "a.txt").as_deref(),
        Some("typed again after the discard\n")
    );
}

#[test]
fn refuses_when_a_touched_ref_moved() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    commit(&repo, "second");
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    write(&repo, "a.txt", "three\n");
    stage(&repo, "a.txt");
    commit(&repo, "third, outside the app");
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(matches!(result, Err(UndoError::RepositoryChanged)));
}

#[test]
fn refuses_when_the_index_changed_for_an_index_entry() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    write(&repo, "b.txt", "b\n");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    stage(&repo, "a.txt");
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    stage(&repo, "b.txt");
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(matches!(result, Err(UndoError::RepositoryChanged)));
}

#[test]
fn unrelated_branch_and_file_changes_do_not_block_undo() {
    let (_dir, repo) = seeded();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("other", &head, false).unwrap();
    write(&repo, "a.txt", "edit\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "a.txt", "one\n");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    // Unrelated: another branch moves, another file is edited.
    let tree = head.tree().unwrap();
    let sig = repo.signature().unwrap();
    let moved = repo
        .commit(None, &sig, &sig, "elsewhere", &tree, &[&head])
        .unwrap();
    repo.reference("refs/heads/other", moved, true, "move other")
        .unwrap();
    write(&repo, "unrelated.txt", "new\n");
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("edit\n"));
    assert_eq!(read(&repo, "unrelated.txt").as_deref(), Some("new\n"));
}

#[test]
fn refuses_while_a_merge_is_in_progress() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    let second = commit(&repo, "second");
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    fs::write(repo.path().join("MERGE_HEAD"), format!("{second}\n")).unwrap();
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(matches!(result, Err(UndoError::OperationInProgress)));
}

#[test]
fn crlf_content_round_trips_byte_exact() {
    let (_dir, repo) = seeded();
    let full = repo.workdir().unwrap().join("a.txt");
    fs::write(&full, b"line one\r\nline two\r\n").unwrap();
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    fs::write(&full, b"one\n").unwrap();
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(fs::read(&full).unwrap(), b"line one\r\nline two\r\n");
}

#[test]
fn writes_reflog_messages_for_moved_refs() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    commit(&repo, "second");
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    restore(&repo, &after, &before, "browsitory: undo Commit “second”").unwrap();
    let branch = match &before.head {
        HeadState::Branch(name) => name.clone(),
        HeadState::Detached(_) => unreachable!(),
    };
    let reflog = repo.reflog(&branch).unwrap();
    assert_eq!(
        reflog.get(0).unwrap().message().unwrap(),
        Some("browsitory: undo Commit “second”")
    );
}

#[cfg(unix)]
#[test]
fn restores_executable_bit_and_symlinks() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, repo) = seeded();
    let workdir = repo.workdir().unwrap().to_path_buf();
    fs::set_permissions(workdir.join("a.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink("a.txt", workdir.join("link")).unwrap();
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    fs::remove_file(workdir.join("link")).unwrap();
    fs::set_permissions(workdir.join("a.txt"), fs::Permissions::from_mode(0o644)).unwrap();
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert!(fs::symlink_metadata(workdir.join("link"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_link(workdir.join("link")).unwrap(),
        Path::new("a.txt")
    );
    assert_ne!(
        fs::metadata(workdir.join("a.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0
    );
}

#[test]
fn ignoring_an_untracked_file_undoes_and_redoes_without_touching_it() {
    let (_dir, repo) = seeded();
    write(&repo, "x", "mine\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, ".gitignore", "x\n");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(before, after);
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(read(&repo, "x").as_deref(), Some("mine\n"));
    assert_eq!(read(&repo, ".gitignore"), None);
    restore(&repo, &before, &after, "browsitory: redo test").unwrap();
    assert_eq!(read(&repo, "x").as_deref(), Some("mine\n"));
    assert_eq!(read(&repo, ".gitignore").as_deref(), Some("x\n"));
}

#[test]
fn refuses_when_an_ignored_file_would_be_overwritten_with_other_bytes() {
    let (_dir, repo) = seeded();
    write(&repo, "x", "orig\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, ".gitignore", "x\n");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "x", "changed since\n");
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(matches!(result, Err(UndoError::RepositoryChanged)));
    assert_eq!(read(&repo, "x").as_deref(), Some("changed since\n"));
    assert_eq!(read(&repo, ".gitignore").as_deref(), Some("x\n"));
}

#[test]
fn refuses_before_writing_when_a_replaced_directory_holds_ignored_files() {
    let (_dir, repo) = seeded();
    write(&repo, ".gitignore", "junk\n");
    stage(&repo, ".gitignore");
    commit(&repo, "ignore junk");
    write(&repo, "d/a", "tracked-ish\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    fs::remove_dir_all(repo.workdir().unwrap().join("d")).unwrap();
    write(&repo, "d", "now a file\n");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    // Undo (after -> before) replaces file d by directory d: fine. Redo is the risky direction.
    fs::remove_file(repo.workdir().unwrap().join("d")).unwrap();
    write(&repo, "d/a", "tracked-ish\n");
    write(&repo, "d/junk", "build output\n");
    let result = restore(&repo, &before, &after, "browsitory: redo test");
    assert!(matches!(result, Err(UndoError::RepositoryChanged)));
    assert_eq!(read(&repo, "d/a").as_deref(), Some("tracked-ish\n"));
    assert_eq!(read(&repo, "d/junk").as_deref(), Some("build output\n"));
}

#[test]
fn writes_head_reflog_message_too() {
    let (_dir, repo) = seeded();
    let first = repo.head().unwrap().target().unwrap();
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    repo.set_head_detached(first).unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    restore(&repo, &after, &before, "browsitory: undo checkout").unwrap();
    let reflog = repo.reflog("HEAD").unwrap();
    assert_eq!(
        reflog.get(0).unwrap().message().unwrap(),
        Some("browsitory: undo checkout")
    );
}

#[test]
fn tracked_files_matching_an_ignore_pattern_are_still_deleted_and_restored() {
    let (_dir, repo) = seeded();
    write(&repo, ".gitignore", "*.log\n");
    write(&repo, "build.log", "tracked anyway\n");
    stage(&repo, ".gitignore");
    stage(&repo, "build.log");
    commit(&repo, "track a log");
    assert_round_trip(&repo, SnapshotScope::Full, |repo| {
        fs::remove_file(repo.workdir().unwrap().join("build.log")).unwrap();
    });
    assert_eq!(
        read(&repo, "build.log"),
        None,
        "redo deleted the tracked file"
    );
}

#[test]
fn refuses_before_writing_when_a_replaced_directory_holds_an_untracked_file() {
    let (_dir, repo) = seeded();
    write(&repo, "d/x.o", "object\n");
    let from = capture(&repo, SnapshotScope::Full).unwrap();
    fs::remove_dir_all(repo.workdir().unwrap().join("d")).unwrap();
    write(&repo, "d", "now a file\n");
    write(&repo, ".gitignore", "*.o\n");
    let to = capture(&repo, SnapshotScope::Full).unwrap();
    // Back to the `from` state, then redo toward `to`.
    fs::remove_file(repo.workdir().unwrap().join("d")).unwrap();
    fs::remove_file(repo.workdir().unwrap().join(".gitignore")).unwrap();
    write(&repo, "d/x.o", "object\n");
    let result = restore(&repo, &from, &to, "browsitory: redo test");
    assert!(
        matches!(result, Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    assert_eq!(read(&repo, ".gitignore"), None);
    assert_eq!(read(&repo, "d/x.o").as_deref(), Some("object\n"));
}

#[test]
fn undoes_a_reset_that_swaps_a_file_for_a_directory() {
    let (_dir, repo) = seeded();
    write(&repo, "d", "a file\n");
    stage(&repo, "d");
    let file_commit = commit(&repo, "d is a file");
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("d")).unwrap();
    index.write().unwrap();
    fs::remove_file(repo.workdir().unwrap().join("d")).unwrap();
    write(&repo, "d/a", "inside\n");
    stage(&repo, "d/a");
    commit(&repo, "d is a directory");
    assert_round_trip(&repo, SnapshotScope::Full, |repo| {
        let target = repo.find_object(file_commit, None).unwrap();
        repo.reset(&target, git2::ResetType::Hard, None).unwrap();
    });
    assert_eq!(read(&repo, "d").as_deref(), Some("a file\n"));
}

/// Adds a linked worktree (under `parent`) with `branch` checked out.
fn add_worktree(repo: &Repository, parent: &TempDir, name: &str, branch: &str) -> Repository {
    let reference = repo.find_reference(branch).unwrap();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    let path = parent.path().join(name);
    repo.worktree(name, &path, Some(&options)).unwrap();
    Repository::open(&path).unwrap()
}

fn head_branch(snapshot: &RepoSnapshot) -> String {
    match &snapshot.head {
        HeadState::Branch(name) => name.clone(),
        HeadState::Detached(_) => unreachable!(),
    }
}

#[test]
fn refuses_to_delete_a_branch_checked_out_in_another_worktree() {
    let (_dir, repo) = seeded();
    let worktrees = TempDir::new().unwrap();
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("x", &head, false).unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    let _linked = add_worktree(&repo, &worktrees, "wt", "refs/heads/x");
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(
        matches!(result, Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    assert!(repo.find_reference("refs/heads/x").is_ok());
}

#[test]
fn refuses_to_delete_the_branch_this_worktree_switched_to_since() {
    let (_dir, repo) = seeded();
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("x", &head, false).unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    repo.set_head("refs/heads/x").unwrap(); // switched outside the app
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(
        matches!(result, Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    assert!(repo.find_reference("refs/heads/x").is_ok());
    assert_eq!(repo.head().unwrap().name(), Ok("refs/heads/x"));
}

#[test]
fn refuses_to_move_a_branch_checked_out_in_another_worktree() {
    let (_dir, repo) = seeded();
    let worktrees = TempDir::new().unwrap();
    let main_branch = head_branch(&capture(&repo, SnapshotScope::RefsAndIndex).unwrap());
    let first = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("x", &first, false).unwrap();
    repo.set_head("refs/heads/x").unwrap();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    commit(&repo, "second on x");
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    // Undoing the commit would move x, which a linked worktree now has checked out.
    repo.set_head(&main_branch).unwrap();
    let _linked = add_worktree(&repo, &worktrees, "wt", "refs/heads/x");
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(
        matches!(result, Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
}

#[test]
fn refuses_to_switch_back_to_a_branch_checked_out_in_another_worktree() {
    let (_dir, repo) = seeded();
    let worktrees = TempDir::new().unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("x", &head, false).unwrap();
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    repo.set_head("refs/heads/x").unwrap();
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    let _linked = add_worktree(&repo, &worktrees, "wt", &head_branch(&before));
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(
        matches!(result, Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    assert_eq!(repo.head().unwrap().name(), Ok("refs/heads/x"));
}

#[test]
fn undoes_a_commit_inside_a_linked_worktree() {
    let (_dir, repo) = seeded();
    let worktrees = TempDir::new().unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("x", &head, false).unwrap();
    let linked = add_worktree(&repo, &worktrees, "wt", "refs/heads/x");
    write(&linked, "a.txt", "two\n");
    stage(&linked, "a.txt");
    assert_round_trip(&linked, SnapshotScope::RefsAndIndex, |linked| {
        commit(linked, "second on x");
    });
}

#[test]
fn refuses_to_rewrite_an_index_with_skip_worktree_entries() {
    let (_dir, repo) = seeded();
    write(&repo, "b.txt", "b\n");
    stage(&repo, "b.txt");
    commit(&repo, "add b");
    let skip = git2::IndexEntryExtendedFlag::SKIP_WORKTREE.bits();
    let flagged = |repo: &Repository| {
        let mut index = repo.index().unwrap();
        index.read(true).unwrap();
        index
            .get_path(Path::new("b.txt"), 0)
            .unwrap()
            .flags_extended
            & skip
            != 0
    };
    write(&repo, "a.txt", "two\n");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    stage(&repo, "a.txt");
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    let mut index = repo.index().unwrap();
    let mut entry = index.get_path(Path::new("b.txt"), 0).unwrap();
    entry.flags_extended |= skip;
    index.add(&entry).unwrap();
    index.write().unwrap();
    assert!(flagged(&repo), "test setup: flag not set");
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(
        matches!(result, Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    assert!(flagged(&repo));
}

/// `core.autocrlf=true` (the Git for Windows default): blobs hold LF, the worktree CRLF.
fn crlf_seeded() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", true)
        .unwrap();
    write(&repo, "a.txt", "one\ntwo\n");
    stage(&repo, "a.txt");
    commit(&repo, "initial");
    fs::remove_file(repo.workdir().unwrap().join("a.txt")).unwrap();
    checkout_path(&repo, "a.txt");
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("one\r\ntwo\r\n"));
    assert_eq!(status_of(&repo, "a.txt"), git2::Status::CURRENT);
    (dir, repo)
}

fn checkout_path(repo: &Repository, path: &str) {
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force().path(path);
    repo.checkout_head(Some(&mut checkout)).unwrap();
}

fn status_of(repo: &Repository, path: &str) -> git2::Status {
    repo.status_file(Path::new(path)).unwrap()
}

#[test]
fn undoes_a_discard_with_autocrlf() {
    let (_dir, repo) = crlf_seeded();
    let full = repo.workdir().unwrap().join("a.txt");
    fs::write(&full, b"edited\r\n").unwrap();
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    checkout_path(&repo, "a.txt"); // the discard
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(fs::read(&full).unwrap(), b"edited\r\n");
    assert_eq!(capture(&repo, SnapshotScope::Full).unwrap(), before);
    restore(&repo, &before, &after, "browsitory: redo test").unwrap();
    assert_eq!(fs::read(&full).unwrap(), b"one\r\ntwo\r\n");
    assert_eq!(status_of(&repo, "a.txt"), git2::Status::CURRENT);
    assert_eq!(capture(&repo, SnapshotScope::Full).unwrap(), after);
}

#[test]
fn undoes_a_branch_switch_with_autocrlf() {
    let (_dir, repo) = crlf_seeded();
    let main_branch = head_branch(&capture(&repo, SnapshotScope::RefsAndIndex).unwrap());
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("x", &head, false).unwrap();
    repo.set_head("refs/heads/x").unwrap();
    write(&repo, "a.txt", "on x\r\n");
    stage(&repo, "a.txt");
    commit(&repo, "x");
    fs::remove_file(repo.workdir().unwrap().join("a.txt")).unwrap();
    checkout_path(&repo, "a.txt");
    assert_round_trip(&repo, SnapshotScope::Full, |repo| {
        let tree = repo.revparse_single(&main_branch).unwrap();
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.safe();
        repo.checkout_tree(&tree, Some(&mut checkout)).unwrap();
        repo.set_head(&main_branch).unwrap();
    });
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("one\r\ntwo\r\n"));
    assert_eq!(status_of(&repo, "a.txt"), git2::Status::CURRENT);
}

#[cfg(unix)]
#[test]
fn keeps_the_recorded_exec_bit_with_filemode_off() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, repo) = seeded();
    let full = repo.workdir().unwrap().join("a.txt");
    fs::set_permissions(&full, fs::Permissions::from_mode(0o755)).unwrap();
    stage(&repo, "a.txt");
    commit(&repo, "make a.txt executable");
    repo.config()
        .unwrap()
        .set_bool("core.filemode", false)
        .unwrap();
    // A filesystem without exec bits: git keeps the recorded 0755 regardless.
    fs::set_permissions(&full, fs::Permissions::from_mode(0o644)).unwrap();
    write(&repo, "a.txt", "edited\n");
    let (before, _) = assert_round_trip(&repo, SnapshotScope::Full, |repo| {
        checkout_path(repo, "a.txt");
    });
    let tree = repo
        .find_tree(git2::Oid::from_str(before.worktree_tree.as_ref().unwrap()).unwrap())
        .unwrap();
    assert_eq!(tree.get_name("a.txt").unwrap().filemode(), 0o100755);
}

#[test]
fn keeps_the_recorded_symlink_mode_with_symlinks_off() {
    let (_dir, repo) = seeded();
    repo.config()
        .unwrap()
        .set_bool("core.symlinks", false)
        .unwrap();
    // As checked out without symlink support: a plain file holding the link target.
    let id = repo.blob(b"a.txt").unwrap();
    let mut index = repo.index().unwrap();
    index
        .add(&git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: 0o120000,
            uid: 0,
            gid: 0,
            file_size: 0,
            id,
            flags: "link".len() as u16,
            flags_extended: 0,
            path: b"link".to_vec(),
        })
        .unwrap();
    index.write().unwrap();
    commit(&repo, "add link");
    checkout_path(&repo, "link");
    write(&repo, "link", "elsewhere.txt");
    let (before, _) = assert_round_trip(&repo, SnapshotScope::Full, |repo| {
        checkout_path(repo, "link");
    });
    let tree = repo
        .find_tree(git2::Oid::from_str(before.worktree_tree.as_ref().unwrap()).unwrap())
        .unwrap();
    assert_eq!(tree.get_name("link").unwrap().filemode(), 0o120000);
    let meta = fs::symlink_metadata(repo.workdir().unwrap().join("link")).unwrap();
    assert!(meta.is_file());
}

#[test]
fn a_successful_restore_clears_a_leftover_squash_message() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    stage(&repo, "a.txt");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    fs::write(
        repo.path().join("SQUASH_MSG"),
        "Squashed commit of the following:\n",
    )
    .unwrap();

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();

    assert!(!repo.path().join("SQUASH_MSG").exists());
}

#[test]
fn a_refused_restore_keeps_the_squash_message() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    stage(&repo, "a.txt");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "a.txt", "staged since\n");
    stage(&repo, "a.txt");
    fs::write(
        repo.path().join("SQUASH_MSG"),
        "Squashed commit of the following:\n",
    )
    .unwrap();

    assert!(restore(&repo, &after, &before, "browsitory: undo test").is_err());

    assert!(repo.path().join("SQUASH_MSG").exists());
}

#[cfg(unix)]
fn outside_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("target.txt"), "OUTSIDE\n").unwrap();
    fs::write(dir.path().join("f"), "OUTSIDE\n").unwrap();
    dir
}

#[cfg(unix)]
fn assert_outside_untouched(out: &Path) {
    assert_eq!(
        fs::read_to_string(out.join("target.txt")).unwrap(),
        "OUTSIDE\n"
    );
    assert_eq!(fs::read_to_string(out.join("f")).unwrap(), "OUTSIDE\n");
}

#[cfg(unix)]
fn seeded_with_tc_and_d() -> (TempDir, Repository) {
    let (dir, repo) = seeded();
    write(&repo, "tc", "plain\n");
    write(&repo, "d/f", "inner\n");
    stage(&repo, "tc");
    stage(&repo, "d/f");
    commit(&repo, "tc and d");
    (dir, repo)
}

#[test]
#[cfg(unix)]
fn restore_over_a_typechanged_symlink_does_not_write_through_it() {
    let out = outside_dir();
    let (dir, repo) = seeded_with_tc_and_d();
    let clean = capture(&repo, SnapshotScope::Full).unwrap();
    fs::remove_file(dir.path().join("tc")).unwrap();
    std::os::unix::fs::symlink(out.path().join("target.txt"), dir.path().join("tc")).unwrap();
    let dirty = capture(&repo, SnapshotScope::Full).unwrap();

    restore(&repo, &dirty, &clean, "undo").unwrap();

    assert_outside_untouched(out.path());
    assert!(!fs::symlink_metadata(dir.path().join("tc"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(read(&repo, "tc").as_deref(), Some("plain\n"));
}

#[test]
#[cfg(unix)]
fn restore_over_a_symlinked_directory_does_not_write_through_it() {
    let out = outside_dir();
    let (dir, repo) = seeded_with_tc_and_d();
    let clean = capture(&repo, SnapshotScope::Full).unwrap();
    fs::remove_dir_all(dir.path().join("d")).unwrap();
    std::os::unix::fs::symlink(out.path(), dir.path().join("d")).unwrap();
    let dirty = capture(&repo, SnapshotScope::Full).unwrap();

    restore(&repo, &dirty, &clean, "undo").unwrap();

    assert_outside_untouched(out.path());
    assert!(!fs::symlink_metadata(dir.path().join("d"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(read(&repo, "d/f").as_deref(), Some("inner\n"));
}

#[test]
#[cfg(unix)]
fn redo_to_a_snapshot_with_a_symlinked_directory_never_deletes_through_it() {
    // Snapshot pair: real `d/f` → `d` replaced by a link. Undo restores the real directory;
    // redo deletes `d/f` and writes the link. Neither may touch the outside directory.
    let out = outside_dir();
    let (dir, repo) = seeded_with_tc_and_d();
    let with_dir = capture(&repo, SnapshotScope::Full).unwrap();
    fs::remove_dir_all(dir.path().join("d")).unwrap();
    std::os::unix::fs::symlink(out.path(), dir.path().join("d")).unwrap();
    let with_link = capture(&repo, SnapshotScope::Full).unwrap();

    restore(&repo, &with_link, &with_dir, "undo").unwrap();
    assert_outside_untouched(out.path());
    assert_eq!(read(&repo, "d/f").as_deref(), Some("inner\n"));

    restore(&repo, &with_dir, &with_link, "redo").unwrap();
    assert_outside_untouched(out.path());
    assert!(fs::symlink_metadata(dir.path().join("d"))
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
#[cfg(unix)]
fn restore_never_deletes_through_a_symlinked_directory_swapped_in_after_the_snapshot() {
    let out = outside_dir();
    fs::write(out.path().join("x.txt"), "OUTSIDE\n").unwrap();
    let (dir, repo) = seeded_with_tc_and_d();
    write(&repo, "d/x.txt", "untracked\n");
    let with_file = capture(&repo, SnapshotScope::Full).unwrap();
    fs::remove_file(dir.path().join("d/x.txt")).unwrap();
    let without = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "d/x.txt", "untracked\n");
    fs::remove_dir_all(dir.path().join("d")).unwrap();
    std::os::unix::fs::symlink(out.path(), dir.path().join("d")).unwrap();

    let result = restore(&repo, &with_file, &without, "redo");

    assert!(
        matches!(result, Ok(()) | Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    assert!(out.path().join("x.txt").exists());
    assert_outside_untouched(out.path());
}

/// A named pipe where undo expects a file must not hang it (#149): it is no file git can
/// record, and opening it for reading would wait for a writer forever.
#[cfg(unix)]
#[test]
fn undo_skips_a_fifo_that_replaced_a_file_instead_of_blocking() {
    let (dir, repo) = seeded();
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "a.txt", "two\n");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    fs::remove_file(dir.path().join("a.txt")).unwrap();
    let pipe = std::ffi::CString::new(dir.path().join("a.txt").to_str().unwrap()).unwrap();
    // SAFETY: `pipe` is a valid NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(pipe.as_ptr(), 0o600) }, 0);

    let path = dir.path().to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let repo = Repository::open(path).unwrap();
        let _ = tx.send(restore(&repo, &after, &before, "undo").map_err(|e| e.to_string()));
    });
    let result = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("undo blocked on the FIFO");
    // The FIFO is not the file the snapshot recorded.
    assert_eq!(result, Err(UndoError::RepositoryChanged.to_string()));
}

/// Windows: an undo whose file sits below a folder that a junction replaced never writes
/// through the junction (#149, #150).
#[cfg(windows)]
#[test]
fn undo_restore_never_writes_through_a_junction() {
    let (dir, repo) = init_repo();
    write(&repo, "d/f", "one\n");
    stage(&repo, "d/f");
    commit(&repo, "initial");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "d/f", "two\n");
    let after = capture(&repo, SnapshotScope::Full).unwrap();

    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("f"), "OUTSIDE\n").unwrap();
    fs::remove_dir_all(dir.path().join("d")).unwrap();
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(dir.path().join("d"))
        .arg(outside.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());

    // The junction isn't the snapshot's folder: refused, or written inside after removing it.
    let _ = restore(&repo, &after, &before, "undo");
    assert_eq!(
        fs::read_to_string(outside.path().join("f")).unwrap(),
        "OUTSIDE\n"
    );
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
}

/// Windows: an undo replaces a file the user made read-only, as `git checkout` does (#194).
#[cfg(windows)]
#[test]
fn undo_restores_a_read_only_file() {
    let (dir, repo) = init_repo();
    write(&repo, "a.txt", "one\n");
    stage(&repo, "a.txt");
    commit(&repo, "initial");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "a.txt", "two\n");
    let path = dir.path().join("a.txt");
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let after = capture(&repo, SnapshotScope::Full).unwrap();

    restore(&repo, &after, &before, "undo").unwrap();
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("one\n"));
}
