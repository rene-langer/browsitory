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
    let outcome = cherry_pick(
        &repo,
        &feature_tip,
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
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

    let outcome = cherry_pick(
        &repo,
        &theirs,
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

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

    cherry_pick(
        &repo,
        &theirs,
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
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
    cherry_pick(
        &repo,
        &feature_tip,
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    let head_before = repo.head().unwrap().target().unwrap();
    assert!(matches!(
        cherry_pick(
            &repo,
            &feature_tip,
            None,
            git_core::hooks::HookOptions::default()
        ),
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
        cherry_pick(
            &repo,
            &feature_tip,
            None,
            git_core::hooks::HookOptions::default()
        ),
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

    assert!(cherry_pick(&repo, &edit, None, git_core::hooks::HookOptions::default()).is_err());
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
        cherry_pick(
            &repo,
            &merge_id.to_string(),
            None,
            git_core::hooks::HookOptions::default()
        ),
        Err(PickError::MainlineRequired)
    ));
    assert!(matches!(
        revert(
            &repo,
            &merge_id.to_string(),
            None,
            git_core::hooks::HookOptions::default()
        ),
        Err(PickError::MainlineRequired)
    ));

    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{}\n", head.id())).unwrap();
    assert!(matches!(
        cherry_pick(
            &repo,
            &feature_tip,
            None,
            git_core::hooks::HookOptions::default()
        ),
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

    let PickOutcome::Committed { .. } = revert(
        &repo,
        &target.to_string(),
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap() else {
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

    let outcome = revert(
        &repo,
        &target.to_string(),
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

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
        cherry_pick(
            &repo,
            &theirs,
            None,
            git_core::hooks::HookOptions::default()
        )
        .unwrap(),
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
    cherry_pick(
        &repo,
        &theirs,
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
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
        cherry_pick(
            &repo,
            &theirs,
            None,
            git_core::hooks::HookOptions::default()
        )
        .unwrap(),
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
        cherry_pick(
            &repo,
            &theirs,
            None,
            git_core::hooks::HookOptions::default()
        )
        .unwrap(),
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

/// base(a.txt) → main adds m.txt ; feature adds f.txt ; M = merge(main, feature) on the base
/// branch. HEAD is left on a branch "target" at base.
fn merge_commit_fixture() -> (tempfile::TempDir, git2::Repository, String, String) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "base\n");
    common::commit_all(&repo, "base");
    let base_ref = base_branch_ref(&repo);
    let (merge_id, main_tip) = {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &base, false).unwrap();
        repo.branch("target", &base, false).unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        common::write_file(dir.path(), "f.txt", "feature\n");
        common::commit_all(&repo, "add f");
        let feature = repo.head().unwrap().peel_to_commit().unwrap();
        repo.set_head(&base_ref).unwrap();
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::new()
                .force()
                .remove_untracked(true),
        ))
        .unwrap();
        common::write_file(dir.path(), "m.txt", "main\n");
        common::commit_all(&repo, "add m");
        let main = repo.head().unwrap().peel_to_commit().unwrap();
        let mut merged = repo.merge_commits(&main, &feature, None).unwrap();
        let tree = repo
            .find_tree(merged.write_tree_to(&repo).unwrap())
            .unwrap();
        let sig = repo.signature().unwrap();
        let merge = repo
            .commit(
                Some(&base_ref),
                &sig,
                &sig,
                "merge feature",
                &tree,
                &[&main, &feature],
            )
            .unwrap();
        (merge.to_string(), main.id().to_string())
    };
    repo.set_head("refs/heads/target").unwrap();
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new()
            .force()
            .remove_untracked(true),
    ))
    .unwrap();
    (dir, repo, merge_id, main_tip)
}

#[test]
fn a_merge_commit_needs_a_mainline() {
    let (_dir, repo, merge, _) = merge_commit_fixture();
    let before = repo.head().unwrap().target().unwrap();
    assert!(matches!(
        cherry_pick(&repo, &merge, None, git_core::hooks::HookOptions::default()),
        Err(PickError::MainlineRequired)
    ));
    assert!(matches!(
        revert(&repo, &merge, None, git_core::hooks::HookOptions::default()),
        Err(PickError::MainlineRequired)
    ));
    assert_eq!(repo.head().unwrap().target().unwrap(), before);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn cherry_picking_a_merge_against_parent_one_applies_the_merged_side() {
    let (dir, repo, merge, _) = merge_commit_fixture();
    let outcome = cherry_pick(
        &repo,
        &merge,
        Some(1),
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    assert!(matches!(outcome, PickOutcome::Committed { .. }));
    assert!(dir.path().join("f.txt").exists());
    assert!(!dir.path().join("m.txt").exists());
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_count(), 1);
    assert_eq!(head.message().unwrap(), "merge feature");
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn reverting_a_merge_against_parent_one_removes_the_merged_side_with_gits_message() {
    let (dir, repo, merge, main_tip) = merge_commit_fixture();
    // Put HEAD on the merge itself.
    repo.set_head(&base_branch_ref(&repo)).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    let outcome = revert(
        &repo,
        &merge,
        Some(1),
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    assert!(matches!(outcome, PickOutcome::Committed { .. }));
    assert!(!dir.path().join("f.txt").exists());
    assert!(dir.path().join("m.txt").exists());
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head.message().unwrap(),
        format!(
            "Revert \"merge feature\"\n\nThis reverts commit {merge}, reversing\nchanges made to {main_tip}.\n"
        )
    );
}

#[test]
fn an_out_of_range_or_needless_mainline_is_refused() {
    let (_dir, repo, merge, main_tip) = merge_commit_fixture();
    assert!(matches!(
        cherry_pick(
            &repo,
            &merge,
            Some(3),
            git_core::hooks::HookOptions::default()
        ),
        Err(PickError::InvalidMainline { mainline: 3 })
    ));
    assert!(matches!(
        cherry_pick(
            &repo,
            &merge,
            Some(0),
            git_core::hooks::HookOptions::default()
        ),
        Err(PickError::InvalidMainline { mainline: 0 })
    ));
    assert!(matches!(
        cherry_pick(
            &repo,
            &main_tip,
            Some(1),
            git_core::hooks::HookOptions::default()
        ),
        Err(PickError::InvalidMainline { mainline: 1 })
    ));
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert_eq!(
        PickError::InvalidMainline { mainline: 3 }.to_string(),
        "Parent 3 doesn't exist on this commit."
    );
    assert_eq!(
        PickError::MainlineRequired.to_string(),
        "This is a merge commit. Choose which parent to apply it against."
    );
}

// ---- hooks on a clean pick (#183) ------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_clean_cherry_pick_and_revert_run_prepare_commit_msg_and_post_commit() {
    let (_dir, repo, feature_tip) = repo_with_feature_branch();
    common::log_hooks(
        &repo,
        &[
            "pre-commit",
            "prepare-commit-msg",
            "commit-msg",
            "post-commit",
        ],
    );

    let PickOutcome::Committed { commit_id } = cherry_pick(
        &repo,
        &feature_tip,
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap() else {
        panic!("expected a commit")
    };
    revert(
        &repo,
        &commit_id,
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    assert_eq!(
        common::read_log(&repo),
        "prepare-commit-msg <msg> message\npost-commit \n".repeat(2)
    );
}

#[cfg(unix)]
#[test]
fn a_failing_prepare_commit_msg_leaves_a_single_pick_staged() {
    let (dir, repo, feature_tip) = repo_with_feature_branch();
    let before = repo.head().unwrap().target().unwrap();
    common::install_hook(
        &repo.path().join("hooks"),
        "prepare-commit-msg",
        "echo rejected; exit 1",
    );

    let outcome = cherry_pick(
        &repo,
        &feature_tip,
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    assert_eq!(
        outcome,
        PickOutcome::HookFailed {
            hook: "prepare-commit-msg".to_string(),
            output: "rejected\n".to_string()
        }
    );
    assert_eq!(repo.head().unwrap().target().unwrap(), before);
    assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
    assert!(dir.path().join("feature.txt").exists());
    let index = repo.index().unwrap();
    assert!(index.get_path(Path::new("feature.txt"), 0).is_some());
}

#[cfg(unix)]
#[test]
fn a_prepare_commit_msg_that_empties_the_message_leaves_the_pick_staged() {
    let (_dir, repo, feature_tip) = repo_with_feature_branch();
    common::install_hook(
        &repo.path().join("hooks"),
        "prepare-commit-msg",
        r#": > "$1""#,
    );

    let outcome = cherry_pick(
        &repo,
        &feature_tip,
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    assert_eq!(
        outcome,
        PickOutcome::HookFailed {
            hook: "prepare-commit-msg".to_string(),
            output: "Aborting commit due to empty commit message.".to_string()
        }
    );
    assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
}
