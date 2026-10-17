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

fn expected_initial_branch() -> String {
    git2::Config::open_default()
        .ok()
        .and_then(|config| config.get_string("init.defaultBranch").ok())
        .filter(|branch| !branch.is_empty())
        .unwrap_or_else(|| "main".to_string())
}

#[test]
fn init_creates_a_repository_with_missing_parents() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("a/b/new-repo");

    git_core::repo::init_repo(&path).unwrap();

    let repo = git2::Repository::open(&path).unwrap();
    let head = repo.find_reference("HEAD").unwrap();
    assert_eq!(
        head.symbolic_target().unwrap(),
        Some(format!("refs/heads/{}", expected_initial_branch()).as_str())
    );
}

#[test]
fn init_uses_an_existing_empty_directory() {
    let dir = tempfile::TempDir::new().unwrap();

    git_core::repo::init_repo(dir.path()).unwrap();

    assert!(dir.path().join(".git").is_dir());
}

#[test]
fn init_refuses_an_existing_repository() {
    let (dir, _repo) = init_repo();

    let result = git_core::repo::init_repo(dir.path());

    assert!(matches!(
        result,
        Err(git_core::repo::RepoError::AlreadyRepository)
    ));
}
