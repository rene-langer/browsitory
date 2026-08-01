mod common;

use common::{init_repo, write_file};
use git_core::open;

#[test]
fn open_discovers_repo_from_a_subdirectory() {
    let (dir, _repo) = init_repo();
    write_file(&dir, "nested/file.txt", "content\n");

    let repo = open(dir.path().join("nested")).unwrap();

    assert_eq!(
        repo.workdir().unwrap().canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
}

#[test]
fn open_fails_outside_any_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    assert!(open(dir.path()).is_err());
}
