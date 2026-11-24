//! #1090: a `Full` snapshot with more untracked files than its limit stores none of them; it
//! identifies each by its stat data instead (`unsaved_untracked`). Restoring leaves such files
//! alone while they are as the snapshot saw them, and refuses (`NotSavedUntracked`) when it would
//! have to bring back contents it never stored. `capture_counted` is `Full` with a small limit,
//! so most of these need no 10,000 files.

// Hide the developer's / CI runner's global and system git config from every test (#111).
test_support::hermetic_git!();

use std::fs;
use std::path::Path;

use git2::Repository;
use git_core::undo::{
    capture, capture_counted, restore, unsaved_untracked, RepoSnapshot, SnapshotScope, UndoError,
};
use tempfile::TempDir;

const LIMIT: usize = 3;

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
    commit(&repo, &["a.txt"], "initial");
    (dir, repo)
}

fn commit(repo: &Repository, paths: &[&str], message: &str) {
    let mut index = repo.index().unwrap();
    for path in paths {
        index.add_path(Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap();
}

fn write(repo: &Repository, path: &str, contents: &str) {
    let full = repo.workdir().unwrap().join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, contents).unwrap();
}

fn read(repo: &Repository, path: &str) -> Option<String> {
    fs::read_to_string(repo.workdir().unwrap().join(path)).ok()
}

/// `n` untracked files `u/0.txt`… with contents `untracked <i>`.
fn untracked(repo: &Repository, n: usize) {
    for i in 0..n {
        write(repo, &format!("u/{i}.txt"), &format!("untracked {i}\n"));
    }
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

fn unsaved_paths(repo: &Repository, snapshot: &RepoSnapshot) -> Vec<String> {
    unsaved_untracked(repo, snapshot)
        .unwrap()
        .into_keys()
        .collect()
}

fn assert_not_saved(result: Result<(), UndoError>, path: &str) {
    match result {
        Err(UndoError::NotSavedUntracked(got)) => assert_eq!(got, path),
        other => panic!("expected NotSavedUntracked({path}), got {other:?}"),
    }
}

#[test]
fn more_untracked_files_than_the_limit_are_identified_not_stored() {
    let (_dir, repo) = seeded();
    untracked(&repo, 4);
    write(&repo, "a.txt", "modified\n");

    let snapshot = capture_counted(&repo, LIMIT).unwrap();

    assert!(snapshot.unsaved_untracked.is_some());
    let unsaved = unsaved_untracked(&repo, &snapshot).unwrap();
    assert_eq!(
        unsaved.keys().collect::<Vec<_>>(),
        vec!["u/0.txt", "u/1.txt", "u/2.txt", "u/3.txt"]
    );
    assert_eq!(unsaved["u/0.txt"].size, "untracked 0\n".len() as u64);
    assert_eq!(unsaved["u/0.txt"].mode, 0o100644);
    assert!(snapshot.skipped.is_empty());
    // A modified tracked file is stored whatever the count.
    assert_eq!(tree_paths(&repo, &snapshot), vec!["a.txt"]);
    assert!(blob_stored(&repo, b"modified\n"));
    assert!(!blob_stored(&repo, b"untracked 0\n"));
    assert!(!blob_stored(&repo, b"untracked 3\n"));
}

#[test]
fn untracked_files_at_the_limit_are_stored() {
    let (_dir, repo) = seeded();
    untracked(&repo, 3);

    let snapshot = capture_counted(&repo, LIMIT).unwrap();

    assert_eq!(snapshot.unsaved_untracked, None);
    assert!(unsaved_untracked(&repo, &snapshot).unwrap().is_empty());
    assert_eq!(
        tree_paths(&repo, &snapshot),
        vec!["a.txt", "u/0.txt", "u/1.txt", "u/2.txt"]
    );
}

#[test]
fn an_untracked_file_head_has_is_left_out_of_the_tree() {
    // Removed from the index only (`git rm --cached`): untracked, but in HEAD's tree. The
    // snapshot's tree must not claim HEAD's contents for it.
    let (_dir, repo) = seeded();
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("a.txt")).unwrap();
    index.write().unwrap();
    write(&repo, "a.txt", "changed\n");
    untracked(&repo, 3);

    let snapshot = capture_counted(&repo, LIMIT).unwrap();

    assert!(unsaved_paths(&repo, &snapshot).contains(&"a.txt".to_string()));
    assert!(tree_paths(&repo, &snapshot).is_empty());
}

#[test]
fn full_has_the_untracked_limit_and_full_uncapped_stores_every_file() {
    let (_dir, repo) = seeded();
    assert_eq!(git_core::undo::SNAPSHOT_UNTRACKED_LIMIT, 10_000);
    untracked(&repo, git_core::undo::SNAPSHOT_UNTRACKED_LIMIT + 1);

    let full = capture(&repo, SnapshotScope::Full).unwrap();
    assert_eq!(
        unsaved_untracked(&repo, &full).unwrap().len(),
        git_core::undo::SNAPSHOT_UNTRACKED_LIMIT + 1
    );
    assert!(!blob_stored(&repo, b"untracked 0\n"));

    let uncapped = capture(&repo, SnapshotScope::FullUncapped).unwrap();
    assert_eq!(uncapped.unsaved_untracked, None);
    assert!(blob_stored(&repo, b"untracked 0\n"));
    assert!(blob_stored(&repo, b"untracked 10000\n"));
}

#[test]
fn identical_lists_are_one_object() {
    let (_dir, repo) = seeded();
    untracked(&repo, 4);
    let before = capture_counted(&repo, LIMIT).unwrap();
    write(&repo, "a.txt", "two\n");
    let after = capture_counted(&repo, LIMIT).unwrap();

    assert!(before.unsaved_untracked.is_some());
    assert_eq!(before.unsaved_untracked, after.unsaved_untracked);
}

#[test]
fn restoring_leaves_unsaved_files_alone_even_after_they_changed() {
    // Listed alike before and after, the command didn't touch them: neither checked nor written.
    let (_dir, repo) = seeded();
    untracked(&repo, 4);
    let before = capture_counted(&repo, LIMIT).unwrap();
    write(&repo, "a.txt", "two\n");
    let after = capture_counted(&repo, LIMIT).unwrap();
    write(&repo, "u/1.txt", "edited since\n");
    fs::remove_file(repo.workdir().unwrap().join("u/2.txt")).unwrap();

    restore(&repo, &after, &before, "undo").unwrap();

    assert_eq!(read(&repo, "a.txt").as_deref(), Some("one\n"));
    assert_eq!(read(&repo, "u/1.txt").as_deref(), Some("edited since\n"));
    assert_eq!(read(&repo, "u/2.txt"), None);
    assert_eq!(read(&repo, "u/3.txt").as_deref(), Some("untracked 3\n"));
}

#[test]
fn undo_refuses_to_bring_back_an_unsaved_file_the_command_deleted() {
    let (_dir, repo) = seeded();
    untracked(&repo, 4);
    let before = capture_counted(&repo, LIMIT).unwrap();
    fs::remove_file(repo.workdir().unwrap().join("u/2.txt")).unwrap();
    write(&repo, "a.txt", "two\n");
    let after = capture_counted(&repo, LIMIT).unwrap();

    assert_not_saved(restore(&repo, &after, &before, "undo"), "u/2.txt");

    // Refused before writing anything.
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("two\n"));
}

#[test]
fn undo_refuses_to_bring_back_an_unsaved_file_changed_since() {
    let (_dir, repo) = seeded();
    untracked(&repo, 4);
    let before = capture_counted(&repo, LIMIT).unwrap();
    write(&repo, ".gitignore", "u/\n");
    let after = capture_counted(&repo, LIMIT).unwrap();
    assert_eq!(after.unsaved_untracked, None);
    write(&repo, "u/0.txt", "edited since\n");

    assert_not_saved(restore(&repo, &after, &before, "undo"), "u/0.txt");
    assert_eq!(read(&repo, ".gitignore").as_deref(), Some("u/\n"));
}

#[test]
fn ignoring_unsaved_files_undoes_and_redoes() {
    let (_dir, repo) = seeded();
    untracked(&repo, 4);
    write(&repo, "notes.txt", "keep\n");
    let before = capture_counted(&repo, LIMIT).unwrap();
    write(&repo, ".gitignore", "u/\n");
    // notes.txt and .gitignore: under the limit, so both stored.
    let after = capture_counted(&repo, LIMIT).unwrap();
    assert_eq!(
        unsaved_paths(&repo, &before),
        vec!["notes.txt", "u/0.txt", "u/1.txt", "u/2.txt", "u/3.txt"]
    );
    assert_eq!(after.unsaved_untracked, None);

    restore(&repo, &after, &before, "undo").unwrap();
    assert_eq!(read(&repo, ".gitignore"), None);
    assert_eq!(read(&repo, "notes.txt").as_deref(), Some("keep\n"));
    assert_eq!(read(&repo, "u/3.txt").as_deref(), Some("untracked 3\n"));

    restore(&repo, &before, &after, "redo").unwrap();
    assert_eq!(read(&repo, ".gitignore").as_deref(), Some("u/\n"));
    assert_eq!(read(&repo, "notes.txt").as_deref(), Some("keep\n"));
    assert_eq!(read(&repo, "u/3.txt").as_deref(), Some("untracked 3\n"));
}

#[test]
fn undo_across_the_limit_leaves_files_already_as_wanted_so_redo_still_works() {
    // A mixed reset that untracks a file takes the count over the limit: after it, every
    // untracked file is only identified. Undo must not rewrite those it already finds as they
    // should be, or they would no longer match what the redo side noted and redo would refuse.
    let (_dir, repo) = seeded();
    let initial = repo.head().unwrap().peel_to_commit().unwrap();
    write(&repo, "t.txt", "tracked\n");
    commit(&repo, &["t.txt"], "add t");
    untracked(&repo, 3);
    let before = capture_counted(&repo, LIMIT).unwrap();
    assert_eq!(before.unsaved_untracked, None);
    repo.reset(initial.as_object(), git2::ResetType::Mixed, None)
        .unwrap();
    let after = capture_counted(&repo, LIMIT).unwrap();
    assert_eq!(
        unsaved_paths(&repo, &after),
        vec!["t.txt", "u/0.txt", "u/1.txt", "u/2.txt"]
    );

    restore(&repo, &after, &before, "undo").unwrap();
    assert_eq!(capture_counted(&repo, LIMIT).unwrap(), before);

    restore(&repo, &before, &after, "redo").unwrap();
    assert_eq!(read(&repo, "t.txt").as_deref(), Some("tracked\n"));
    assert_eq!(capture_counted(&repo, LIMIT).unwrap(), after);
}

#[test]
fn a_not_saved_untracked_refusal_names_the_file_and_the_limit() {
    assert_eq!(
        UndoError::NotSavedUntracked("notes.txt".into()).to_string(),
        "notes.txt is one of more than 10,000 untracked files, so its contents weren't saved \
         for undo, and it has changed since."
    );
}
