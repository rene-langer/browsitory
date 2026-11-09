//! #522: merge-like steps that bring in the other name of a case pair (`a` next to a committed
//! `A`, which only a case-sensitive drive holds) while `core.ignorecase` is on. libgit2's
//! in-memory merge holds both names, but its ignore-case index takes `a` for `A`'s entry: the
//! step's index, and the commit made from it, kept only one of them (the other's file stayed on
//! disk, untracked). Each step is refused now (`CasePairBroughtIn`) before it writes anything;
//! a rebase or a sequence that already landed steps can still be aborted.
#![cfg(target_os = "linux")]

mod common;

use common::{commit_all, init_repo, write_file};
use git_core::hooks::HookOptions;

/// `(first, second)`: `first` is committed where the step lands, `second` comes with the step.
const CASE_PAIRS: [(&str, &str); 5] = [
    ("A", "a"),
    ("a", "A"),
    ("x/B", "x/b"),
    ("x/b", "x/B"),
    ("D/f", "d/f"),
];

fn head_id(repo: &git2::Repository) -> String {
    repo.head().unwrap().target().unwrap().to_string()
}

fn index_paths(dir: &std::path::Path) -> Vec<String> {
    let index = git2::Index::open(&dir.join(".git/index")).unwrap();
    let mut paths: Vec<String> = index
        .iter()
        .map(|entry| String::from_utf8_lossy(&entry.path).into_owned())
        .collect();
    paths.sort();
    paths
}

fn tree_paths(repo: &git2::Repository, rev: &str) -> Vec<String> {
    let tree = repo.revparse_single(rev).unwrap().peel_to_tree().unwrap();
    let mut flat = git2::Index::new().unwrap();
    flat.read_tree(&tree).unwrap();
    let mut paths: Vec<String> = flat
        .iter()
        .map(|entry| String::from_utf8_lossy(&entry.path).into_owned())
        .collect();
    paths.sort();
    paths
}

fn read(dir: &std::path::Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(rel)).ok()
}

fn sorted(names: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    names.sort();
    names
}

fn sorted_pair(pair: (&str, &str)) -> (String, String) {
    let names = sorted(&[pair.0, pair.1]);
    (names[0].clone(), names[1].clone())
}

/// Turns `core.ignorecase` on and reopens the repository (an open repository keeps its index's
/// case mode). Everything before is committed with it off: libgit2's ignore-case index can't
/// stage a pair.
fn ignore_case(dir: &std::path::Path, repo: git2::Repository) -> git2::Repository {
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    drop(repo);
    git2::Repository::open(dir).unwrap()
}

fn commit_id(repo: &git2::Repository, rev: &str) -> String {
    repo.revparse_single(rev)
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string()
}

/// Switches to `branch` with a forced checkout that also removes what the old tip had.
fn switch(repo: &git2::Repository, branch: &str) {
    repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force().remove_untracked(true);
    repo.checkout_head(Some(&mut checkout)).unwrap();
}

/// `main` holds `pair.0`, `conf` and `other`, plus a commit editing `conf`. `side`, forked
/// before that, adds `pair.1` (and edits `other` when `edit_other`). Checked out: `main`.
fn side_adds_the_other_name(
    pair: (&str, &str),
    edit_other: bool,
) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), pair.0, "first\n");
    write_file(dir.path(), "conf", "base\n");
    write_file(dir.path(), "other", "base\n");
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("side", &base, false).unwrap();
    }
    write_file(dir.path(), "conf", "main\n");
    commit_all(&repo, "main edits conf");
    switch(&repo, "side");
    write_file(dir.path(), pair.1, "second\n");
    if edit_other {
        write_file(dir.path(), "other", "side\n");
    }
    commit_all(&repo, "side adds the other name");
    switch(&repo, &main);
    let repo = ignore_case(dir.path(), repo);
    assert_eq!(read(dir.path(), pair.1), None);
    (dir, repo)
}

/// Nothing changed on `main`: HEAD, a clean state, the index and the files as committed.
fn assert_untouched(dir: &std::path::Path, repo: &git2::Repository, pair: (&str, &str), tip: &str) {
    assert_eq!(head_id(repo), tip, "{pair:?}");
    assert!(!repo.head_detached().unwrap(), "{pair:?}");
    assert_eq!(repo.state(), git2::RepositoryState::Clean, "{pair:?}");
    assert_eq!(
        index_paths(dir),
        sorted(&[pair.0, "conf", "other"]),
        "{pair:?}"
    );
    assert_eq!(read(dir, pair.0).as_deref(), Some("first\n"), "{pair:?}");
    assert_eq!(read(dir, pair.1), None, "{pair:?}");
    assert_eq!(read(dir, "conf").as_deref(), Some("main\n"), "{pair:?}");
    assert_eq!(read(dir, "other").as_deref(), Some("base\n"), "{pair:?}");
}

fn assert_brought_in(error: &dyn std::fmt::Display, pair: (&str, &str)) {
    let (path, other) = sorted_pair(pair);
    assert_eq!(
        error.to_string(),
        format!(
            "'{path}' and '{other}' differ only in case, which this repository's core.ignorecase setting takes for one name, so the result would keep only one of them. Rename one of the two where it was added, then try again."
        ),
        "{pair:?}"
    );
}

#[test]
fn cherry_pick_refuses_a_commit_that_brings_in_a_case_pair() {
    // The pick committed `a` without `A` (or `A` was not recorded), its file left untracked.
    for pair in CASE_PAIRS {
        let (dir, repo) = side_adds_the_other_name(pair, false);
        let tip = head_id(&repo);
        let side = commit_id(&repo, "side");

        let error =
            git_core::pick::cherry_pick(&repo, &side, None, HookOptions::default()).unwrap_err();

        assert!(
            matches!(error, git_core::pick::PickError::CasePairBroughtIn(_)),
            "{pair:?}: {error:?}"
        );
        assert_brought_in(&error, pair);
        assert_untouched(dir.path(), &repo, pair, &tip);
    }
}

#[test]
fn cherry_pick_keeps_a_case_only_rename_where_names_fold() {
    // A rename `A` -> `a` holds no pair: it goes ahead, and the commit has the new name only.
    let (dir, repo) = init_repo();
    write_file(dir.path(), "A", "first\n");
    write_file(dir.path(), "conf", "base\n");
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("side", &base, false).unwrap();
    }
    write_file(dir.path(), "conf", "main\n");
    commit_all(&repo, "main edits conf");
    switch(&repo, "side");
    std::fs::rename(dir.path().join("A"), dir.path().join("a")).unwrap();
    {
        let mut index = repo.index().unwrap();
        index.remove_path(std::path::Path::new("A")).unwrap();
        index.write().unwrap();
    }
    commit_all(&repo, "rename A to a");
    assert_eq!(tree_paths(&repo, "side"), sorted(&["a", "conf"]));
    switch(&repo, &main);
    let repo = ignore_case(dir.path(), repo);
    let side = commit_id(&repo, "side");

    let outcome = git_core::pick::cherry_pick(&repo, &side, None, HookOptions::default()).unwrap();

    assert!(
        matches!(outcome, git_core::pick::PickOutcome::Committed { .. }),
        "{outcome:?}"
    );
    assert_eq!(tree_paths(&repo, "HEAD"), sorted(&["a", "conf"]));
    assert_eq!(read(dir.path(), "a").as_deref(), Some("first\n"));
}

#[test]
fn revert_refuses_a_commit_whose_revert_brings_in_a_case_pair() {
    // `old` deletes `pair.1`, a later commit adds `pair.0`: reverting `old` brings `pair.1`
    // back next to it.
    for pair in CASE_PAIRS {
        let (dir, repo) = init_repo();
        write_file(dir.path(), pair.1, "second\n");
        write_file(dir.path(), "conf", "main\n");
        write_file(dir.path(), "other", "base\n");
        commit_all(&repo, "base");
        std::fs::remove_file(dir.path().join(pair.1)).unwrap();
        {
            let mut index = repo.index().unwrap();
            index.remove_path(std::path::Path::new(pair.1)).unwrap();
            index.write().unwrap();
        }
        commit_all(&repo, "old deletes the other name");
        let old = head_id(&repo);
        write_file(dir.path(), pair.0, "first\n");
        commit_all(&repo, "adds the first name");
        let repo = ignore_case(dir.path(), repo);
        let tip = head_id(&repo);

        let error = git_core::pick::revert(&repo, &old, None, HookOptions::default()).unwrap_err();

        assert!(
            matches!(error, git_core::pick::PickError::CasePairBroughtIn(_)),
            "{pair:?}: {error:?}"
        );
        assert_untouched(dir.path(), &repo, pair, &tip);
    }
}

#[test]
fn a_sequence_stops_at_a_step_that_brings_in_a_case_pair_and_aborts() {
    // The first step lands, the second would bring in the pair: the sequence stops there with
    // the refusal, and Abort rewinds to where it started.
    for pair in CASE_PAIRS {
        let (dir, repo) = init_repo();
        write_file(dir.path(), pair.0, "first\n");
        write_file(dir.path(), "conf", "base\n");
        write_file(dir.path(), "other", "base\n");
        commit_all(&repo, "base");
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        {
            let base = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch("side", &base, false).unwrap();
        }
        write_file(dir.path(), "conf", "main\n");
        commit_all(&repo, "main edits conf");
        switch(&repo, "side");
        write_file(dir.path(), "other", "side\n");
        commit_all(&repo, "s1 edits other");
        let s1 = head_id(&repo);
        write_file(dir.path(), pair.1, "second\n");
        commit_all(&repo, "s2 adds the other name");
        let s2 = head_id(&repo);
        switch(&repo, &main);
        let repo = ignore_case(dir.path(), repo);
        let tip = head_id(&repo);

        let run = git_core::pick::cherry_pick_many(&repo, &[s1, s2], HookOptions::default());

        let error = run.outcome.unwrap_err();
        assert!(
            matches!(error, git_core::pick::PickError::CasePairBroughtIn(_)),
            "{pair:?}: {error:?}"
        );
        let state = run.state.expect("the sequence stopped after a landed step");
        assert_eq!(state.next, 1, "{pair:?}");
        assert_ne!(head_id(&repo), tip, "{pair:?}");
        assert_eq!(repo.state(), git2::RepositoryState::Clean, "{pair:?}");
        assert_eq!(
            tree_paths(&repo, "HEAD"),
            sorted(&[pair.0, "conf", "other"]),
            "{pair:?}"
        );
        assert_eq!(read(dir.path(), pair.1), None, "{pair:?}");

        let abort = git_core::pick::abort_sequence(&repo, &state).unwrap();

        assert!(abort.rewound, "{pair:?}");
        assert_untouched(dir.path(), &repo, pair, &tip);
    }
}

/// `base` holds `pair.0`; `onto` adds `conf`. The branch `topic`, forked at `base`, edits
/// `other` (`c1`), adds `pair.1` (`c2`) and deletes it again (`c3`): neither tip holds the
/// pair, so the rebase starts, and picking `c2` would bring it in. Returns `(onto, tip)`.
fn rebase_brings_in_a_case_pair(
    pair: (&str, &str),
) -> (tempfile::TempDir, git2::Repository, String, String) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), pair.0, "first\n");
    write_file(dir.path(), "other", "x\n");
    commit_all(&repo, "base");
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("topic", &base, false).unwrap();
    }
    write_file(dir.path(), "conf", "upstream\n");
    commit_all(&repo, "upstream");
    let onto = head_id(&repo);
    switch(&repo, "topic");
    write_file(dir.path(), "other", "c1\n");
    commit_all(&repo, "c1");
    write_file(dir.path(), pair.1, "second\n");
    commit_all(&repo, "c2 adds the other name");
    std::fs::remove_file(dir.path().join(pair.1)).unwrap();
    {
        let mut index = repo.index().unwrap();
        index.remove_path(std::path::Path::new(pair.1)).unwrap();
        index.write().unwrap();
    }
    commit_all(&repo, "c3 deletes it again");
    let tip = head_id(&repo);
    let repo = ignore_case(dir.path(), repo);
    (dir, repo, onto, tip)
}

fn plan(
    repo: &git2::Repository,
    onto: &str,
    first: git_core::rebase::RebaseAction,
) -> Vec<git_core::rebase::RebasePlanEntry> {
    use git_core::rebase::{RebaseAction, RebasePlanEntry};
    git_core::rebase::commits_since(repo, onto)
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(at, commit)| RebasePlanEntry {
            commit_id: commit.id,
            action: if at == 0 {
                first.clone()
            } else {
                RebaseAction::Pick
            },
            combined_message: None,
        })
        .collect()
}

/// The branch as the rebase found it: `topic` at `tip`, `pair.0` and `other` (`c1`).
fn assert_on_topic(dir: &std::path::Path, repo: &git2::Repository, pair: (&str, &str), tip: &str) {
    assert!(!repo.head_detached().unwrap(), "{pair:?}");
    assert_eq!(
        repo.head().unwrap().shorthand().ok(),
        Some("topic"),
        "{pair:?}"
    );
    assert_eq!(head_id(repo), tip, "{pair:?}");
    assert_eq!(repo.state(), git2::RepositoryState::Clean, "{pair:?}");
    assert_eq!(index_paths(dir), sorted(&[pair.0, "other"]), "{pair:?}");
    assert_eq!(read(dir, pair.0).as_deref(), Some("first\n"), "{pair:?}");
    assert_eq!(read(dir, pair.1), None, "{pair:?}");
    assert_eq!(read(dir, "other").as_deref(), Some("c1\n"), "{pair:?}");
    assert_eq!(read(dir, "conf"), None, "{pair:?}");
}

#[test]
fn rebase_refuses_a_pick_that_brings_in_a_case_pair() {
    // The rebased `c2` had `a` and no `A` (or `A` wasn't recorded), so the rebased branch lost
    // one name for good. `start_rebase` now stops at `c2` and rolls back to the branch.
    use git_core::rebase::{RebaseAction, RebaseError};
    for pair in CASE_PAIRS {
        let (dir, repo, onto, tip) = rebase_brings_in_a_case_pair(pair);
        let plan = plan(&repo, &onto, RebaseAction::Pick);
        assert_eq!(plan.len(), 3, "{pair:?}");

        let error = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap_err();

        assert!(
            matches!(error, RebaseError::CasePairBroughtIn(_)),
            "{pair:?}: {error:?}"
        );
        assert_brought_in(&error, pair);
        assert_on_topic(dir.path(), &repo, pair, &tip);
    }
}

#[test]
fn rebase_continue_stops_at_a_pick_that_brings_in_a_case_pair_and_aborts() {
    // Paused at `c1` (Edit), Continue lands it and stops at `c2` without applying it: HEAD
    // stays at the rebased `c1`, nothing of `c2` is written, Continue would refuse again and
    // Abort goes back to the branch.
    use git_core::rebase::{RebaseAction, RebaseError, RebaseStepResult, RebaseStop};
    for pair in CASE_PAIRS {
        let (dir, repo, onto, tip) = rebase_brings_in_a_case_pair(pair);
        let plan = plan(&repo, &onto, RebaseAction::Edit);
        let (mut state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
        assert_eq!(result, RebaseStepResult::PausedForEdit, "{pair:?}");

        let error = git_core::rebase::rebase_continue(&repo, &mut state).unwrap_err();

        assert!(
            matches!(error, RebaseError::CasePairBroughtIn(_)),
            "{pair:?}: {error:?}"
        );
        assert_eq!(state.stop, RebaseStop::Error, "{pair:?}");
        assert_eq!(state.cursor, 1, "{pair:?}");
        assert!(repo.head_detached().unwrap(), "{pair:?}");
        assert_eq!(head_id(&repo), state.expected_head.to_string(), "{pair:?}");
        assert_eq!(repo.state(), git2::RepositoryState::Clean, "{pair:?}");
        assert_eq!(
            index_paths(dir.path()),
            sorted(&[pair.0, "conf", "other"]),
            "{pair:?}"
        );
        assert_eq!(read(dir.path(), pair.1), None, "{pair:?}");
        let again = git_core::rebase::rebase_continue(&repo, &mut state).unwrap_err();
        assert!(
            matches!(again, RebaseError::CasePairBroughtIn(_)),
            "{pair:?}: {again:?}"
        );

        let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

        assert!(abort.restored, "{pair:?}");
        assert_on_topic(dir.path(), &repo, pair, &tip);
    }
}

#[test]
fn merge_refuses_a_branch_that_brings_in_a_case_pair() {
    // A merge commit (and a squash) recorded only one name; a fast-forward kept one in the
    // index and didn't write the other's file.
    use git_core::merge::{FastForward, MergeError, MergeOptions};
    let options = [
        MergeOptions::default(),
        MergeOptions {
            fast_forward: FastForward::Never,
            squash: false,
        },
        MergeOptions {
            fast_forward: FastForward::Allow,
            squash: true,
        },
    ];
    for pair in CASE_PAIRS {
        for opts in options {
            let (dir, repo) = side_adds_the_other_name(pair, true);
            let tip = head_id(&repo);

            let error = git_core::merge::start_merge(&repo, "side", opts).unwrap_err();

            assert!(
                matches!(error, MergeError::CasePairBroughtIn(_)),
                "{pair:?} {opts:?}: {error:?}"
            );
            assert_brought_in(&error, pair);
            assert_untouched(dir.path(), &repo, pair, &tip);
            assert!(!dir.path().join(".git/SQUASH_MSG").exists());
        }
    }
}

/// `main` holds `pair.0`, `conf` and `other`; `ahead`, one commit further, adds `pair.1`.
/// Checked out: `main`.
fn ahead_adds_the_other_name(pair: (&str, &str)) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), pair.0, "first\n");
    write_file(dir.path(), "conf", "main\n");
    write_file(dir.path(), "other", "base\n");
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("ahead", &base, false).unwrap();
    }
    switch(&repo, "ahead");
    write_file(dir.path(), pair.1, "second\n");
    commit_all(&repo, "adds the other name");
    switch(&repo, &main);
    let repo = ignore_case(dir.path(), repo);
    (dir, repo)
}

#[test]
fn fast_forward_merge_refuses_a_branch_that_brings_in_a_case_pair() {
    use git_core::merge::{FastForward, MergeError, MergeOptions};
    for pair in CASE_PAIRS {
        for fast_forward in [FastForward::Allow, FastForward::Only] {
            let (dir, repo) = ahead_adds_the_other_name(pair);
            let tip = head_id(&repo);
            let opts = MergeOptions {
                fast_forward,
                squash: false,
            };

            let error = git_core::merge::start_merge(&repo, "ahead", opts).unwrap_err();

            assert!(
                matches!(error, MergeError::CasePairBroughtIn(_)),
                "{pair:?}: {error:?}"
            );
            assert_untouched(dir.path(), &repo, pair, &tip);
        }
    }
}

#[test]
fn fast_forward_pull_refuses_an_upstream_that_brings_in_a_case_pair() {
    for pair in CASE_PAIRS {
        let (dir, repo) = ahead_adds_the_other_name(pair);
        let tip = head_id(&repo);
        let ahead = repo.revparse_single("ahead").unwrap().id();
        repo.reference("refs/remotes/origin/main", ahead, true, "test")
            .unwrap();

        let error = git_core::remote::pull_after_fetch(&repo, "origin", "main").unwrap_err();

        assert!(
            matches!(error, git_core::remote::RemoteError::CasePairBroughtIn(_)),
            "{pair:?}: {error:?}"
        );
        assert_untouched(dir.path(), &repo, pair, &tip);
    }
}

#[test]
fn stash_apply_refuses_a_stash_that_brings_in_a_case_pair() {
    // The stash staged `pair.1` next to `pair.0`; applying it (with or without its index)
    // kept only one of them in the index.
    use git_core::stash::{apply_stash, StashError};
    for pair in CASE_PAIRS {
        for restore_index in [false, true] {
            let (dir, mut repo) = init_repo();
            write_file(dir.path(), pair.0, "first\n");
            write_file(dir.path(), "conf", "main\n");
            write_file(dir.path(), "other", "base\n");
            commit_all(&repo, "base");
            write_file(dir.path(), pair.1, "second\n");
            {
                let mut index = repo.index().unwrap();
                index.add_path(std::path::Path::new(pair.1)).unwrap();
                index.write().unwrap();
            }
            let signature = repo.signature().unwrap();
            repo.stash_save(&signature, "adds the other name", None)
                .unwrap();
            assert_eq!(read(dir.path(), pair.1), None);
            let mut repo = ignore_case(dir.path(), repo);
            let tip = head_id(&repo);
            let stash = repo.revparse_single("refs/stash").unwrap().id();

            let error = apply_stash(&mut repo, 0, None, restore_index).unwrap_err();

            assert!(
                matches!(error, StashError::CasePairBroughtIn(_)),
                "{pair:?} {restore_index}: {error:?}"
            );
            assert_untouched(dir.path(), &repo, pair, &tip);
            assert_eq!(repo.revparse_single("refs/stash").unwrap().id(), stash);
        }
    }
}

/// `main` holds `pair.0`, `conf` and `notes`. `side`, forked before `main`'s last commit, adds
/// `pair.1` and edits `notes` (`s1`), then edits `conf` (`s2`, which conflicts). Then, with `core.ignorecase` on, what a
/// pick outside Browsitory (or one before #522) did: `s1` landed through libgit2's ignore-case
/// index, which committed one name and wrote the other's file, and `s2` paused on its conflict.
/// Returns the sequence's state and `main`'s tip.
fn sequence_after_a_split_step(
    pair: (&str, &str),
) -> (
    tempfile::TempDir,
    git2::Repository,
    git_core::pick::SequenceState,
    String,
) {
    use git_core::pick::{OriginalRef, SequenceKind, SequenceState};
    let (dir, repo) = init_repo();
    write_file(dir.path(), pair.0, "first\n");
    write_file(dir.path(), "conf", "base\n");
    write_file(dir.path(), "notes", "base\n");
    commit_all(&repo, "base");
    let main_ref = repo.head().unwrap().name().unwrap().to_string();
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("side", &base, false).unwrap();
    }
    write_file(dir.path(), "conf", "main\n");
    commit_all(&repo, "main");
    let main = head_id(&repo);
    switch(&repo, "side");
    write_file(dir.path(), pair.1, "second\n");
    write_file(dir.path(), "notes", "s1\n");
    commit_all(&repo, "s1 adds the other name");
    let s1 = head_id(&repo);
    write_file(dir.path(), "conf", "side\n");
    commit_all(&repo, "s2 conflicts");
    let s2 = head_id(&repo);
    repo.set_head(&main_ref).unwrap();
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout)).unwrap();
    let repo = ignore_case(dir.path(), repo);

    repo.cherrypick(&repo.find_commit(s1.parse().unwrap()).unwrap(), None)
        .unwrap();
    let landed = {
        let mut index = repo.index().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = repo.signature().unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "s1", &tree, &[&head])
            .unwrap()
    };
    repo.cleanup_state().unwrap();
    // The landed step lost one name: what #522 refuses now.
    assert_eq!(tree_paths(&repo, "HEAD").len(), 3, "{pair:?}");
    repo.cherrypick(&repo.find_commit(s2.parse().unwrap()).unwrap(), None)
        .unwrap();
    assert!(repo.index().unwrap().has_conflicts(), "{pair:?}");
    let state = SequenceState {
        kind: SequenceKind::CherryPick,
        commits: vec![s1, s2],
        next: 1,
        skipped: Vec::new(),
        original_head: main.clone(),
        expected_head: landed.to_string(),
        stopped_on_conflict: true,
        original_ref: OriginalRef::Branch(main_ref),
    };
    (dir, repo, state, main)
}

#[test]
fn sequence_abort_rewinds_after_a_step_brought_in_a_case_pair() {
    // The issue's comment on #522: Abort's rewind is a safe checkout of the original commit,
    // which HEAD's other name of the pair kept from working ("1 conflict prevents checkout",
    // for `A` then `a`). The pair's names are now put back directly; the picked name's file,
    // which the step wrote, is removed. The step's other file is rewound too, by the safe
    // checkout of every path outside the pair.
    for pair in CASE_PAIRS {
        let (dir, repo, state, main) = sequence_after_a_split_step(pair);

        let abort = git_core::pick::abort_sequence(&repo, &state).unwrap();

        assert!(abort.rewound, "{pair:?}");
        assert_eq!(head_id(&repo), main, "{pair:?}");
        assert!(!repo.head_detached().unwrap(), "{pair:?}");
        assert_eq!(repo.state(), git2::RepositoryState::Clean, "{pair:?}");
        assert_eq!(
            index_paths(dir.path()),
            sorted(&[pair.0, "conf", "notes"]),
            "{pair:?}"
        );
        assert_eq!(
            read(dir.path(), pair.0).as_deref(),
            Some("first\n"),
            "{pair:?}"
        );
        assert_eq!(read(dir.path(), pair.1), None, "{pair:?}");
        // Written back as the original commit has it: not executable.
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(dir.path().join(pair.0))
                .unwrap()
                .permissions(),
        );
        assert_eq!(mode & 0o111, 0, "{pair:?}");
        assert_eq!(
            read(dir.path(), "notes").as_deref(),
            Some("base\n"),
            "{pair:?}"
        );
        assert_eq!(
            read(dir.path(), "conf").as_deref(),
            Some("main\n"),
            "{pair:?}"
        );
    }
}

#[test]
fn sequence_abort_keeps_a_local_change_to_one_of_a_case_pair() {
    // Like the safe checkout it stands in for, the rewind never overwrites a local change: a
    // pair name whose file holds neither HEAD's nor the original commit's content is
    // `RewindBlocked`, and Abort can be retried once it is saved.
    for pair in CASE_PAIRS {
        let (dir, repo, state, main) = sequence_after_a_split_step(pair);
        let held = tree_paths(&repo, "HEAD")
            .into_iter()
            .find(|path| path != "conf" && path != "notes")
            .unwrap();
        write_file(dir.path(), &held, "mine\n");

        let error = git_core::pick::abort_sequence(&repo, &state).unwrap_err();

        assert!(
            matches!(&error, git_core::pick::PickError::RewindBlocked(path) if *path == held),
            "{pair:?}: {error:?}"
        );
        assert_ne!(head_id(&repo), main, "{pair:?}");
        assert_eq!(
            read(dir.path(), &held).as_deref(),
            Some("mine\n"),
            "{pair:?}"
        );
    }
}

// #532: a switch or a checkout whose tree holds a case pair HEAD's tree doesn't. libgit2's safe
// checkout kept one name in the index and didn't write the other's file, so the next commit
// dropped it.

#[test]
fn switch_branch_refuses_a_branch_that_brings_in_a_case_pair() {
    for pair in CASE_PAIRS {
        let (dir, repo) = ahead_adds_the_other_name(pair);
        let tip = head_id(&repo);

        let error = git_core::branch::switch_branch(&repo, "ahead").unwrap_err();

        assert!(
            matches!(error, git_core::branch::BranchError::CasePairBroughtIn(_)),
            "{pair:?}: {error:?}"
        );
        assert_brought_in(&error, pair);
        assert_untouched(dir.path(), &repo, pair, &tip);
    }
}

#[test]
fn checkout_commit_refuses_a_commit_that_brings_in_a_case_pair() {
    for pair in CASE_PAIRS {
        let (dir, repo) = ahead_adds_the_other_name(pair);
        let tip = head_id(&repo);
        let ahead = commit_id(&repo, "ahead");

        let error = git_core::branch::checkout_commit(&repo, &ahead).unwrap_err();

        assert!(
            matches!(error, git_core::branch::BranchError::CasePairBroughtIn(_)),
            "{pair:?}: {error:?}"
        );
        assert_untouched(dir.path(), &repo, pair, &tip);
    }
}

#[test]
fn checkout_commit_refuses_a_tag_or_remote_branch_that_brings_in_a_case_pair() {
    // A tag or a remote branch is checked out by its commit (detached).
    for pair in CASE_PAIRS {
        let (dir, repo) = ahead_adds_the_other_name(pair);
        let tip = head_id(&repo);
        let ahead = repo.revparse_single("ahead").unwrap().id();
        repo.reference("refs/remotes/origin/ahead", ahead, true, "test")
            .unwrap();
        let target = commit_id(&repo, "origin/ahead");

        let error = git_core::branch::checkout_commit(&repo, &target).unwrap_err();

        assert!(
            matches!(error, git_core::branch::BranchError::CasePairBroughtIn(_)),
            "{pair:?}: {error:?}"
        );
        assert_untouched(dir.path(), &repo, pair, &tip);
    }
}

#[test]
fn create_branch_refuses_a_start_point_that_brings_in_a_case_pair_and_creates_nothing() {
    for pair in CASE_PAIRS {
        let (dir, repo) = ahead_adds_the_other_name(pair);
        let tip = head_id(&repo);

        let error = git_core::branch::create_branch(&repo, "topic", "ahead").unwrap_err();

        assert!(
            matches!(error, git_core::branch::BranchError::CasePairBroughtIn(_)),
            "{pair:?}: {error:?}"
        );
        assert!(
            repo.find_branch("topic", git2::BranchType::Local).is_err(),
            "{pair:?}"
        );
        assert_untouched(dir.path(), &repo, pair, &tip);
    }
}

#[test]
fn switch_branch_keeps_a_case_only_rename_where_names_fold() {
    // A rename `A` -> `a` holds no pair: the switch goes ahead.
    let (dir, repo) = init_repo();
    write_file(dir.path(), "A", "first\n");
    write_file(dir.path(), "conf", "base\n");
    commit_all(&repo, "base");
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("side", &base, false).unwrap();
    }
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    switch(&repo, "side");
    std::fs::rename(dir.path().join("A"), dir.path().join("a")).unwrap();
    {
        let mut index = repo.index().unwrap();
        index.remove_path(std::path::Path::new("A")).unwrap();
        index.write().unwrap();
    }
    commit_all(&repo, "rename A to a");
    switch(&repo, &main);
    let repo = ignore_case(dir.path(), repo);

    git_core::branch::switch_branch(&repo, "side").unwrap();

    assert_eq!(head_id(&repo), commit_id(&repo, "side"));
    assert_eq!(index_paths(dir.path()), sorted(&["a", "conf"]));
    assert_eq!(read(dir.path(), "a").as_deref(), Some("first\n"));
}

#[test]
fn switch_branch_goes_ahead_when_head_already_holds_the_pair() {
    // A pair HEAD already holds is #512's, not brought in: the check lets the switch through.
    for pair in CASE_PAIRS {
        let (dir, repo) = init_repo();
        write_file(dir.path(), pair.0, "first\n");
        write_file(dir.path(), pair.1, "second\n");
        write_file(dir.path(), "conf", "base\n");
        commit_all(&repo, "base");
        {
            let base = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch("same", &base, false).unwrap();
        }
        let repo = ignore_case(dir.path(), repo);

        git_core::branch::switch_branch(&repo, "same").unwrap();

        assert_eq!(
            repo.head().unwrap().shorthand().unwrap(),
            "same",
            "{pair:?}"
        );
    }
}

#[test]
fn create_worktree_refuses_a_branch_that_holds_a_case_pair() {
    for pair in CASE_PAIRS {
        let (dir, repo) = ahead_adds_the_other_name(pair);
        let tip = head_id(&repo);
        let parent = tempfile::tempdir().unwrap();
        let path = parent.path().join("wt");

        let error = git_core::worktree::create_worktree_with(
            &repo,
            "wt",
            &path,
            "topic",
            Some("ahead"),
            |_| Ok(false),
        )
        .unwrap_err();

        assert!(
            matches!(
                error,
                git_core::worktree::WorktreeError::CasePairBroughtIn(_)
            ),
            "{pair:?}: {error:?}"
        );
        assert!(!path.exists(), "{pair:?}");
        assert!(
            repo.find_branch("topic", git2::BranchType::Local).is_err(),
            "{pair:?}"
        );
        assert_untouched(dir.path(), &repo, pair, &tip);
    }
}

#[test]
fn a_sequence_abort_without_a_case_pair_rewinds_by_checkout() {
    // The refused step wrote nothing, so no pair is left: the rewind is libgit2's safe checkout,
    // which runs the file's filters and keeps a change staged on a file the rewind doesn't
    // change. The direct writes that stand in for it around a pair are only for the pair.
    let pair = ("A", "a");
    let (dir, repo) = init_repo();
    write_file(dir.path(), ".gitattributes", "other text eol=crlf\n");
    write_file(dir.path(), pair.0, "first\n");
    write_file(dir.path(), "conf", "base\n");
    write_file(dir.path(), "other", "base\n");
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("side", &base, false).unwrap();
    }
    switch(&repo, "side");
    write_file(dir.path(), "other", "side\n");
    commit_all(&repo, "s1 edits other");
    let s1 = head_id(&repo);
    write_file(dir.path(), pair.1, "second\n");
    commit_all(&repo, "s2 adds the other name");
    let s2 = head_id(&repo);
    switch(&repo, &main);
    let repo = ignore_case(dir.path(), repo);
    let tip = head_id(&repo);
    let run = git_core::pick::cherry_pick_many(&repo, &[s1, s2], HookOptions::default());
    assert!(run.outcome.is_err());
    let state = run.state.expect("the sequence stopped after a landed step");
    write_file(dir.path(), "conf", "staged\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("conf")).unwrap();
    index.write().unwrap();
    let staged = index.get_path(std::path::Path::new("conf"), 0).unwrap().id;

    let abort = git_core::pick::abort_sequence(&repo, &state).unwrap();

    assert!(abort.rewound);
    assert_eq!(head_id(&repo), tip);
    assert_eq!(read(dir.path(), "other").as_deref(), Some("base\r\n"));
    let index = git2::Index::open(&dir.path().join(".git/index")).unwrap();
    assert_eq!(
        index.get_path(std::path::Path::new("conf"), 0).unwrap().id,
        staged
    );
    assert_eq!(read(dir.path(), "conf").as_deref(), Some("staged\n"));
}
