use std::fs;
use std::path::Path;

use git2::Repository;
use git_core::undo::{capture, HeadState, SnapshotScope, UndoError};
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
