//! #512: a case pair (`A` and `a`, committed on a case-sensitive drive) that HEAD already holds
//! while `core.ignorecase` is on. libgit2's merge checkout then refuses every merge, even one
//! that touches neither name, with a raw "1 conflict prevents checkout". The merge is refused up
//! front with `CasePair`, which says why. A fast-forward, a cherry-pick and a revert keep both
//! names (pinned below), so they go ahead.
#![cfg(target_os = "linux")]

mod common;

use common::{commit_all, init_repo, write_file};
use git_core::merge::{FastForward, MergeError, MergeOptions, MergeOutcome};

const CASE_PAIRS: [(&str, &str); 5] = [
    ("A", "a"),
    ("a", "A"),
    ("x/B", "x/b"),
    ("x/b", "x/B"),
    ("D/f", "d/f"),
];

fn sorted(names: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    names.sort();
    names
}

/// HEAD holds `pair` and `conf`; `side` adds `other.txt`, and with `diverged` the checked-out
/// branch adds `main.txt`. `core.ignorecase` is turned on afterwards, the repository reopened.
/// Returns `side`'s tip.
fn repo_holding(
    pair: (&str, &str),
    diverged: bool,
) -> (tempfile::TempDir, git2::Repository, git2::Oid) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), pair.0, "first\n");
    write_file(dir.path(), pair.1, "second\n");
    write_file(dir.path(), "conf", "base\n");
    commit_all(&repo, "pair");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
    write_file(dir.path(), "other.txt", "other\n");
    commit_all(&repo, "side");
    let side = common::head_id(&repo);
    git_core::branch::switch_branch(&repo, &main).unwrap();
    if diverged {
        write_file(dir.path(), "main.txt", "main\n");
        commit_all(&repo, "main");
    }
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    drop(repo);
    let repo = git2::Repository::open(dir.path()).unwrap();
    (dir, repo, side)
}

fn options(fast_forward: FastForward, squash: bool) -> MergeOptions {
    MergeOptions {
        fast_forward,
        squash,
    }
}

#[test]
fn a_merge_is_refused_with_the_pair_named() {
    for pair in CASE_PAIRS {
        for (diverged, opts) in [
            (true, options(FastForward::Allow, false)),
            (true, options(FastForward::Allow, true)),
            (false, options(FastForward::Never, false)),
        ] {
            let (dir, repo, _side) = repo_holding(pair, diverged);
            let head = common::head_id(&repo);

            let error = git_core::merge::start_merge(&repo, "side", opts).unwrap_err();

            let MergeError::CasePair(found) = &error else {
                panic!("{pair:?} {opts:?}: {error:?}");
            };
            let mut names = [found.path.as_str(), found.other.as_str()];
            names.sort();
            assert_eq!(sorted(&names), sorted(&[pair.0, pair.1]), "{pair:?}");
            assert!(error.to_string().contains("differ only in case"), "{error}");
            // Nothing was written.
            assert_eq!(repo.state(), git2::RepositoryState::Clean, "{pair:?}");
            assert_eq!(common::head_id(&repo), head, "{pair:?}");
            assert!(!dir.path().join("other.txt").exists(), "{pair:?}");
            assert!(!repo.path().join("SQUASH_MSG").exists(), "{pair:?}");
        }
    }
}

#[test]
fn a_fast_forward_keeps_both_names() {
    for pair in CASE_PAIRS {
        let (dir, repo, side) = repo_holding(pair, false);

        let outcome = git_core::merge::start_merge(&repo, "side", MergeOptions::default()).unwrap();

        assert_eq!(outcome, MergeOutcome::FastForwarded, "{pair:?}");
        assert_eq!(common::head_id(&repo), side);
        assert_eq!(
            common::index_paths(dir.path()),
            sorted(&[pair.0, pair.1, "conf", "other.txt"]),
            "{pair:?}"
        );
    }
}

#[test]
fn a_cherry_pick_and_a_revert_keep_both_names() {
    for pair in CASE_PAIRS {
        let (dir, repo, side) = repo_holding(pair, true);

        git_core::pick::cherry_pick(&repo, &side.to_string(), None, Default::default()).unwrap();
        assert_eq!(
            common::index_paths(dir.path()),
            sorted(&[pair.0, pair.1, "conf", "main.txt", "other.txt"]),
            "{pair:?}"
        );

        let picked = common::head_id(&repo).to_string();
        git_core::pick::revert(&repo, &picked, None, Default::default()).unwrap();
        assert_eq!(
            common::index_paths(dir.path()),
            sorted(&[pair.0, pair.1, "conf", "main.txt"]),
            "{pair:?}"
        );
        assert_eq!(repo.state(), git2::RepositoryState::Clean, "{pair:?}");
    }
}
