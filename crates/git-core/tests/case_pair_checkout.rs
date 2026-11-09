//! #403: operations that force a checkout, with a committed case pair (`A` and `a`, which only a
//! case-sensitive drive holds) and `core.ignorecase` on. libgit2 1.9's ignore-case index takes the
//! pair for one entry: a forced checkout that includes both keeps one of them in the index and,
//! depending on the order the directory lists them in, deletes the other's file (#392). Such
//! checkouts are refused (`CasePair`) before anything is written; checkouts limited to other
//! paths keep the pair and go ahead. Aborts never refuse: they write the pair's files directly (#511).
#![cfg(target_os = "linux")]

mod common;

use common::{commit_all, init_repo, write_file};
use git_core::reset::{reset_to, ResetError, ResetMode};

/// `(first, second)` in the order they are written, which decided which file libgit2 deleted.
const CASE_PAIRS: [(&str, &str); 4] = [("A", "a"), ("a", "A"), ("x/B", "x/b"), ("D/f", "d/f")];

/// HEAD's history: `base` (the pair and `other`), then `c1` and `c2` editing `other`. Committed
/// with `core.ignorecase` off (libgit2's ignore-case index can't stage the pair), then turned on
/// and the repository reopened (an open repository keeps its index's case mode). Returns
/// `base`'s id.
fn case_pair_history(pair: (&str, &str)) -> (tempfile::TempDir, git2::Repository, String) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), pair.0, "first\n");
    write_file(dir.path(), pair.1, "second\n");
    write_file(dir.path(), "other", "x\n");
    commit_all(&repo, "base");
    let base = head_id(&repo);
    for name in ["c1", "c2"] {
        write_file(dir.path(), "other", &format!("{name}\n"));
        commit_all(&repo, name);
    }
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    drop(repo);
    let repo = git2::Repository::open(dir.path()).unwrap();
    (dir, repo, base)
}

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

fn read(dir: &std::path::Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(rel)).ok()
}

/// Both of the pair and `other` in the index, and their files as committed (`other` as given).
fn assert_pair_kept(dir: &std::path::Path, pair: (&str, &str), other: &str) {
    let mut expected = vec![pair.0.to_string(), pair.1.to_string(), "other".to_string()];
    expected.sort();
    assert_eq!(index_paths(dir), expected, "{pair:?}");
    assert_eq!(read(dir, pair.0).as_deref(), Some("first\n"), "{pair:?}");
    assert_eq!(read(dir, pair.1).as_deref(), Some("second\n"), "{pair:?}");
    assert_eq!(read(dir, "other").as_deref(), Some(other), "{pair:?}");
}

fn sorted_pair(pair: (&str, &str)) -> (String, String) {
    let mut names = [pair.0, pair.1];
    names.sort();
    (names[0].to_string(), names[1].to_string())
}

#[test]
fn libgit2_forced_checkout_of_both_names_of_a_case_pair_keeps_one_under_ignorecase() {
    // What the refusals guard against, with the pair named explicitly: limiting the checkout to
    // the two paths doesn't help, only one stays in the index. Limited to other paths, both
    // stay. If a libgit2 update changes this, the refusals may be relaxed.
    for pair in CASE_PAIRS {
        let (dir, repo, _) = case_pair_history(pair);
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout
            .force()
            .disable_pathspec_match(true)
            .path(pair.0)
            .path(pair.1);
        repo.checkout_head(Some(&mut checkout)).unwrap();
        assert_eq!(index_paths(dir.path()).len(), 2, "{pair:?}");

        let (dir, repo, _) = case_pair_history(pair);
        write_file(dir.path(), "other", "edited\n");
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().disable_pathspec_match(true).path("other");
        repo.checkout_head(Some(&mut checkout)).unwrap();
        assert_pair_kept(dir.path(), pair, "c2\n");
    }
}

#[test]
fn hard_reset_refuses_a_case_pair_where_names_fold() {
    // #403: the forced checkout of the whole tree kept one of the pair in the index (the reset
    // then rewrote the index) and deleted the other's file (`d/f` on ext4 and tmpfs), to HEAD
    // as to an older commit. Refused now, nothing changed.
    for pair in CASE_PAIRS {
        for to_head in [true, false] {
            let (dir, repo, base) = case_pair_history(pair);
            write_file(dir.path(), "other", "edited\n");
            let tip = head_id(&repo);
            let index_before = std::fs::read(dir.path().join(".git/index")).unwrap();
            let target = if to_head { tip.clone() } else { base };

            let error = reset_to(&repo, &target, ResetMode::Hard).unwrap_err();

            let (path, other) = sorted_pair(pair);
            assert!(
                matches!(&error, ResetError::CasePair(found) if found.path == path && found.other == other),
                "{pair:?}: {error:?}"
            );
            assert_eq!(
                error.to_string(),
                format!(
                    "'{path}' and '{other}' differ only in case, which this repository's core.ignorecase setting takes for one name, so the checkout would remove one of them. Rename one of the two in the repository first."
                )
            );
            assert_eq!(head_id(&repo), tip);
            assert_eq!(
                std::fs::read(dir.path().join(".git/index")).unwrap(),
                index_before
            );
            assert_pair_kept(dir.path(), pair, "edited\n");
        }
    }
}

#[test]
fn mixed_and_soft_resets_keep_a_case_pair_where_names_fold() {
    // No checkout: the index is rewritten from the tree, which keeps both.
    for pair in CASE_PAIRS {
        for mode in [ResetMode::Mixed, ResetMode::Soft] {
            let (dir, repo, base) = case_pair_history(pair);

            reset_to(&repo, &base, mode).unwrap();

            assert_eq!(head_id(&repo), base);
            let mut expected = vec![pair.0.to_string(), pair.1.to_string(), "other".to_string()];
            expected.sort();
            assert_eq!(index_paths(dir.path()), expected, "{pair:?} {mode:?}");
            assert_eq!(read(dir.path(), pair.0).as_deref(), Some("first\n"));
            assert_eq!(read(dir.path(), pair.1).as_deref(), Some("second\n"));
        }
    }
}

#[test]
fn hard_reset_keeps_a_case_pair_where_names_do_not_fold() {
    for pair in CASE_PAIRS {
        let (dir, repo, base) = case_pair_history(pair);
        repo.config()
            .unwrap()
            .set_bool("core.ignorecase", false)
            .unwrap();
        drop(repo);
        let repo = git2::Repository::open(dir.path()).unwrap();

        reset_to(&repo, &base, ResetMode::Hard).unwrap();

        assert_pair_kept(dir.path(), pair, "x\n");
    }
}

#[test]
fn discard_refuses_one_of_a_case_pair_where_names_fold() {
    // #403: discarding `x/B` (or `x/b`) renamed the other's index entry to the same name, so the
    // index held `x/b` twice. Refused for every pair; nothing changed.
    for pair in CASE_PAIRS {
        for picked in [pair.0, pair.1] {
            let (dir, repo, _) = case_pair_history(pair);
            write_file(dir.path(), picked, "edited\n");
            let index_before = std::fs::read(dir.path().join(".git/index")).unwrap();

            let error = git_core::stage::discard_file(&repo, picked).unwrap_err();

            assert!(
                matches!(error, git_core::stage::StageError::CasePair(_)),
                "{picked:?}: {error:?}"
            );
            assert_eq!(
                std::fs::read(dir.path().join(".git/index")).unwrap(),
                index_before
            );
            assert_eq!(read(dir.path(), picked).as_deref(), Some("edited\n"));
        }
    }
}

#[test]
fn discard_of_another_file_keeps_a_case_pair_where_names_fold() {
    for pair in CASE_PAIRS {
        let (dir, repo, _) = case_pair_history(pair);
        write_file(dir.path(), "other", "edited\n");

        git_core::stage::discard_file(&repo, "other").unwrap();

        assert_pair_kept(dir.path(), pair, "c2\n");
    }
}

#[test]
fn rebase_refuses_a_case_pair_where_names_fold() {
    // #403: a rebase onto `base` started (for `A`/`a` it was refused as having local changes
    // in `A` and `a`), and its Abort's forced checkout then dropped `d/f` from the index and
    // deleted its file. Refused before anything is written, so there is no rebase to abort.
    use git_core::rebase::{RebaseAction, RebaseError, RebasePlanEntry};
    for pair in CASE_PAIRS {
        let (dir, repo, base) = case_pair_history(pair);
        let tip = head_id(&repo);
        let plan: Vec<RebasePlanEntry> = git_core::rebase::commits_since(&repo, &base)
            .unwrap()
            .into_iter()
            .map(|commit| RebasePlanEntry {
                commit_id: commit.id,
                action: RebaseAction::Edit,
                combined_message: None,
            })
            .collect();

        let error = git_core::rebase::start_rebase(&repo, &base, plan).unwrap_err();

        assert!(
            matches!(error, RebaseError::CasePair(_)),
            "{pair:?}: {error:?}"
        );
        assert_eq!(repo.state(), git2::RepositoryState::Clean);
        assert!(!repo.head_detached().unwrap());
        assert_eq!(head_id(&repo), tip);
        assert_pair_kept(dir.path(), pair, "c2\n");
    }
}

#[test]
fn undo_restores_one_of_a_case_pair_and_keeps_the_other() {
    // #403: a restored file equal to its index entry went through a forced checkout from the
    // index, which for `A` (found as itself in the ignore-case index) would now be refused, and
    // for `a` (found as `A`) was written raw. Pair members are written raw now, both ways.
    use git_core::undo::{capture, restore, SnapshotScope};
    for pair in CASE_PAIRS {
        for picked in [pair.0, pair.1] {
            let (dir, repo, _) = case_pair_history(pair);
            let clean = capture(&repo, SnapshotScope::Full).unwrap();
            write_file(dir.path(), picked, "edited\n");
            write_file(dir.path(), "new", "new\n");
            let edited = capture(&repo, SnapshotScope::Full).unwrap();

            restore(&repo, &edited, &clean, "undo").unwrap();

            assert_pair_kept(dir.path(), pair, "c2\n");
            assert_eq!(read(dir.path(), "new"), None, "{picked:?}");
        }
    }
}

/// `base` holds `pair.0`; `onto` adds `conf` on top of it. The branch, forked at `base`, edits
/// `other` (`c1`), adds `pair.1` (`c2`), adds `conf` (`c3`, which conflicts with `onto`'s) and
/// deletes `pair.1` again (`c4`). Neither tip holds the pair, so a rebase starts, and picking
/// `c2` brings the pair in (refused since #522). Committed with `core.ignorecase` off, then
/// turned on and the repository reopened. Returns `(onto, tip)`.
fn pair_brought_in_by_a_rebase(
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

    repo.set_head("refs/heads/topic").unwrap();
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force().remove_untracked(true);
    repo.checkout_head(Some(&mut checkout)).unwrap();
    write_file(dir.path(), "other", "c1\n");
    commit_all(&repo, "c1");
    write_file(dir.path(), pair.1, "second\n");
    commit_all(&repo, "c2 adds the other name");
    write_file(dir.path(), "conf", "branch\n");
    commit_all(&repo, "c3 conflicts");
    {
        let mut index = repo.index().unwrap();
        index.remove_path(std::path::Path::new(pair.1)).unwrap();
        index.write().unwrap();
        std::fs::remove_file(dir.path().join(pair.1)).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        let signature = repo.signature().unwrap();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "c4 deletes it again",
            &tree,
            &[&parent],
        )
        .unwrap();
    }
    let tip = head_id(&repo);

    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    drop(repo);
    let repo = git2::Repository::open(dir.path()).unwrap();
    (dir, repo, onto, tip)
}

/// What a rebase that picked `c2` before #522 refused it left (a pick outside Browsitory can
/// still leave it): `c1` and `c2` landed through libgit2's ignore-case index on a detached HEAD,
/// `c3` paused on its conflict, and the state the rebase kept for it.
fn rebase_paused_after_a_pick_brought_in_a_case_pair(
    repo: &git2::Repository,
    onto: &str,
    tip: &str,
) -> git_core::rebase::RebaseState {
    use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseState, RebaseStop};
    let plan: Vec<RebasePlanEntry> = git_core::rebase::commits_since(repo, onto)
        .unwrap()
        .into_iter()
        .map(|commit| RebasePlanEntry {
            commit_id: commit.id,
            action: RebaseAction::Pick,
            combined_message: None,
        })
        .collect();
    assert_eq!(plan.len(), 4);
    let original_branch_ref = repo.head().unwrap().name().unwrap().to_string();
    let onto = repo
        .revparse_single(onto)
        .unwrap()
        .peel_to_commit()
        .unwrap();
    repo.checkout_tree(onto.as_object(), None).unwrap();
    repo.set_head_detached(onto.id()).unwrap();
    for entry in &plan[..2] {
        let commit = repo
            .find_commit(git2::Oid::from_str(&entry.commit_id).unwrap())
            .unwrap();
        repo.cherrypick(&commit, None).unwrap();
        let mut index = repo.index().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let committer = repo.signature().unwrap();
        repo.commit(
            Some("HEAD"),
            &commit.author(),
            &committer,
            commit.message().unwrap(),
            &tree,
            &[&head],
        )
        .unwrap();
        repo.cleanup_state().unwrap();
    }
    let expected_head = repo.head().unwrap().target().unwrap();
    let c3 = repo
        .find_commit(git2::Oid::from_str(&plan[2].commit_id).unwrap())
        .unwrap();
    repo.cherrypick(&c3, None).unwrap();
    assert!(repo.index().unwrap().has_conflicts());
    RebaseState {
        plan,
        cursor: 2,
        original_branch_ref,
        original_tip: git2::Oid::from_str(tip).unwrap(),
        onto: onto.id(),
        group_start_parent: None,
        expected_head,
        stop: RebaseStop::Conflict,
    }
}

#[test]
fn rebase_abort_restores_the_branch_after_a_pick_brought_in_a_case_pair() {
    // #511: `start_rebase` checks onto, HEAD and the index for a case pair, but a picked commit
    // can bring one in (refused since #522; a pick outside Browsitory still can). Abort refused
    // with `CasePair`, so the rebase couldn't be aborted. Abort now leaves the pair out of its
    // forced checkout and writes the pair's files directly.
    use git_core::rebase::abort_rebase;
    for (pair, own) in CASE_PAIRS
        .into_iter()
        .flat_map(|pair| [(pair, false), (pair, true)])
    {
        let (dir, repo, onto, tip) = pair_brought_in_by_a_rebase(pair);
        let state = rebase_paused_after_a_pick_brought_in_a_case_pair(&repo, &onto, &tip);
        // libgit2's pick of `c2` took `pair.1` for `pair.0`'s entry: the pair is split between
        // HEAD, the index and the disk, which holds both files.
        assert_eq!(
            read(dir.path(), pair.0).as_deref(),
            Some("first\n"),
            "{pair:?}"
        );
        assert_eq!(
            read(dir.path(), pair.1).as_deref(),
            Some("second\n"),
            "{pair:?}"
        );
        // Unless the index tracks it (then Abort discards it like any change to a file the
        // rebase tracks), what the user wrote there during the pause stays: the repository
        // doesn't hold it.
        let kept = own && !index_paths(dir.path()).contains(&pair.1.to_string());
        if own {
            write_file(dir.path(), pair.1, "mine\n");
        }

        let abort = abort_rebase(&repo, &state).unwrap();

        assert!(abort.restored, "{pair:?}");
        assert!(!repo.head_detached().unwrap(), "{pair:?}");
        assert_eq!(head_id(&repo), tip, "{pair:?}");
        assert_eq!(repo.state(), git2::RepositoryState::Clean, "{pair:?}");
        let mut expected = vec![pair.0.to_string(), "conf".to_string(), "other".to_string()];
        expected.sort();
        assert_eq!(index_paths(dir.path()), expected, "{pair:?}");
        assert_eq!(
            read(dir.path(), pair.0).as_deref(),
            Some("first\n"),
            "{pair:?}"
        );
        assert_eq!(
            read(dir.path(), pair.1).as_deref(),
            kept.then_some("mine\n"),
            "{pair:?}"
        );
        assert_eq!(
            read(dir.path(), "conf").as_deref(),
            Some("branch\n"),
            "{pair:?}"
        );
        assert_eq!(
            read(dir.path(), "other").as_deref(),
            Some("c1\n"),
            "{pair:?}"
        );
    }
}

#[test]
fn cherry_pick_abort_after_a_pick_brought_in_a_case_pair() {
    // #511: the same for a paused cherry-pick, whose Abort resets the paths the pick touched.
    // It refused with `WouldDiscard` (libgit2's diff took the file on disk for a change to the
    // other name's entry), or left the picked file behind. Browsitory's own pick refuses such a
    // commit since #522: the pick here is libgit2's, as a terminal's would be.
    for pair in CASE_PAIRS {
        let (dir, repo) = init_repo();
        write_file(dir.path(), pair.0, "first\n");
        write_file(dir.path(), "conf", "base\n");
        commit_all(&repo, "base");
        {
            let base = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch("side", &base, false).unwrap();
        }
        write_file(dir.path(), "conf", "main\n");
        commit_all(&repo, "main");
        let main = head_id(&repo);
        let main_ref = repo.head().unwrap().name().unwrap().to_string();
        repo.set_head("refs/heads/side").unwrap();
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force();
        repo.checkout_head(Some(&mut checkout)).unwrap();
        write_file(dir.path(), pair.1, "second\n");
        write_file(dir.path(), "conf", "side\n");
        commit_all(&repo, "side");
        let side = head_id(&repo);
        repo.set_head(&main_ref).unwrap();
        repo.checkout_head(Some(&mut checkout)).unwrap();
        assert_eq!(read(dir.path(), pair.1), None);
        repo.config()
            .unwrap()
            .set_bool("core.ignorecase", true)
            .unwrap();
        drop(repo);
        let repo = git2::Repository::open(dir.path()).unwrap();
        assert_eq!(head_id(&repo), main);

        repo.cherrypick(
            &repo
                .find_commit(git2::Oid::from_str(&side).unwrap())
                .unwrap(),
            None,
        )
        .unwrap();
        assert!(repo.index().unwrap().has_conflicts(), "{pair:?}");
        assert_eq!(
            read(dir.path(), pair.1).as_deref(),
            Some("second\n"),
            "{pair:?}"
        );
        // For `a`/`A` libgit2's index kept `a` and dropped the picked `A` (its file is written
        // all the same): Abort resets only what the pick touched, so `A` stays, untracked.
        let staged = index_paths(dir.path()).contains(&pair.1.to_string());

        git_core::merge::abort_merge(&repo).unwrap();

        assert_eq!(repo.state(), git2::RepositoryState::Clean, "{pair:?}");
        assert_eq!(head_id(&repo), main, "{pair:?}");
        let mut expected = vec![pair.0.to_string(), "conf".to_string()];
        expected.sort();
        assert_eq!(index_paths(dir.path()), expected, "{pair:?}");
        assert_eq!(
            read(dir.path(), pair.0).as_deref(),
            Some("first\n"),
            "{pair:?}"
        );
        assert_eq!(
            read(dir.path(), pair.1).as_deref(),
            (!staged).then_some("second\n"),
            "{pair:?}"
        );
        assert_eq!(
            read(dir.path(), "conf").as_deref(),
            Some("main\n"),
            "{pair:?}"
        );
    }
}
