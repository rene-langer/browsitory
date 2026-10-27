use std::path::Path;

use git2::{Oid, Repository};
use tempfile::TempDir;

pub fn init_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().expect("create temp dir");
    let repo = Repository::init(dir.path()).expect("init repo");
    {
        let mut config = repo.config().expect("repo config");
        config.set_str("user.name", "Test User").unwrap();
        config.set_bool("core.autocrlf", false).unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_bool("commit.gpgSign", false).unwrap();
    }
    (dir, repo)
}

/// Stages everything in the worktree and commits it on `HEAD`, creating the first commit
/// when there is none yet.
#[allow(dead_code)]
pub fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().expect("open index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("stage all");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("write tree");
    let tree = repo.find_tree(tree_id).expect("find tree");
    let signature = repo.signature().expect("signature");

    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )
    .expect("commit");
}

#[allow(dead_code)]
pub fn write_file(dir: &Path, relative_path: &str, contents: &str) {
    let full_path = dir.join(relative_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(full_path, contents).expect("write file");
}

/// True when `program` can be spawned at all. Used to skip signing tests on machines (Windows
/// CI) without gpg / ssh-keygen.
#[allow(dead_code)]
pub fn program_available(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

#[allow(dead_code)]
pub struct SshKey {
    pub private: std::path::PathBuf,
    pub public: std::path::PathBuf,
    /// The public key file's single line, e.g. "ssh-ed25519 AAAA… test@example.com".
    pub public_line: String,
}

/// Generates a passphrase-less ed25519 key pair in `dir` plus an allowed-signers file
/// (`dir/allowed_signers`) that trusts it for principal `test@example.com`.
#[allow(dead_code)]
pub fn ssh_key_fixture(dir: &Path) -> SshKey {
    let private = dir.join("id_test");
    let status = std::process::Command::new("ssh-keygen")
        .args([
            "-q",
            "-t",
            "ed25519",
            "-N",
            "",
            "-C",
            "test@example.com",
            "-f",
        ])
        .arg(&private)
        .status()
        .expect("run ssh-keygen");
    assert!(status.success(), "ssh-keygen failed");
    let public = dir.join("id_test.pub");
    let public_line = std::fs::read_to_string(&public).unwrap().trim().to_string();
    std::fs::write(
        dir.join("allowed_signers"),
        format!("test@example.com {public_line}\n"),
    )
    .unwrap();
    SshKey {
        private,
        public,
        public_line,
    }
}

/// `commit.gpgSign=true`, `gpg.format=ssh`, `user.signingKey=<private key path>`.
#[allow(dead_code)]
pub fn enable_ssh_signing(repo: &Repository, key: &SshKey) {
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.format", "ssh").unwrap();
    config
        .set_str("user.signingKey", key.private.to_str().unwrap())
        .unwrap();
}

/// Commits exactly `files` (path, contents, mode) as the whole tree, with `parents`, without
/// touching HEAD or the worktree.
#[allow(dead_code)]
pub fn commit_tree(
    repo: &Repository,
    files: &[(&str, &str, u32)],
    parents: &[Oid],
    message: &str,
) -> Oid {
    let mut index = git2::Index::new().unwrap();
    for (path, contents, mode) in files {
        let id = repo.blob(contents.as_bytes()).unwrap();
        index
            .add(&git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: *mode,
                uid: 0,
                gid: 0,
                file_size: contents.len() as u32,
                id,
                flags: 0,
                flags_extended: 0,
                path: path.as_bytes().to_vec(),
            })
            .unwrap();
    }
    let tree = repo.find_tree(index.write_tree_to(repo).unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    let parents: Vec<git2::Commit> = parents
        .iter()
        .map(|id| repo.find_commit(*id).unwrap())
        .collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(None, &sig, &sig, message, &tree, &parent_refs)
        .unwrap()
}
