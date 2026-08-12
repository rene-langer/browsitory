mod common;

use common::init_repo;

#[test]
fn opens_an_existing_repository() {
    let (dir, _repo) = init_repo();

    let result = git_core::repo::open(dir.path());

    assert!(result.is_ok());
}

#[test]
fn discovers_repository_from_a_subdirectory() {
    let (dir, _repo) = init_repo();
    let subdir = dir.path().join("nested/sub/dir");
    std::fs::create_dir_all(&subdir).unwrap();

    let result = git_core::repo::open(&subdir);

    assert!(result.is_ok());
}

#[test]
fn fails_on_a_non_repository_path() {
    let dir = tempfile::TempDir::new().unwrap();

    let result = git_core::repo::open(dir.path());

    assert!(result.is_err());
}
