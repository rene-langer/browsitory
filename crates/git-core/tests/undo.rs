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
    }
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
