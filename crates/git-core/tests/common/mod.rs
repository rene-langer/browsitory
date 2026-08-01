use std::fs;
use std::path::Path;

use git2::Repository;
use tempfile::TempDir;

/// Initializes a fresh repo in a temp directory with a test identity
/// configured, mirroring the real on-disk repos `git.test.ts` used in the
/// old JS codebase — no mocking of git2 itself.
pub fn init_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().expect("create temp dir");
    let repo = Repository::init(dir.path()).expect("init repo");
    {
        let mut config = repo.config().expect("repo config");
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    (dir, repo)
}

#[allow(dead_code)]
pub fn write_file(dir: &TempDir, relative_path: &str, contents: &str) {
    let full_path = dir.path().join(relative_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(full_path, contents).expect("write file");
}

#[allow(dead_code)]
pub fn remove_file(dir: &TempDir, relative_path: &str) {
    fs::remove_file(dir.path().join(relative_path)).expect("remove file");
}

#[allow(dead_code)]
pub fn path_exists(dir: &TempDir, relative_path: &str) -> bool {
    Path::new(dir.path()).join(relative_path).exists()
}
