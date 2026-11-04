//! A libgit2 checkout that stops part-way (#321): on Windows at a file another program holds
//! open, on any platform at any other error. Here a read-only file (or folder) stops it on Unix
//! at the same point. Each operation puts back what the checkout had already written, so trying
//! again once the file is writable (or closed) finishes it, and the user's own edits stay.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::{commit_all, init_repo, write_file};
use git2::{Oid, Repository};
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};
use tempfile::TempDir;

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
                out.insert(
                    rel.replace('\\', "/"),
                    std::fs::read_to_string(&path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// The index on disk, path to blob id and stage, read through a fresh handle.
fn index_on_disk(dir: &Path) -> Vec<(String, Oid, u16)> {
    let repo = Repository::open(dir).unwrap();
    let index = repo.index().unwrap();
    index
        .iter()
        .map(|entry| {
            (
                String::from_utf8_lossy(&entry.path).into_owned(),
                entry.id,
                (entry.flags >> 12) & 0x3,
            )
        })
        .collect()
}

/// `git status` of `repo` (untracked files included), path and flags.
fn status(repo: &Repository) -> Vec<(String, git2::Status)> {
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    repo.statuses(Some(&mut options))
        .unwrap()
        .iter()
        .map(|entry| (entry.path().unwrap().to_string(), entry.status()))
        .collect()
}

/// What a test compares before and after a failed operation: the work tree, the index on disk,
/// and the status seen through a fresh handle and through the operation's own handle (the
/// worker keeps one handle, whose index libgit2 may have changed in memory).
#[derive(Debug, PartialEq)]
struct State {
    files: BTreeMap<String, String>,
    index: Vec<(String, Oid, u16)>,
    status: Vec<(String, git2::Status)>,
    status_own_handle: Vec<(String, git2::Status)>,
    head: Option<String>,
}

fn state(dir: &Path, repo: &Repository) -> State {
    State {
        files: files(dir),
        index: index_on_disk(dir),
        status: status(&Repository::open(dir).unwrap()),
        status_own_handle: status(repo),
        head: repo.head().unwrap().name().ok().map(str::to_string),
    }
}

fn map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(path, content)| (path.to_string(), content.to_string()))
        .collect()
}

/// Stages every change (deletions too) and commits it.
fn commit_everything(repo: &Repository, message: &str) -> Oid {
    let mut index = repo.index().unwrap();
    index.update_all(["*"].iter(), None).unwrap();
    index.write().unwrap();
    commit_all(repo, message);
    repo.head().unwrap().peel_to_commit().unwrap().id()
}

fn remove(dir: &Path, rel: &str) {
    std::fs::remove_file(dir.join(rel)).unwrap();
}

const BASE: &[(&str, &str)] = &[
    (".gitignore", "*.log\n"),
    ("a.txt", "a\n"),
    ("c.txt", "c\n"),
    ("d/m.txt", "m\n"),
    ("u.txt", "u\n"),
    ("z.txt", "z\n"),
];

/// The side branch's changes on top of `BASE`, in the order libgit2 checks them out: `c.txt`
/// deleted (removed first), then `a.txt` and the new `b.txt` written before `d/m.txt`, and
/// `z.txt` after it. Every changed file changes size.
fn write_side_changes(dir: &Path) {
    write_file(dir, "a.txt", "side a, longer\n");
    write_file(dir, "b.txt", "side b\n");
    remove(dir, "c.txt");
    write_file(dir, "d/m.txt", "side m, longer\n");
    write_file(dir, "z.txt", "side z, longer\n");
}

const SIDE: &[(&str, &str)] = &[
    (".gitignore", "*.log\n"),
    ("a.txt", "side a, longer\n"),
    ("b.txt", "side b\n"),
    ("d/m.txt", "side m, longer\n"),
    ("u.txt", "u\n"),
    ("z.txt", "side z, longer\n"),
];

struct Fixture {
    dir: TempDir,
    repo: Repository,
    main: String,
    /// The side branch's commit (`SIDE`, a child of the base commit).
    side: Oid,
}

/// `BASE` committed on the main branch, and the branch `side` with `SIDE` on top. With
/// `main_work`, the main branch then gets a commit of its own adding `main.txt`. HEAD is on the
/// main branch.
fn fixture(main_work: bool) -> Fixture {
    let (dir, repo) = init_repo();
    for (path, content) in BASE {
        write_file(dir.path(), path, content);
    }
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
    write_side_changes(dir.path());
    let side = commit_everything(&repo, "side");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    if main_work {
        write_file(dir.path(), "main.txt", "main\n");
        commit_all(&repo, "main work");
    }
    Fixture {
        dir,
        repo,
        main,
        side,
    }
}

/// Unix: makes the file `d/m.txt` and its folder `d` read-only until dropped. libgit2 then can
/// neither write the file in place (Linux) nor replace it (macOS) and stops there, as at a file
/// another program holds open on Windows. `None` as root, who ignores the mode.
#[cfg(unix)]
struct ReadOnly {
    /// Each path with its mode before, the folder last (put back first).
    paths: Vec<(std::path::PathBuf, u32)>,
}

#[cfg(unix)]
impl ReadOnly {
    fn new(dir: &Path) -> Option<Self> {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            // CI must never pass these tests without running them.
            assert!(
                std::env::var_os("CI").is_none(),
                "running as root on CI: root writes read-only files, so this test would prove \
                 nothing"
            );
            eprintln!(
                "SKIP: running as root, who writes read-only files; this test proves nothing"
            );
            return None;
        }
        let mut paths = Vec::new();
        for (rel, read_only) in [("d/m.txt", 0o444), ("d", 0o555)] {
            let path = dir.join(rel);
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(read_only)).unwrap();
            paths.push((path, mode));
        }
        Some(Self { paths })
    }
}

#[cfg(unix)]
impl Drop for ReadOnly {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        for (path, mode) in self.paths.iter().rev() {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(*mode));
        }
    }
}

/// What stops the next checkout at `d/m.txt` until dropped.
enum Stopper {
    #[cfg(unix)]
    ReadOnly(#[allow(dead_code)] ReadOnly),
    #[cfg(windows)]
    Held(#[allow(dead_code)] std::fs::File),
}

/// Stops the next checkout at `d/m.txt`, after it wrote the files before it: Unix makes the
/// file and its folder read-only, Windows holds it open without sharing delete. `None` as root.
fn stop_at_m(dir: &Path) -> Option<Stopper> {
    #[cfg(unix)]
    return ReadOnly::new(dir).map(Stopper::ReadOnly);
    #[cfg(windows)]
    return Some(Stopper::Held(common::hold_open_without_share_delete(
        &dir.join("d/m.txt"),
    )));
}

/// On Windows the error names the held file with #302's plain message; elsewhere libgit2's own
/// error stays.
#[track_caller]
fn assert_held_and_rolled_back(error: &dyn std::error::Error) {
    if cfg!(windows) {
        assert_eq!(
            error.to_string(),
            "'d/m.txt' is open in another program. Close it, then try again.",
        );
    } else {
        assert!(!error.to_string().contains("open in another program"));
    }
}

#[test]
fn a_cherry_pick_stopped_part_way_is_rolled_back_and_finishes_when_tried_again() {
    let f = fixture(true);
    write_file(f.dir.path(), "u.txt", "my own edit\n");
    let before = state(f.dir.path(), &f.repo);
    let Some(stopper) = stop_at_m(f.dir.path()) else {
        return;
    };

    let error = git_core::pick::cherry_pick(&f.repo, &f.side.to_string(), None, Default::default())
        .unwrap_err();

    drop(stopper);
    assert_held_and_rolled_back(&error);
    if cfg!(windows) {
        assert!(
            matches!(&error, git_core::pick::PickError::HeldOpen(held) if held.path == "d/m.txt"),
            "{error:?}"
        );
    }
    assert_eq!(state(f.dir.path(), &f.repo), before);
    assert_eq!(f.repo.state(), git2::RepositoryState::Clean);

    let outcome =
        git_core::pick::cherry_pick(&f.repo, &f.side.to_string(), None, Default::default())
            .unwrap();

    assert!(
        matches!(outcome, git_core::pick::PickOutcome::Committed { .. }),
        "{outcome:?}"
    );
    let mut expected = map(SIDE);
    expected.insert("main.txt".into(), "main\n".into());
    expected.insert("u.txt".into(), "my own edit\n".into());
    assert_eq!(files(f.dir.path()), expected);
    assert_eq!(
        status(&f.repo),
        vec![("u.txt".to_string(), git2::Status::WT_MODIFIED)]
    );
}

#[test]
fn a_revert_stopped_part_way_is_rolled_back_and_finishes_when_tried_again() {
    let f = fixture(true);
    let picked =
        match git_core::pick::cherry_pick(&f.repo, &f.side.to_string(), None, Default::default())
            .unwrap()
        {
            git_core::pick::PickOutcome::Committed { commit_id } => commit_id,
            other => panic!("{other:?}"),
        };
    write_file(f.dir.path(), "u.txt", "my own edit\n");
    let before = state(f.dir.path(), &f.repo);
    let Some(stopper) = stop_at_m(f.dir.path()) else {
        return;
    };

    let error = git_core::pick::revert(&f.repo, &picked, None, Default::default()).unwrap_err();

    drop(stopper);
    assert_held_and_rolled_back(&error);
    assert_eq!(state(f.dir.path(), &f.repo), before);
    assert_eq!(f.repo.state(), git2::RepositoryState::Clean);

    git_core::pick::revert(&f.repo, &picked, None, Default::default()).unwrap();

    let mut expected = map(BASE);
    expected.insert("main.txt".into(), "main\n".into());
    expected.insert("u.txt".into(), "my own edit\n".into());
    assert_eq!(files(f.dir.path()), expected);
    assert_eq!(
        status(&f.repo),
        vec![("u.txt".to_string(), git2::Status::WT_MODIFIED)]
    );
}

#[test]
fn a_rebase_start_stopped_part_way_is_rolled_back_and_finishes_when_tried_again() {
    let f = fixture(true);
    write_file(f.dir.path(), "notes.txt", "my own notes\n");
    let main_work = f
        .repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    let plan = || {
        vec![RebasePlanEntry {
            commit_id: main_work.clone(),
            action: RebaseAction::Pick,
            combined_message: None,
        }]
    };
    let before = state(f.dir.path(), &f.repo);
    let Some(stopper) = stop_at_m(f.dir.path()) else {
        return;
    };

    let error = git_core::rebase::start_rebase(&f.repo, "side", plan()).unwrap_err();

    drop(stopper);
    assert_held_and_rolled_back(&error);
    assert_eq!(state(f.dir.path(), &f.repo), before);

    let (_state, result) = git_core::rebase::start_rebase(&f.repo, "side", plan()).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(
        f.repo.head().unwrap().shorthand().ok(),
        Some(f.main.as_str())
    );
    let mut expected = map(SIDE);
    expected.insert("main.txt".into(), "main\n".into());
    expected.insert("notes.txt".into(), "my own notes\n".into());
    assert_eq!(files(f.dir.path()), expected);
    assert_eq!(
        status(&f.repo),
        vec![("notes.txt".to_string(), git2::Status::WT_NEW)]
    );
}

#[test]
fn a_fast_forward_pull_stopped_part_way_is_rolled_back_and_finishes_when_tried_again() {
    let f = fixture(false);
    f.repo
        .reference("refs/remotes/origin/main", f.side, true, "test: fetched")
        .unwrap();
    // An ignored file: the only kind a pull allows.
    write_file(f.dir.path(), "build.log", "my own log\n");
    let before = state(f.dir.path(), &f.repo);
    let Some(stopper) = stop_at_m(f.dir.path()) else {
        return;
    };

    let error = git_core::remote::pull_after_fetch(&f.repo, "origin", "main").unwrap_err();

    drop(stopper);
    assert_held_and_rolled_back(&error);
    assert_eq!(state(f.dir.path(), &f.repo), before);

    let outcome = git_core::remote::pull_after_fetch(&f.repo, "origin", "main").unwrap();

    assert!(
        matches!(outcome, git_core::remote::PullOutcome::FastForwarded { .. }),
        "{outcome:?}"
    );
    assert_eq!(f.repo.head().unwrap().target(), Some(f.side));
    let mut expected = map(SIDE);
    expected.insert("build.log".into(), "my own log\n".into());
    assert_eq!(files(f.dir.path()), expected);
    assert!(status(&f.repo).is_empty(), "{:?}", status(&f.repo));
}

/// A stash of `SIDE`'s changes (`b.txt` untracked) on the main branch, then the user's own
/// edit of `u.txt`.
fn stashed() -> Option<(Fixture, State, Stopper)> {
    let mut f = fixture(true);
    write_side_changes(f.dir.path());
    git_core::stash::save_stash(&mut f.repo, &Default::default()).unwrap();
    write_file(f.dir.path(), "u.txt", "my own edit\n");
    let before = state(f.dir.path(), &f.repo);
    let stopper = stop_at_m(f.dir.path())?;
    Some((f, before, stopper))
}

fn stash_applied() -> BTreeMap<String, String> {
    let mut expected = map(SIDE);
    expected.insert("main.txt".into(), "main\n".into());
    expected.insert("u.txt".into(), "my own edit\n".into());
    expected
}

#[test]
fn a_stash_apply_stopped_part_way_is_rolled_back_and_finishes_when_tried_again() {
    let Some((mut f, before, stopper)) = stashed() else {
        return;
    };

    let error = git_core::stash::apply_stash(&mut f.repo, 0, None, false).unwrap_err();

    drop(stopper);
    assert_held_and_rolled_back(&error);
    if cfg!(windows) {
        assert!(
            matches!(&error, git_core::stash::StashError::HeldOpen(held) if held.path == "d/m.txt"),
            "{error:?}"
        );
    }
    assert_eq!(state(f.dir.path(), &f.repo), before);

    let outcome = git_core::stash::apply_stash(&mut f.repo, 0, None, false).unwrap();

    assert_eq!(outcome, git_core::stash::StashApplyOutcome::Applied);
    assert_eq!(files(f.dir.path()), stash_applied());
    assert_eq!(git_core::stash::list_stashes(&mut f.repo).unwrap().len(), 1);
}

#[test]
fn a_stash_pop_stopped_part_way_is_rolled_back_keeps_the_stash_and_finishes_when_tried_again() {
    let Some((mut f, before, stopper)) = stashed() else {
        return;
    };

    let error = git_core::stash::pop_stash(&mut f.repo, 0, None, false).unwrap_err();

    drop(stopper);
    assert_held_and_rolled_back(&error);
    assert_eq!(state(f.dir.path(), &f.repo), before);
    assert_eq!(git_core::stash::list_stashes(&mut f.repo).unwrap().len(), 1);

    let outcome = git_core::stash::pop_stash(&mut f.repo, 0, None, false).unwrap();

    assert_eq!(outcome, git_core::stash::StashApplyOutcome::Applied);
    assert_eq!(files(f.dir.path()), stash_applied());
    assert!(git_core::stash::list_stashes(&mut f.repo)
        .unwrap()
        .is_empty());
}

/// A merge of `side` into the main branch paused on a conflict in `x.txt`, with every other
/// change of `SIDE` staged, and the user's own edit of `u.txt`.
fn paused_merge() -> Fixture {
    let (dir, repo) = init_repo();
    for (path, content) in BASE {
        write_file(dir.path(), path, content);
    }
    write_file(dir.path(), "x.txt", "x\n");
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
    write_side_changes(dir.path());
    write_file(dir.path(), "x.txt", "side x\n");
    let side = commit_everything(&repo, "side");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    write_file(dir.path(), "x.txt", "main x\n");
    commit_all(&repo, "main x");
    let outcome = git_core::merge::start_merge(&repo, "side", Default::default()).unwrap();
    assert!(
        matches!(outcome, git_core::merge::MergeOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    write_file(dir.path(), "u.txt", "my own edit\n");
    Fixture {
        dir,
        repo,
        main,
        side,
    }
}

fn merge_aborted() -> BTreeMap<String, String> {
    let mut expected = map(BASE);
    expected.insert("x.txt".into(), "main x\n".into());
    expected.insert("u.txt".into(), "my own edit\n".into());
    expected
}

#[test]
fn a_merge_abort_stopped_part_way_keeps_the_merge_to_abort_again() {
    let f = paused_merge();
    let Some(stopper) = stop_at_m(f.dir.path()) else {
        return;
    };

    let error = git_core::merge::abort_merge(&f.repo).unwrap_err();

    drop(stopper);
    if cfg!(windows) {
        assert_eq!(
            error.to_string(),
            "'d/m.txt' is open in another program. Some files were already put back. Close it, \
             then abort again to finish.",
        );
        assert!(
            matches!(&error, git_core::merge::MergeError::HeldOpen(held) if held.path == "d/m.txt"),
            "{error:?}"
        );
    }
    assert!(git_core::merge::is_merging(&f.repo));
    assert!(!f.dir.path().join("b.txt").exists());
    // Linux: the files put back so far (`a.txt`, `c.txt`, and `b.txt`, which the merge added)
    // are as HEAD has them; the rest of the merge, its conflict included, is still there to
    // abort. (Where libgit2 stops elsewhere, before or after writing them, isn't pinned.)
    #[cfg(target_os = "linux")]
    {
        let expected = vec![
            ("d/m.txt".to_string(), git2::Status::INDEX_MODIFIED),
            ("u.txt".to_string(), git2::Status::WT_MODIFIED),
            ("x.txt".to_string(), git2::Status::CONFLICTED),
            ("z.txt".to_string(), git2::Status::INDEX_MODIFIED),
        ];
        assert_eq!(status(&Repository::open(f.dir.path()).unwrap()), expected);
        assert_eq!(status(&f.repo), expected);
        assert_eq!(common::read(f.dir.path(), "a.txt"), "a\n");
        assert_eq!(common::read(f.dir.path(), "c.txt"), "c\n");
    }

    git_core::merge::abort_merge(&f.repo).unwrap();

    assert!(!git_core::merge::is_merging(&f.repo));
    assert_eq!(files(f.dir.path()), merge_aborted());
    assert_eq!(
        status(&Repository::open(f.dir.path()).unwrap()),
        vec![("u.txt".to_string(), git2::Status::WT_MODIFIED)]
    );
    assert_eq!(
        status(&f.repo),
        vec![("u.txt".to_string(), git2::Status::WT_MODIFIED)]
    );
}

/// Windows: a merge abort that can't remove a file the merge added, because another program
/// holds it open, names it (not an I/O error), puts back what it can, and finishes when
/// aborted again once the file is closed (#321 review).
#[cfg(windows)]
#[test]
fn a_merge_abort_at_a_held_file_the_merge_added_says_so_and_finishes_when_aborted_again() {
    let f = paused_merge();
    let editor = common::hold_open_without_share_delete(&f.dir.path().join("b.txt"));

    let error = git_core::merge::abort_merge(&f.repo).unwrap_err();

    drop(editor);
    assert_eq!(
        error.to_string(),
        "'b.txt' is open in another program. Some files were already put back. Close it, then \
         abort again to finish.",
    );
    assert!(
        matches!(&error, git_core::merge::MergeError::HeldOpen(held) if held.path == "b.txt"),
        "{error:?}"
    );
    assert!(git_core::merge::is_merging(&f.repo));

    git_core::merge::abort_merge(&f.repo).unwrap();

    assert!(!git_core::merge::is_merging(&f.repo));
    assert_eq!(files(f.dir.path()), merge_aborted());
    assert_eq!(
        status(&f.repo),
        vec![("u.txt".to_string(), git2::Status::WT_MODIFIED)]
    );
}

/// Windows: a rebase abort whose checkout stops before it changed anything says so plainly,
/// not "Some files were already put back" (#321 review), and finishes when aborted again.
#[cfg(windows)]
#[test]
fn a_rebase_abort_stopped_before_writing_anything_says_so_plainly() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "base\n");
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::branch::create_branch(&repo, "onto", "HEAD").unwrap();
    write_file(dir.path(), "f.txt", "onto\n");
    commit_all(&repo, "onto");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    write_file(dir.path(), "f.txt", "main\n");
    commit_all(&repo, "main");
    let main_tip = repo.head().unwrap().target().unwrap();
    let plan = vec![RebasePlanEntry {
        commit_id: main_tip.to_string(),
        action: RebaseAction::Pick,
        combined_message: None,
    }];
    let (state, result) = git_core::rebase::start_rebase(&repo, "onto", plan).unwrap();
    assert!(
        matches!(result, RebaseStepResult::Conflicted { .. }),
        "{result:?}"
    );
    // The only file the abort has to put back.
    let editor = common::hold_open_without_share_delete(&dir.path().join("f.txt"));

    let error = git_core::rebase::abort_rebase(&repo, &state).unwrap_err();

    drop(editor);
    assert_eq!(
        error.to_string(),
        "'f.txt' is open in another program. Close it, then try again.",
    );

    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert!(abort.restored);
    assert_eq!(repo.head().unwrap().shorthand().ok(), Some(main.as_str()));
    assert_eq!(common::read(dir.path(), "f.txt"), "main\n");
}
