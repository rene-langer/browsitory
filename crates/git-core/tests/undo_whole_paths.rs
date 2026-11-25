//! #1104: the snapshot of a command that destroys work-tree content stores whole only the paths
//! the command can change (files, and everything below folders, matched ignoring case); every
//! other file is under `Full`'s limits. `capture_limited_except` is `capture_keeping` with small
//! limits, so these need no 10,000 files or 50 MB.

// Hide the developer's / CI runner's global and system git config from every test (#111).
test_support::hermetic_git!();

use std::fs;
use std::path::Path;

use git2::Repository;
use git_core::undo::{capture_limited_except, restore, unsaved_untracked, RepoSnapshot};
use tempfile::TempDir;

const FILE_LIMIT: u64 = 10;
const UNTRACKED_LIMIT: usize = 3;

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
    write(&repo, "big.txt", "small\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("a.txt")).unwrap();
    index.add_path(Path::new("big.txt")).unwrap();
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

fn unsaved(repo: &Repository, snapshot: &RepoSnapshot) -> Vec<String> {
    unsaved_untracked(repo, snapshot)
        .unwrap()
        .into_keys()
        .collect()
}

fn owned(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

const LONG: &str = "more than ten bytes\n";

#[test]
fn named_files_are_stored_whatever_their_size_the_others_under_the_limits() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", LONG);
    write(&repo, "big.txt", LONG);
    for i in 0..4 {
        write(&repo, &format!("u/{i}.txt"), "u\n");
    }
    write(&repo, "keep/x.txt", LONG);
    write(&repo, "keep/y.txt", "y\n");
    write(&repo, "Other.TXT", LONG);

    let whole = owned(&["a.txt", "keep/", "other.txt"]);
    let snapshot = capture_limited_except(&repo, FILE_LIMIT, UNTRACKED_LIMIT, &whole).unwrap();

    assert_eq!(snapshot.skipped.keys().collect::<Vec<_>>(), vec!["big.txt"]);
    assert_eq!(
        unsaved(&repo, &snapshot),
        ["u/0.txt", "u/1.txt", "u/2.txt", "u/3.txt"]
    );
    assert_eq!(
        tree_paths(&repo, &snapshot),
        ["Other.TXT", "a.txt", "keep/x.txt", "keep/y.txt"]
    );
}

/// The untracked limit counts only the files not named: three of them and two named ones are
/// all stored.
#[test]
fn named_untracked_files_do_not_count_toward_the_limit() {
    let (_dir, repo) = seeded();
    for i in 0..3 {
        write(&repo, &format!("u/{i}.txt"), "u\n");
    }
    write(&repo, "d/1.txt", "1\n");
    write(&repo, "d/2.txt", "2\n");

    let whole = owned(&["d"]);
    let snapshot = capture_limited_except(&repo, FILE_LIMIT, UNTRACKED_LIMIT, &whole).unwrap();

    assert_eq!(snapshot.unsaved_untracked, None);
    assert_eq!(
        tree_paths(&repo, &snapshot),
        ["a.txt", "big.txt", "d/1.txt", "d/2.txt", "u/0.txt", "u/1.txt", "u/2.txt"]
    );
}

/// "d" names the folder `d/`, not a file whose name only starts with it.
#[test]
fn a_named_folder_is_not_a_prefix_of_other_names() {
    let (_dir, repo) = seeded();
    for i in 0..4 {
        write(&repo, &format!("u/{i}.txt"), "u\n");
    }
    write(&repo, "dd.txt", "x\n");

    let whole = owned(&["d"]);
    let snapshot = capture_limited_except(&repo, FILE_LIMIT, UNTRACKED_LIMIT, &whole).unwrap();

    assert!(unsaved(&repo, &snapshot).contains(&"dd.txt".to_string()));
}

#[test]
fn a_discard_between_such_snapshots_undoes_and_redoes() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", LONG);
    for i in 0..5 {
        write(&repo, &format!("u/{i}.txt"), "u\n");
    }
    let whole = owned(&["a.txt"]);
    let before = capture_limited_except(&repo, FILE_LIMIT, UNTRACKED_LIMIT, &whole).unwrap();
    git_core::stage::discard_file(&repo, "a.txt").unwrap();
    let after = capture_limited_except(&repo, FILE_LIMIT, UNTRACKED_LIMIT, &whole).unwrap();
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("one\n"));

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(read(&repo, "a.txt").as_deref(), Some(LONG));
    assert_eq!(read(&repo, "u/0.txt").as_deref(), Some("u\n"));

    restore(&repo, &before, &after, "browsitory: redo test").unwrap();
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("one\n"));
    assert_eq!(read(&repo, "u/4.txt").as_deref(), Some("u\n"));
}

/// `capture_keeping` and `capture_worktree` with real limits store a named file over
/// `SNAPSHOT_FILE_LIMIT` (sparse: no 50 MB written), and leave out one not named.
#[test]
fn the_real_captures_store_a_named_big_file() {
    let (_dir, repo) = seeded();
    let workdir = repo.workdir().unwrap().to_path_buf();
    for name in ["named.bin", "other.bin"] {
        fs::File::create(workdir.join(name))
            .unwrap()
            .set_len(git_core::undo::SNAPSHOT_FILE_LIMIT + 1)
            .unwrap();
    }
    let whole = owned(&["named.bin"]);

    let kept = git_core::undo::capture_keeping(&repo, &whole).unwrap();
    let worktree = git_core::undo::capture_worktree(&repo, Some(&whole)).unwrap();

    for snapshot in [&kept, &worktree] {
        assert_eq!(
            snapshot.skipped.keys().collect::<Vec<_>>(),
            vec!["other.bin"]
        );
        assert!(tree_paths(&repo, snapshot).contains(&"named.bin".to_string()));
    }
}

#[test]
fn a_patch_names_the_paths_of_every_file_it_changes() {
    let patch = b"diff --git a/b.txt b/b.txt\n\
index 5626abf..f719efd 100644\n\
--- a/b.txt\n\
+++ b/b.txt\n\
@@ -1 +1 @@\n\
-one\n\
+two\n\
diff --git a/old.txt b/dir/new.txt\n\
similarity index 100%\n\
rename from old.txt\n\
rename to dir/new.txt\n\
diff --git a/a.txt b/a.txt\n\
deleted file mode 100644\n\
index 5626abf..0000000\n\
--- a/a.txt\n\
+++ /dev/null\n\
@@ -1 +0,0 @@\n\
-one\n";
    assert_eq!(
        git_core::patch::touched_paths(patch),
        Some(owned(&["a.txt", "b.txt", "dir/new.txt", "old.txt"]))
    );
    assert_eq!(git_core::patch::touched_paths(b"not a patch\n"), None);
}
