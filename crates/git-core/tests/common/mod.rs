use std::path::Path;

use git2::Repository;
use tempfile::TempDir;

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
pub fn write_file(dir: &Path, relative_path: &str, contents: &str) {
    let full_path = dir.join(relative_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(full_path, contents).expect("write file");
}
