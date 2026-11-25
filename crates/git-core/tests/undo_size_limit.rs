//! #1065: a `Full` snapshot stores no work-tree file over its limit; it identifies the file by
//! its stat data instead (`skipped`). Restoring leaves such a file alone while it is as the
//! snapshot saw it, and refuses (`NotSaved`) when it would have to bring back contents it never
//! stored. `capture_limited` is `Full` with a small limit, so most of these need no 50 MB files.

// Hide the developer's / CI runner's global and system git config from every test (#111).
test_support::hermetic_git!();

use std::fs;
use std::path::Path;

use git2::Repository;
use git_core::undo::{capture, capture_limited, restore, RepoSnapshot, SnapshotScope, UndoError};
use tempfile::TempDir;

const LIMIT: u64 = 1000;

fn seeded() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_bool("core.autocrlf", false).unwrap();
        config.set_bool("commit.gpgSign", false).unwrap();
    }
    repo.set_head("refs/heads/main").unwrap();
    write(&repo, "a.txt", "one\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
        .unwrap();
    drop(tree);
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

fn xs(n: usize) -> String {
    "x".repeat(n)
}

fn blob_stored(repo: &Repository, contents: &[u8]) -> bool {
    let id = git2::Oid::hash_object(git2::ObjectType::Blob, contents).unwrap();
    repo.odb().unwrap().exists(id)
}

fn tree_paths(repo: &Repository, snapshot: &RepoSnapshot) -> Vec<String> {
    let tree = repo
        .find_tree(git2::Oid::from_str(snapshot.worktree_tree.as_ref().unwrap()).unwrap())
        .unwrap();
    let mut paths = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            paths.push(format!("{root}{}", entry.name().unwrap()));
        }
        git2::TreeWalkResult::Ok
    })
    .unwrap();
    paths
}

fn assert_not_saved(result: Result<(), UndoError>, path: &str) {
    match result {
        Err(UndoError::NotSaved(got)) => assert_eq!(got, path),
        other => panic!("expected NotSaved({path}), got {other:?}"),
    }
}

#[test]
fn a_file_over_the_limit_is_identified_not_stored() {
    let (_dir, repo) = seeded();
    write(&repo, "big.bin", &xs(1500));
    write(&repo, "small.txt", "small\n");

    let snapshot = capture_limited(&repo, LIMIT).unwrap();

    assert_eq!(snapshot.skipped.keys().collect::<Vec<_>>(), vec!["big.bin"]);
    assert_eq!(snapshot.skipped["big.bin"].size, 1500);
    assert_eq!(snapshot.skipped["big.bin"].mode, 0o100644);
    assert_eq!(tree_paths(&repo, &snapshot), vec!["a.txt", "small.txt"]);
    assert!(!blob_stored(&repo, xs(1500).as_bytes()));
    assert!(blob_stored(&repo, b"small\n"));
}

#[test]
fn a_file_at_the_limit_is_stored() {
    let (_dir, repo) = seeded();
    write(&repo, "edge.bin", &xs(1000));

    let snapshot = capture_limited(&repo, LIMIT).unwrap();

    assert!(snapshot.skipped.is_empty());
    assert_eq!(tree_paths(&repo, &snapshot), vec!["a.txt", "edge.bin"]);
}

#[test]
fn a_modified_tracked_file_over_the_limit_is_left_out_of_the_tree() {
    // Not HEAD's version in its place: the tree must not claim contents the file doesn't have.
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", &xs(1500));

    let snapshot = capture_limited(&repo, LIMIT).unwrap();

    assert!(snapshot.skipped.contains_key("a.txt"));
    assert!(tree_paths(&repo, &snapshot).is_empty());
}

#[test]
fn full_skips_files_over_the_snapshot_file_limit_and_full_uncapped_stores_them() {
    let (_dir, repo) = seeded();
    let path = repo.workdir().unwrap().join("dump.sql");
    // Sparse: no 50 MB written to disk.
    fs::File::create(&path)
        .unwrap()
        .set_len(git_core::undo::SNAPSHOT_FILE_LIMIT + 1)
        .unwrap();
    let id = git2::Oid::hash_file(git2::ObjectType::Blob, &path).unwrap();

    let full = capture(&repo, SnapshotScope::Full).unwrap();
    assert!(full.skipped.contains_key("dump.sql"));
    assert!(!repo.odb().unwrap().exists(id));

    let uncapped = capture(&repo, SnapshotScope::FullUncapped).unwrap();
    assert!(uncapped.skipped.is_empty());
    assert!(repo.odb().unwrap().exists(id));
    assert_eq!(git_core::undo::SNAPSHOT_FILE_LIMIT, 50 * 1024 * 1024);
}

#[test]
fn restoring_leaves_a_skipped_file_alone_even_after_it_changed() {
    // Skipped alike before and after, the command didn't touch it: like any unchanged file it is
    // neither checked nor written, so editing it afterwards never blocks the undo.
    let (_dir, repo) = seeded();
    write(&repo, "big.bin", &xs(1500));
    let before = capture_limited(&repo, LIMIT).unwrap();
    write(&repo, "a.txt", "two\n");
    let after = capture_limited(&repo, LIMIT).unwrap();
    assert!(!before.skipped.is_empty());
    assert_eq!(before.skipped, after.skipped);
    write(&repo, "big.bin", &xs(1600));

    restore(&repo, &after, &before, "undo").unwrap();

    assert_eq!(read(&repo, "a.txt").as_deref(), Some("one\n"));
    assert_eq!(read(&repo, "big.bin"), Some(xs(1600)));
}

#[test]
fn undo_refuses_to_bring_back_a_skipped_file_the_command_deleted() {
    let (_dir, repo) = seeded();
    write(&repo, "big.bin", &xs(1500));
    let before = capture_limited(&repo, LIMIT).unwrap();
    fs::remove_file(repo.workdir().unwrap().join("big.bin")).unwrap();
    write(&repo, "a.txt", "two\n");
    let after = capture_limited(&repo, LIMIT).unwrap();

    assert_not_saved(restore(&repo, &after, &before, "undo"), "big.bin");

    // Refused before writing anything.
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("two\n"));
}

#[test]
fn undo_refuses_when_only_a_skipped_file_changed() {
    // Same worktree tree on both sides: only `skipped` tells them apart.
    let (_dir, repo) = seeded();
    write(&repo, "big.bin", &xs(1500));
    let before = capture_limited(&repo, LIMIT).unwrap();
    write(&repo, "big.bin", &xs(1600));
    let after = capture_limited(&repo, LIMIT).unwrap();
    assert_eq!(before.worktree_tree, after.worktree_tree);

    assert_not_saved(restore(&repo, &after, &before, "undo"), "big.bin");
    assert_eq!(read(&repo, "big.bin"), Some(xs(1600)));
}

#[test]
fn ignoring_a_skipped_file_undoes_and_redoes() {
    let (_dir, repo) = seeded();
    write(&repo, "big.bin", &xs(1500));
    let before = capture_limited(&repo, LIMIT).unwrap();
    write(&repo, ".gitignore", "big.bin\n");
    let after = capture_limited(&repo, LIMIT).unwrap();
    assert!(before.skipped.contains_key("big.bin"));
    assert!(after.skipped.is_empty());

    restore(&repo, &after, &before, "undo").unwrap();
    assert_eq!(read(&repo, ".gitignore"), None);
    assert_eq!(read(&repo, "big.bin"), Some(xs(1500)));

    restore(&repo, &before, &after, "redo").unwrap();
    assert_eq!(read(&repo, ".gitignore").as_deref(), Some("big.bin\n"));
    assert_eq!(read(&repo, "big.bin"), Some(xs(1500)));
}

#[test]
fn undo_writes_back_a_file_that_grew_over_the_limit_and_redo_refuses() {
    let (_dir, repo) = seeded();
    write(&repo, "f.txt", "small\n");
    let before = capture_limited(&repo, LIMIT).unwrap();
    write(&repo, "f.txt", &xs(1500));
    let after = capture_limited(&repo, LIMIT).unwrap();
    assert!(after.skipped.contains_key("f.txt"));

    restore(&repo, &after, &before, "undo").unwrap();
    assert_eq!(read(&repo, "f.txt").as_deref(), Some("small\n"));

    // Its grown contents were never stored.
    assert_not_saved(restore(&repo, &before, &after, "redo"), "f.txt");
    assert_eq!(read(&repo, "f.txt").as_deref(), Some("small\n"));
}

#[test]
fn undo_refuses_to_overwrite_a_skipped_file_changed_since() {
    let (_dir, repo) = seeded();
    write(&repo, "f.txt", "small\n");
    let before = capture_limited(&repo, LIMIT).unwrap();
    write(&repo, "f.txt", &xs(1500));
    let after = capture_limited(&repo, LIMIT).unwrap();
    assert!(after.skipped.contains_key("f.txt"));
    write(&repo, "f.txt", &xs(1600));

    assert!(matches!(
        restore(&repo, &after, &before, "undo"),
        Err(UndoError::RepositoryChanged)
    ));
    assert_eq!(read(&repo, "f.txt"), Some(xs(1600)));
}

#[test]
fn a_not_saved_refusal_names_the_file_and_the_limit() {
    assert_eq!(
        UndoError::NotSaved("dump.sql".into()).to_string(),
        "dump.sql is larger than 50 MB, so its contents weren't saved for undo, and it has \
         changed since."
    );
}
