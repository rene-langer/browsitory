mod common;

use git_core::stage::{restore_file_version, StageError};
use git_core::tree::{
    file_at_commit, find_in_tree, list_tree, save_file_version, FileContent, TreeEntryKind,
    TreeError, MAX_VIEW_BYTES,
};

/// `common::commit_all` with deletions staged too.
fn commit_everything(repo: &git2::Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index.update_all(["*"].iter(), None).unwrap();
    index.write().unwrap();
    common::commit_all(repo, message);
}

fn names(listing: &git_core::tree::TreeListing) -> Vec<(&str, TreeEntryKind)> {
    listing
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.kind))
        .collect()
}

#[test]
fn lists_one_folder_at_a_time_folders_first() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "b.txt", "bee\n");
    common::write_file(dir.path(), "A.md", "a\n");
    common::write_file(dir.path(), "src/main.rs", "fn main() {}\n");
    common::write_file(dir.path(), "src/deep/x.rs", "x\n");
    common::write_file(dir.path(), "docs/guide.md", "g\n");
    common::commit_all(&repo, "base");
    let id = common::head_id(&repo).to_string();

    let root = list_tree(&repo, &id, "").unwrap();
    assert_eq!(
        names(&root),
        vec![
            ("docs", TreeEntryKind::Folder),
            ("src", TreeEntryKind::Folder),
            ("A.md", TreeEntryKind::File),
            ("b.txt", TreeEntryKind::File),
        ]
    );
    assert!(!root.truncated);
    let b = root.entries.iter().find(|e| e.path == "b.txt").unwrap();
    assert_eq!(b.size, Some(4));
    assert_eq!(b.name, "b.txt");
    assert_eq!(root.entries[0].size, None);

    let src = list_tree(&repo, &id, "src").unwrap();
    assert_eq!(
        names(&src),
        vec![
            ("src/deep", TreeEntryKind::Folder),
            ("src/main.rs", TreeEntryKind::File),
        ]
    );
}

#[test]
fn lists_the_tree_of_an_older_commit() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "old.txt", "o\n");
    common::commit_all(&repo, "first");
    let first = common::head_id(&repo).to_string();
    std::fs::remove_file(dir.path().join("old.txt")).unwrap();
    common::write_file(dir.path(), "new.txt", "n\n");
    commit_everything(&repo, "second");

    let listing = list_tree(&repo, &first, "").unwrap();
    assert_eq!(names(&listing), vec![("old.txt", TreeEntryKind::File)]);
}

#[test]
fn refuses_a_missing_folder_a_file_and_an_unsafe_path() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "f\n");
    common::commit_all(&repo, "base");
    let id = common::head_id(&repo).to_string();

    assert!(matches!(
        list_tree(&repo, &id, "nope"),
        Err(TreeError::NotFound(_))
    ));
    assert!(matches!(
        list_tree(&repo, &id, "f.txt"),
        Err(TreeError::NotAFolder(_))
    ));
    assert!(matches!(
        list_tree(&repo, &id, "../x"),
        Err(TreeError::InvalidPath(_))
    ));
}

#[cfg(unix)]
#[test]
fn reports_executables_and_symlinks() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "run.sh", "#!/bin/sh\n");
    std::fs::set_permissions(
        dir.path().join("run.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    std::os::unix::fs::symlink("run.sh", dir.path().join("link")).unwrap();
    common::commit_all(&repo, "base");
    let id = common::head_id(&repo).to_string();

    let listing = list_tree(&repo, &id, "").unwrap();
    assert_eq!(
        names(&listing),
        vec![
            ("link", TreeEntryKind::Symlink),
            ("run.sh", TreeEntryKind::Executable),
        ]
    );
}

#[test]
fn finds_files_by_path_case_insensitively() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "src/Main.rs", "m\n");
    common::write_file(dir.path(), "src/lib.rs", "l\n");
    common::write_file(dir.path(), "docs/main-guide.md", "g\n");
    common::commit_all(&repo, "base");
    let id = common::head_id(&repo).to_string();

    let found = find_in_tree(&repo, &id, "MAIN", 10).unwrap();
    assert_eq!(
        names(&found),
        vec![
            ("docs/main-guide.md", TreeEntryKind::File),
            ("src/Main.rs", TreeEntryKind::File),
        ]
    );
    assert!(!found.truncated);
    assert_eq!(found.entries[1].name, "Main.rs");

    // Folder names match through the paths of the files in them; folders themselves aren't listed.
    let in_src = find_in_tree(&repo, &id, "src/", 10).unwrap();
    assert_eq!(in_src.entries.len(), 2);

    assert!(find_in_tree(&repo, &id, "  ", 10)
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn stops_finding_at_the_limit() {
    let (dir, repo) = common::init_repo();
    for i in 0..5 {
        common::write_file(dir.path(), &format!("f{i}.txt"), "x\n");
    }
    common::commit_all(&repo, "base");
    let id = common::head_id(&repo).to_string();

    let found = find_in_tree(&repo, &id, "f", 3).unwrap();
    assert_eq!(found.entries.len(), 3);
    assert!(found.truncated);
    let exact = find_in_tree(&repo, &id, "f", 5).unwrap();
    assert_eq!(exact.entries.len(), 5);
    assert!(!exact.truncated);
}

#[test]
fn reads_text_binary_and_too_large_files() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "hello\n");
    std::fs::write(dir.path().join("b.bin"), [0u8, 1, 2, 3]).unwrap();
    common::commit_all(&repo, "base");
    let id = common::head_id(&repo).to_string();
    common::write_file(dir.path(), "a.txt", "changed in the work tree\n");

    assert_eq!(
        file_at_commit(&repo, &id, "a.txt", MAX_VIEW_BYTES).unwrap(),
        FileContent::Text {
            text: "hello\n".to_string(),
            size: 6
        }
    );
    assert_eq!(
        file_at_commit(&repo, &id, "b.bin", MAX_VIEW_BYTES).unwrap(),
        FileContent::Binary {
            bytes: vec![0, 1, 2, 3]
        }
    );
    assert_eq!(
        file_at_commit(&repo, &id, "a.txt", 3).unwrap(),
        FileContent::TooLarge { size: 6 }
    );
    assert!(matches!(
        file_at_commit(&repo, &id, "missing.txt", MAX_VIEW_BYTES),
        Err(TreeError::NotFound(_))
    ));
}

#[test]
fn saves_a_version_to_another_place() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "v1\n");
    common::commit_all(&repo, "first");
    let first = common::head_id(&repo).to_string();
    common::write_file(dir.path(), "a.txt", "v2\n");
    common::commit_all(&repo, "second");

    let outside = common::outside_dir();
    let destination = outside.path().join("saved.txt");
    save_file_version(&repo, &first, "a.txt", &destination).unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), b"v1\n");
    assert_eq!(common::read(dir.path(), "a.txt"), "v2\n");
}

#[test]
fn restores_a_version_to_the_work_tree_without_staging_it() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "v1\n");
    common::commit_all(&repo, "first");
    let first = common::head_id(&repo).to_string();
    common::write_file(dir.path(), "a.txt", "v2\n");
    common::commit_all(&repo, "second");
    common::write_file(dir.path(), "a.txt", "local edit\n");

    restore_file_version(&repo, &first, "a.txt").unwrap();

    assert_eq!(common::read(dir.path(), "a.txt"), "v1\n");
    let statuses = git_core::status::status(&repo).unwrap();
    assert!(statuses.iter().any(|e| e.path == "a.txt" && !e.staged));
    assert!(!statuses.iter().any(|e| e.path == "a.txt" && e.staged));
}

#[test]
fn restores_a_file_deleted_since_and_its_folder() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "keep.txt", "k\n");
    common::write_file(dir.path(), "gone/old.txt", "old\n");
    common::commit_all(&repo, "first");
    let first = common::head_id(&repo).to_string();
    std::fs::remove_dir_all(dir.path().join("gone")).unwrap();
    commit_everything(&repo, "second");

    restore_file_version(&repo, &first, "gone/old.txt").unwrap();

    assert_eq!(common::read(dir.path(), "gone/old.txt"), "old\n");
}

#[test]
fn restore_refuses_a_path_that_is_not_a_file_in_the_commit() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "src/a.txt", "a\n");
    common::commit_all(&repo, "base");
    let id = common::head_id(&repo).to_string();

    assert!(matches!(
        restore_file_version(&repo, &id, "src"),
        Err(StageError::NotAFileInCommit(_))
    ));
    assert!(matches!(
        restore_file_version(&repo, &id, "nope.txt"),
        Err(StageError::NotAFileInCommit(_))
    ));
    assert!(matches!(
        restore_file_version(&repo, &id, "../x"),
        Err(StageError::InvalidPath)
    ));
}

#[test]
fn restore_refuses_when_a_file_took_a_folders_place() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "d/f.txt", "f\n");
    common::commit_all(&repo, "base");
    let id = common::head_id(&repo).to_string();
    std::fs::remove_dir_all(dir.path().join("d")).unwrap();
    common::write_file(dir.path(), "d", "now a file\n");

    assert!(matches!(
        restore_file_version(&repo, &id, "d/f.txt"),
        Err(StageError::RestoreBlockedByFile { .. })
    ));
    assert_eq!(common::read(dir.path(), "d"), "now a file\n");
}

#[cfg(unix)]
#[test]
fn restore_never_writes_through_a_symlinked_folder() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "d/f.txt", "f\n");
    common::commit_all(&repo, "base");
    let id = common::head_id(&repo).to_string();
    let outside = common::outside_dir();
    std::fs::remove_dir_all(dir.path().join("d")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("d")).unwrap();

    assert!(restore_file_version(&repo, &id, "d/f.txt").is_err());
    common::assert_outside_untouched(outside.path());
    assert!(!outside.path().join("f.txt").exists());
}
