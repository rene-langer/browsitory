//! The parents of the commit that concludes a merge: like `git commit`, the ones already in
//! another parent's history are dropped (git's `reduce_heads`) unless `MERGE_MODE` says
//! `no-ff` (#227).
mod common;

use common::{commit_all, init_repo, write_file};
use git2::{Oid, Repository};

/// `base`, then `side` (on a branch) and `main` (checked out) on top of it.
struct Repo {
    _dir: tempfile::TempDir,
    repo: Repository,
    base: Oid,
    side: Oid,
    main: Oid,
}

fn head(repo: &Repository) -> Oid {
    repo.head().unwrap().peel_to_commit().unwrap().id()
}

fn diverged() -> Repo {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "base\n");
    commit_all(&repo, "base");
    let base = head(&repo);
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
    write_file(dir.path(), "side.txt", "side\n");
    commit_all(&repo, "side");
    let side = head(&repo);
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "main.txt", "main\n");
    commit_all(&repo, "main");
    let main = head(&repo);
    Repo {
        _dir: dir,
        repo,
        base,
        side,
        main,
    }
}

fn write_merge_head(repo: &Repository, heads: &[Oid]) {
    let text: String = heads.iter().map(|oid| format!("{oid}\n")).collect();
    std::fs::write(repo.path().join("MERGE_HEAD"), text).unwrap();
    std::fs::write(repo.path().join("MERGE_MSG"), "Merge\n").unwrap();
}

fn commit(repo: &mut Repository) -> Vec<Oid> {
    git_core::commit::commit(repo, "Merge", git_core::hooks::HookOptions::default()).unwrap();
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    repo.head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .parent_ids()
        .collect()
}

#[test]
fn a_merge_head_already_in_heads_history_is_dropped_and_the_others_stay() {
    let Repo {
        _dir,
        mut repo,
        base,
        side,
        main,
    } = diverged();
    write_merge_head(&repo, &[base, side]);

    assert_eq!(commit(&mut repo), vec![main, side]);
}

#[test]
fn a_merge_head_in_another_merge_heads_history_is_dropped() {
    let Repo {
        _dir,
        mut repo,
        side,
        main,
        ..
    } = diverged();
    // A second commit on `side`: `side` itself is in its history.
    let side_tip = {
        let parent = repo.find_commit(side).unwrap();
        let tree = parent.tree().unwrap();
        let signature = repo.signature().unwrap();
        repo.commit(None, &signature, &signature, "side 2", &tree, &[&parent])
            .unwrap()
    };
    write_merge_head(&repo, &[side, side_tip]);

    assert_eq!(commit(&mut repo), vec![main, side_tip]);
}

#[test]
fn a_repeated_merge_head_becomes_one_parent() {
    let Repo {
        _dir,
        mut repo,
        side,
        main,
        ..
    } = diverged();
    write_merge_head(&repo, &[side, side]);

    assert_eq!(commit(&mut repo), vec![main, side]);
}

#[test]
fn head_in_a_merge_heads_history_is_dropped_like_git_does() {
    // `git commit` reduces HEAD together with MERGE_HEAD: a HEAD the merged commit already
    // contains is not a parent either.
    let Repo {
        _dir,
        mut repo,
        base,
        side,
        ..
    } = diverged();
    repo.set_head_detached(base).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_merge_head(&repo, &[side]);

    assert_eq!(commit(&mut repo), vec![side]);
}

#[test]
fn merge_mode_no_ff_keeps_every_parent() {
    let Repo {
        _dir,
        mut repo,
        base,
        side,
        main,
    } = diverged();
    write_merge_head(&repo, &[base, side]);
    std::fs::write(repo.path().join("MERGE_MODE"), "no-ff").unwrap();

    assert_eq!(commit(&mut repo), vec![main, base, side]);
}

#[test]
fn a_no_ff_merge_records_merge_mode_no_ff_like_git() {
    let Repo { _dir, repo, .. } = diverged();
    let options = git_core::merge::MergeOptions {
        fast_forward: git_core::merge::FastForward::Never,
        squash: false,
    };

    git_core::merge::start_merge(&repo, "side", options).unwrap();

    assert_eq!(
        std::fs::read_to_string(repo.path().join("MERGE_MODE")).unwrap(),
        "no-ff"
    );
}

#[test]
fn an_ordinary_merge_leaves_merge_mode_empty() {
    let Repo { _dir, repo, .. } = diverged();

    git_core::merge::start_merge(&repo, "side", git_core::merge::MergeOptions::default()).unwrap();

    assert_eq!(
        std::fs::read_to_string(repo.path().join("MERGE_MODE")).unwrap(),
        ""
    );
}
