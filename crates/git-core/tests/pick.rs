// crates/git-core/tests/pick.rs
mod common;

use git_core::merge::abort_merge;
use git_core::pick::{cherry_pick, revert, PickError, PickOutcome};
use std::path::Path;

/// main: base(shared.txt="one\n") ; feature: +feature.txt ; main stays at base.
fn repo_with_feature_branch() -> (tempfile::TempDir, git2::Repository, String) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "shared.txt", "one\n");
    common::commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &base, false).unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    common::write_file(dir.path(), "feature.txt", "feature\n");
    common::commit_all(&repo, "add feature");
    let feature_tip = repo.head().unwrap().target().unwrap().to_string();
    let main = repo
        .find_branch("master", git2::BranchType::Local)
        .or_else(|_| repo.find_branch("main", git2::BranchType::Local))
        .unwrap();
    let main_ref = main.get().name().unwrap().to_string();
    repo.set_head(&main_ref).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    drop(main);
    drop(base);
    (dir, repo, feature_tip)
}

fn stage(repo: &git2::Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

#[test]
fn clean_cherry_pick_commits_with_original_message_and_author() {
    let (dir, repo, feature_tip) = repo_with_feature_branch();
    let before = repo.head().unwrap().target().unwrap();
    let outcome = cherry_pick(&repo, &feature_tip).unwrap();
    let PickOutcome::Committed { commit_id } = outcome else {
        panic!("expected commit")
    };
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.id().to_string(), commit_id);
    // Not asserting id != feature_tip: same parent/tree/author/message and a same-second
    // committer timestamp legitimately yield an identical commit object.
    assert_eq!(head.parent_id(0).unwrap(), before);
    assert_eq!(head.message().unwrap(), "add feature");
    assert_eq!(head.parent_count(), 1);
    assert!(dir.path().join("feature.txt").exists());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn conflicting_cherry_pick_pauses_with_merge_msg_and_can_be_finished() {
    let (dir, repo, _) = repo_with_feature_branch();
    // feature2 edits shared.txt one way, main another.
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature2", &base, false).unwrap();
    repo.set_head("refs/heads/feature2").unwrap();
    common::write_file(dir.path(), "shared.txt", "theirs\n");
    common::commit_all(&repo, "theirs edit");
    let theirs = repo.head().unwrap().target().unwrap().to_string();
    repo.set_head(base_branch_ref(&repo).as_str()).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    common::write_file(dir.path(), "shared.txt", "ours\n");
    common::commit_all(&repo, "ours edit");

    let outcome = cherry_pick(&repo, &theirs).unwrap();

    assert_eq!(
        outcome,
        PickOutcome::Conflicted {
            files: vec!["shared.txt".into()]
        }
    );
    assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
    assert!(git_core::merge::merge_message(&repo)
        .unwrap()
        .contains("theirs edit"));

    common::write_file(dir.path(), "shared.txt", "resolved\n");
    stage(&repo, "shared.txt");
    drop(base);
    let mut repo = repo;
    git_core::commit::commit(
        &mut repo,
        "theirs edit",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn aborting_a_conflicted_cherry_pick_restores_head_and_files() {
    let (dir, repo, _) = repo_with_feature_branch();
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature2", &base, false).unwrap();
    repo.set_head("refs/heads/feature2").unwrap();
    common::write_file(dir.path(), "shared.txt", "theirs\n");
    common::commit_all(&repo, "theirs edit");
    let theirs = repo.head().unwrap().target().unwrap().to_string();
    repo.set_head(base_branch_ref(&repo).as_str()).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    common::write_file(dir.path(), "shared.txt", "ours\n");
    common::commit_all(&repo, "ours edit");
    let before = repo.head().unwrap().target().unwrap();

    cherry_pick(&repo, &theirs).unwrap();
    abort_merge(&repo).unwrap();

    assert_eq!(repo.head().unwrap().target().unwrap(), before);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "ours\n"
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn cherry_picking_an_already_applied_commit_is_empty_and_leaves_state_clean() {
    let (_dir, repo, feature_tip) = repo_with_feature_branch();
    cherry_pick(&repo, &feature_tip).unwrap();
    let head_before = repo.head().unwrap().target().unwrap();
    assert!(matches!(
        cherry_pick(&repo, &feature_tip),
        Err(PickError::EmptyResult)
    ));
    assert_eq!(repo.head().unwrap().target().unwrap(), head_before);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn cherry_pick_refuses_staged_changes() {
    let (dir, repo, feature_tip) = repo_with_feature_branch();
    common::write_file(dir.path(), "staged.txt", "x\n");
    stage(&repo, "staged.txt");
    assert!(matches!(
        cherry_pick(&repo, &feature_tip),
        Err(PickError::DirtyIndex)
    ));
}

#[test]
fn cherry_pick_never_overwrites_an_unstaged_edit_to_a_touched_file() {
    let (dir, repo, _) = repo_with_feature_branch();
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("edit", &base, false).unwrap();
    repo.set_head("refs/heads/edit").unwrap();
    common::write_file(dir.path(), "shared.txt", "from edit branch\n");
    common::commit_all(&repo, "edit shared");
    let edit = repo.head().unwrap().target().unwrap().to_string();
    repo.set_head(base_branch_ref(&repo).as_str()).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    common::write_file(dir.path(), "shared.txt", "my unsaved work\n");

    assert!(cherry_pick(&repo, &edit).is_err());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "my unsaved work\n"
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn cherry_pick_refuses_merge_commits_and_paused_operations() {
    let (dir, repo, feature_tip) = repo_with_feature_branch();
    // Build a merge commit by hand.
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let other = repo
        .find_commit(git2::Oid::from_str(&feature_tip).unwrap())
        .unwrap();
    let sig = repo.signature().unwrap();
    let tree = other.tree().unwrap();
    let merge_id = repo
        .commit(None, &sig, &sig, "merge", &tree, &[&head, &other])
        .unwrap();
    assert!(matches!(
        cherry_pick(&repo, &merge_id.to_string()),
        Err(PickError::MergeCommitUnsupported)
    ));
    assert!(matches!(
        revert(&repo, &merge_id.to_string()),
        Err(PickError::MergeCommitUnsupported)
    ));

    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{}\n", head.id())).unwrap();
    assert!(matches!(
        cherry_pick(&repo, &feature_tip),
        Err(PickError::OperationInProgress)
    ));
    let _ = dir;
}

#[test]
fn clean_revert_commits_the_inverse_with_the_standard_message() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "b.txt", "b\n");
    common::commit_all(&repo, "add b");
    let target = repo.head().unwrap().target().unwrap();

    let PickOutcome::Committed { .. } = revert(&repo, &target.to_string()).unwrap() else {
        panic!("expected commit")
    };

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head.message().unwrap(),
        format!("Revert \"add b\"\n\nThis reverts commit {target}.\n")
    );
    assert!(!dir.path().join("b.txt").exists());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn conflicting_revert_pauses_in_revert_state() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "one\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "a.txt", "two\n");
    common::commit_all(&repo, "to two");
    let target = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "three\n");
    common::commit_all(&repo, "to three");

    let outcome = revert(&repo, &target.to_string()).unwrap();

    assert_eq!(
        outcome,
        PickOutcome::Conflicted {
            files: vec!["a.txt".into()]
        }
    );
    assert_eq!(repo.state(), git2::RepositoryState::Revert);
}

fn base_branch_ref(repo: &git2::Repository) -> String {
    for name in ["main", "master"] {
        if let Ok(branch) = repo.find_branch(name, git2::BranchType::Local) {
            return branch.get().name().unwrap().to_string();
        }
    }
    panic!("no base branch");
}

/// main edits shared.txt to "ours"; returns the sha of a branch commit editing it to "theirs".
fn conflicting_pick_setup(dir: &std::path::Path, repo: &git2::Repository) -> String {
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature2", &base, false).unwrap();
    repo.set_head("refs/heads/feature2").unwrap();
    common::write_file(dir, "shared.txt", "theirs\n");
    common::commit_all(repo, "theirs edit");
    let theirs = repo.head().unwrap().target().unwrap().to_string();
    repo.set_head(base_branch_ref(repo).as_str()).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    common::write_file(dir, "shared.txt", "ours\n");
    common::commit_all(repo, "ours edit");
    theirs
}

#[test]
fn aborting_a_conflicted_cherry_pick_keeps_unrelated_unstaged_edits() {
    let (dir, repo, _) = repo_with_feature_branch();
    let theirs = conflicting_pick_setup(dir.path(), &repo);
    // feature.txt is not tracked on main; use a tracked unrelated file instead.
    common::write_file(dir.path(), "other.txt", "orig\n");
    common::commit_all(&repo, "add other");
    common::write_file(dir.path(), "other.txt", "my unsaved other work\n");

    assert!(matches!(
        cherry_pick(&repo, &theirs).unwrap(),
        PickOutcome::Conflicted { .. }
    ));
    abort_merge(&repo).unwrap();

    assert_eq!(
        std::fs::read_to_string(dir.path().join("other.txt")).unwrap(),
        "my unsaved other work\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "ours\n"
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn aborting_after_resolving_and_staging_a_cherry_pick_still_works() {
    let (dir, repo, _) = repo_with_feature_branch();
    let theirs = conflicting_pick_setup(dir.path(), &repo);
    cherry_pick(&repo, &theirs).unwrap();
    common::write_file(dir.path(), "shared.txt", "resolved\n");
    stage(&repo, "shared.txt");

    abort_merge(&repo).unwrap();

    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "ours\n"
    );
}

#[test]
fn abort_treats_conflicted_paths_literally_not_as_globs() {
    let (dir, repo, _) = repo_with_feature_branch();
    common::write_file(dir.path(), "[ab].txt", "base\n");
    common::write_file(dir.path(), "a.txt", "orig\n");
    common::commit_all(&repo, "add bracket and a");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature3", &base, false).unwrap();
    repo.set_head("refs/heads/feature3").unwrap();
    common::write_file(dir.path(), "[ab].txt", "theirs\n");
    common::commit_all(&repo, "theirs bracket");
    let theirs = repo.head().unwrap().target().unwrap().to_string();
    repo.set_head(base_branch_ref(&repo).as_str()).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    common::write_file(dir.path(), "[ab].txt", "ours\n");
    common::commit_all(&repo, "ours bracket");
    common::write_file(dir.path(), "a.txt", "my unsaved a work\n");

    assert!(matches!(
        cherry_pick(&repo, &theirs).unwrap(),
        PickOutcome::Conflicted { .. }
    ));
    abort_merge(&repo).unwrap();

    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "my unsaved a work\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("[ab].txt")).unwrap(),
        "ours\n"
    );
}

#[test]
fn finishing_a_conflicted_cherry_pick_keeps_the_original_author() {
    let (dir, repo, _) = repo_with_feature_branch();
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature2", &base, false).unwrap();
    repo.set_head("refs/heads/feature2").unwrap();
    common::write_file(dir.path(), "shared.txt", "theirs\n");
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let other = git2::Signature::now("Other Author", "other@example.com").unwrap();
    let theirs = repo
        .commit(Some("HEAD"), &other, &other, "theirs edit", &tree, &[&base])
        .unwrap()
        .to_string();
    drop(tree);
    repo.set_head(base_branch_ref(&repo).as_str()).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    common::write_file(dir.path(), "shared.txt", "ours\n");
    common::commit_all(&repo, "ours edit");

    assert!(matches!(
        cherry_pick(&repo, &theirs).unwrap(),
        PickOutcome::Conflicted { .. }
    ));
    common::write_file(dir.path(), "shared.txt", "resolved\n");
    stage(&repo, "shared.txt");
    drop(base);
    let mut repo = repo;
    git_core::commit::commit(
        &mut repo,
        "theirs edit",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.author().name().unwrap(), "Other Author");
    assert_eq!(head.author().email().unwrap(), "other@example.com");
    assert_eq!(head.committer().name().unwrap(), "Test User");
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}
