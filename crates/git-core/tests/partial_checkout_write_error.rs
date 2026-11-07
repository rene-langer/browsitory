//! A libgit2 checkout that stops *while* writing a file (#329): libgit2 opens the file with
//! `O_TRUNC` and reports it done only once it is written, so a full disk or an I/O error leaves
//! it truncated and unreported. The rollback puts it back too. Here the process's file size
//! limit (`RLIMIT_FSIZE`) stops the write part-way on Unix, as a full disk would. The limit is
//! the whole process's, so this file's tests take turns, and nothing else runs in this binary.

#![cfg(unix)]

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use common::{commit_all, init_repo, write_file};
use git2::Repository;

static LIMIT: Mutex<()> = Mutex::new(());

/// Files this process writes stop growing at `bytes` (a write past it fails with `EFBIG`)
/// until dropped. `SIGXFSZ`, which would end the process, is ignored for good.
struct FileSizeLimit {
    before: libc::rlimit,
}

impl FileSizeLimit {
    fn new(bytes: u64) -> Self {
        unsafe {
            libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
            let mut before = std::mem::zeroed::<libc::rlimit>();
            assert_eq!(libc::getrlimit(libc::RLIMIT_FSIZE, &mut before), 0);
            let limit = libc::rlimit {
                rlim_cur: bytes as libc::rlim_t,
                rlim_max: before.rlim_max,
            };
            assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limit), 0);
            Self { before }
        }
    }
}

impl Drop for FileSizeLimit {
    fn drop(&mut self) {
        unsafe {
            libc::setrlimit(libc::RLIMIT_FSIZE, &self.before);
        }
    }
}

const LIMIT_BYTES: u64 = 16 * 1024;

/// Larger than the limit.
fn big() -> String {
    "a line of the big file\n".repeat(4096)
}

/// Every work-tree file below `dir` (not `.git`), path to content.
fn files(dir: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_name() == ".git" {
                continue;
            }
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_string_lossy();
                out.insert(rel.into_owned(), std::fs::read_to_string(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// `git status` (untracked files included), path and flags.
fn status(repo: &Repository) -> Vec<(String, git2::Status)> {
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    repo.statuses(Some(&mut options))
        .unwrap()
        .iter()
        .map(|entry| (entry.path().unwrap().to_string(), entry.status()))
        .collect()
}

/// `a.txt`, `big.txt` and `z.txt` committed on the main branch; the branch `side` changes all
/// three, `big.txt` past the limit. `main_work` adds a commit of the main branch's own.
fn fixture(main_work: bool) -> (tempfile::TempDir, Repository, git2::Oid) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "big.txt", "big\n");
    write_file(dir.path(), "z.txt", "z\n");
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
    write_file(dir.path(), "a.txt", "side a, longer\n");
    write_file(dir.path(), "big.txt", &big());
    write_file(dir.path(), "z.txt", "side z, longer\n");
    commit_all(&repo, "side");
    let side = repo.head().unwrap().target().unwrap();
    git_core::branch::switch_branch(&repo, &main).unwrap();
    if main_work {
        write_file(dir.path(), "main.txt", "main\n");
        commit_all(&repo, "main work");
    }
    (dir, repo, side)
}

#[test]
fn a_cherry_pick_stopped_while_writing_a_file_puts_that_file_back_too() {
    let _turn = LIMIT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (dir, repo, side) = fixture(true);
    write_file(dir.path(), "notes.txt", "my own notes\n");
    let before = files(dir.path());

    let error = {
        let _limit = FileSizeLimit::new(LIMIT_BYTES);
        git_core::pick::cherry_pick(&repo, &side.to_string(), None, Default::default()).unwrap_err()
    };

    assert!(
        !error.to_string().contains("open in another program"),
        "{error}"
    );
    assert_eq!(files(dir.path()), before);
    let untracked = vec![("notes.txt".to_string(), git2::Status::WT_NEW)];
    assert_eq!(status(&Repository::open(dir.path()).unwrap()), untracked);
    assert_eq!(status(&repo), untracked);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);

    let outcome =
        git_core::pick::cherry_pick(&repo, &side.to_string(), None, Default::default()).unwrap();

    assert!(
        matches!(outcome, git_core::pick::PickOutcome::Committed { .. }),
        "{outcome:?}"
    );
    assert_eq!(common::read(dir.path(), "big.txt"), big());
    assert_eq!(status(&repo), untracked);
}

/// libgit2 writes a side of a file/folder conflict under an altered name (`ab~HEAD`), which it
/// never reports, and writes conflicts last; here the conflict in `big.txt` after it stops the
/// checkout, leaving `big.txt.lock`, the temporary file its merged content went through. The
/// rollback removes both too, so trying again works (#329). (A file another program holds
/// open, or a read-only folder, stops it earlier: libgit2 removes each conflicted file before
/// it writes anything.)
#[test]
fn a_cherry_pick_stopped_at_a_conflict_removes_sides_written_under_other_names() {
    let _turn = LIMIT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "big.txt", "big\n");
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
    write_file(dir.path(), "a.txt", "side a, longer\n");
    write_file(dir.path(), "ab/g", "side g\n");
    write_file(dir.path(), "big.txt", &big());
    commit_all(&repo, "side");
    let side = repo.head().unwrap().target().unwrap();
    git_core::branch::switch_branch(&repo, &main).unwrap();
    write_file(dir.path(), "ab", "main ab\n");
    write_file(dir.path(), "big.txt", "main big\n");
    commit_all(&repo, "main");
    let before = files(dir.path());

    let error = {
        let _limit = FileSizeLimit::new(LIMIT_BYTES);
        git_core::pick::cherry_pick(&repo, &side.to_string(), None, Default::default()).unwrap_err()
    };

    assert!(
        !error.to_string().contains("open in another program"),
        "{error}"
    );
    assert_eq!(files(dir.path()), before);
    assert!(!dir.path().join("ab~HEAD").exists());
    assert!(!dir.path().join("big.txt.lock").exists());
    assert!(status(&repo).is_empty(), "{:?}", status(&repo));
    assert_eq!(repo.state(), git2::RepositoryState::Clean);

    let outcome =
        git_core::pick::cherry_pick(&repo, &side.to_string(), None, Default::default()).unwrap();

    assert!(
        matches!(&outcome, git_core::pick::PickOutcome::Conflicted { files } if files.contains(&"big.txt".to_string())),
        "{outcome:?}"
    );
    assert!(
        dir.path().join("ab~HEAD").exists(),
        "{:?}",
        files(dir.path())
    );
}

#[test]
fn a_fast_forward_pull_stopped_while_writing_a_file_puts_that_file_back_too() {
    let _turn = LIMIT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (dir, repo, side) = fixture(false);
    repo.reference("refs/remotes/origin/main", side, true, "test: fetched")
        .unwrap();
    let before = files(dir.path());

    let error = {
        let _limit = FileSizeLimit::new(LIMIT_BYTES);
        git_core::remote::pull_after_fetch(&repo, "origin", "main").unwrap_err()
    };

    assert!(
        !error.to_string().contains("open in another program"),
        "{error}"
    );
    assert_eq!(files(dir.path()), before);
    assert!(status(&repo).is_empty(), "{:?}", status(&repo));
    assert!(
        status(&Repository::open(dir.path()).unwrap()).is_empty(),
        "{:?}",
        status(&repo)
    );

    let outcome = git_core::remote::pull_after_fetch(&repo, "origin", "main").unwrap();

    assert!(
        matches!(outcome, git_core::remote::PullOutcome::FastForwarded { .. }),
        "{outcome:?}"
    );
    assert_eq!(common::read(dir.path(), "big.txt"), big());
}
