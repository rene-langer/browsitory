mod common;

use common::{init_repo, write_file};
use git_core::{
    RebaseAction, RebaseStatus, RebaseStep, abort_rebase, continue_rebase_step, create_commit,
    drive_rebase_step, plan_rebase, stage_path, start_rebase, status,
};

fn checkout(repo: &git2::Repository, refname: &str) {
    repo.set_head(refname).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
}

/// Sets up the default branch (whatever `init.defaultBranch` resolves to —
/// not hardcoded, since it varies by system git config) with a base commit,
/// then a `feature` branch three commits ahead of it (`c1`, `c2`, `c3`),
/// with `feature` left checked out and the default branch not diverged from
/// it otherwise — i.e. a plain linear rebase target. Returns the three
/// commit oids in application order and the default branch's short name
/// (e.g. `"main"` or `"master"`), for use as the rebase `upstream` argument.
fn three_commit_fixture() -> (tempfile::TempDir, git2::Repository, Vec<git2::Oid>, String) {
    let (dir, mut repo) = init_repo();
    write_file(&dir, "base.txt", "base\n");
    stage_path(&repo, "base.txt").unwrap();
    create_commit(&mut repo, "base").unwrap();
    let base_branch_ref = repo.head().unwrap().name().unwrap().to_string();
    let base_branch_short = base_branch_ref
        .strip_prefix("refs/heads/")
        .unwrap()
        .to_string();

    repo.branch(
        "feature",
        &repo.head().unwrap().peel_to_commit().unwrap(),
        false,
    )
    .unwrap();
    checkout(&repo, "refs/heads/feature");

    let mut oids = Vec::new();
    for i in 1..=3 {
        write_file(&dir, &format!("f{i}.txt"), &format!("content {i}\n"));
        stage_path(&repo, &format!("f{i}.txt")).unwrap();
        oids.push(create_commit(&mut repo, &format!("c{i}")).unwrap());
    }

    // Advance the default branch past base with an unrelated commit so the
    // rebase is a genuine "replay elsewhere", not a no-op fast-forward-shaped
    // rebase.
    checkout(&repo, &base_branch_ref);
    write_file(&dir, "main-only.txt", "main progress\n");
    stage_path(&repo, "main-only.txt").unwrap();
    create_commit(&mut repo, "main progress").unwrap();

    checkout(&repo, "refs/heads/feature");

    (dir, repo, oids, base_branch_short)
}

fn drive_to_completion(
    repo: &git2::Repository,
    rebase: &mut git2::Rebase<'_>,
    step: &RebaseStep,
) -> RebaseStatus {
    drive_rebase_step(repo, rebase, step).unwrap()
}

#[test]
fn plan_rebase_lists_commits_oldest_first() {
    let (_dir, repo, oids, upstream) = three_commit_fixture();

    let plan = plan_rebase(&repo, &upstream, None).unwrap();

    assert_eq!(plan.iter().map(|c| c.id).collect::<Vec<_>>(), oids);
}

#[test]
fn linear_pick_only_rebase_replays_all_commits_onto_upstream() {
    let (dir, repo, oids, upstream) = three_commit_fixture();
    let main_tip = repo
        .find_reference(&format!("refs/heads/{upstream}"))
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id();

    let mut rebase = start_rebase(&repo, &upstream, None, None).unwrap();
    let mut status_result = RebaseStatus::Done;
    for &oid in &oids {
        let step = RebaseStep {
            commit: oid,
            action: RebaseAction::Pick,
        };
        status_result = drive_to_completion(&repo, &mut rebase, &step);
        assert!(!matches!(status_result, RebaseStatus::Conflict { .. }));
    }
    assert_eq!(status_result, RebaseStatus::Done);
    drop(rebase);

    // All three original files should exist, `main-only.txt` (from the new
    // base) should exist too, and the new history should have the upstream
    // commit as an ancestor.
    for i in 1..=3 {
        assert!(dir.path().join(format!("f{i}.txt")).exists());
    }
    assert!(dir.path().join("main-only.txt").exists());

    let new_head = repo.head().unwrap().peel_to_commit().unwrap();
    let mut found_main_tip = false;
    let mut walk = repo.revwalk().unwrap();
    walk.push(new_head.id()).unwrap();
    for oid in walk {
        if oid.unwrap() == main_tip {
            found_main_tip = true;
        }
    }
    assert!(
        found_main_tip,
        "new history should include the upstream tip"
    );
    assert!(status(&repo).unwrap().is_empty());
}

#[test]
fn squash_combines_two_commits_into_one_with_merged_message() {
    let (dir, repo, oids, upstream) = three_commit_fixture();

    let mut rebase = start_rebase(&repo, &upstream, None, None).unwrap();

    let step1 = RebaseStep {
        commit: oids[0],
        action: RebaseAction::Pick,
    };
    let status1 = drive_to_completion(&repo, &mut rebase, &step1);
    assert_eq!(status1, RebaseStatus::StepComplete { step_index: 0 });

    let step2 = RebaseStep {
        commit: oids[1],
        action: RebaseAction::Squash,
    };
    let status2 = drive_to_completion(&repo, &mut rebase, &step2);
    assert_eq!(status2, RebaseStatus::StepComplete { step_index: 1 });

    let step3 = RebaseStep {
        commit: oids[2],
        action: RebaseAction::Pick,
    };
    let status3 = drive_to_completion(&repo, &mut rebase, &step3);
    assert_eq!(status3, RebaseStatus::Done);
    drop(rebase);

    // Final history: base -> main progress -> (c1+c2 squashed) -> c3.
    let mut walk = repo.revwalk().unwrap();
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
        .unwrap();
    walk.push_head().unwrap();
    let commits: Vec<git2::Commit> = walk
        .map(|oid| repo.find_commit(oid.unwrap()).unwrap())
        .collect();

    // c3 (HEAD), squashed(c1+c2), main progress, base — 4 commits total,
    // down from the 5 that would exist with a plain pick-only rebase.
    assert_eq!(commits.len(), 4);

    let squashed = &commits[1];
    assert!(squashed.message().unwrap().contains("c1"));
    assert!(squashed.message().unwrap().contains("c2"));
    assert_eq!(squashed.parent_ids().count(), 1);

    // Both files' content should be present in the squashed commit's tree.
    let tree = squashed.tree().unwrap();
    assert!(tree.get_path(std::path::Path::new("f1.txt")).is_ok());
    assert!(tree.get_path(std::path::Path::new("f2.txt")).is_ok());
    assert!(dir.path().join("f1.txt").exists());
    assert!(dir.path().join("f2.txt").exists());
    assert!(dir.path().join("f3.txt").exists());
    assert!(status(&repo).unwrap().is_empty());
}

#[test]
fn fixup_combines_two_commits_and_discards_the_second_message() {
    let (_dir, repo, oids, upstream) = three_commit_fixture();

    let mut rebase = start_rebase(&repo, &upstream, None, None).unwrap();

    let step1 = RebaseStep {
        commit: oids[0],
        action: RebaseAction::Pick,
    };
    drive_to_completion(&repo, &mut rebase, &step1);

    let step2 = RebaseStep {
        commit: oids[1],
        action: RebaseAction::Fixup,
    };
    drive_to_completion(&repo, &mut rebase, &step2);

    let step3 = RebaseStep {
        commit: oids[2],
        action: RebaseAction::Pick,
    };
    let final_status = drive_to_completion(&repo, &mut rebase, &step3);
    assert_eq!(final_status, RebaseStatus::Done);
    drop(rebase);

    let mut walk = repo.revwalk().unwrap();
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
        .unwrap();
    walk.push_head().unwrap();
    let commits: Vec<git2::Commit> = walk
        .map(|oid| repo.find_commit(oid.unwrap()).unwrap())
        .collect();

    assert_eq!(commits.len(), 4);
    let fixed_up = &commits[1];
    assert_eq!(fixed_up.message().unwrap().trim(), "c1");
    assert!(!fixed_up.message().unwrap().contains("c2"));
    assert_eq!(fixed_up.parent_ids().count(), 1);
}

#[test]
fn reword_replaces_the_commit_message_and_keeps_the_tree() {
    let (_dir, repo, oids, upstream) = three_commit_fixture();

    let mut rebase = start_rebase(&repo, &upstream, None, None).unwrap();
    let step = RebaseStep {
        commit: oids[0],
        action: RebaseAction::Reword("new message".to_string()),
    };
    let status1 = drive_to_completion(&repo, &mut rebase, &step);
    assert_eq!(status1, RebaseStatus::StepComplete { step_index: 0 });

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.summary().unwrap(), Some("new message"));

    let step2 = RebaseStep {
        commit: oids[1],
        action: RebaseAction::Pick,
    };
    drive_to_completion(&repo, &mut rebase, &step2);
    let step3 = RebaseStep {
        commit: oids[2],
        action: RebaseAction::Pick,
    };
    let final_status = drive_to_completion(&repo, &mut rebase, &step3);
    assert_eq!(final_status, RebaseStatus::Done);
}

#[test]
fn drop_discards_that_commits_change_but_keeps_the_others() {
    let (dir, repo, oids, upstream) = three_commit_fixture();

    let mut rebase = start_rebase(&repo, &upstream, None, None).unwrap();
    let step1 = RebaseStep {
        commit: oids[0],
        action: RebaseAction::Pick,
    };
    drive_to_completion(&repo, &mut rebase, &step1);

    let step2 = RebaseStep {
        commit: oids[1],
        action: RebaseAction::Drop,
    };
    let status2 = drive_to_completion(&repo, &mut rebase, &step2);
    assert_eq!(status2, RebaseStatus::StepComplete { step_index: 1 });

    let step3 = RebaseStep {
        commit: oids[2],
        action: RebaseAction::Pick,
    };
    let final_status = drive_to_completion(&repo, &mut rebase, &step3);
    assert_eq!(final_status, RebaseStatus::Done);
    drop(rebase);

    assert!(dir.path().join("f1.txt").exists());
    assert!(!dir.path().join("f2.txt").exists());
    assert!(dir.path().join("f3.txt").exists());
    assert!(status(&repo).unwrap().is_empty());
}

#[test]
fn conflicting_step_pauses_then_continues_after_resolution() {
    let (dir, mut repo) = init_repo();
    write_file(&dir, "shared.txt", "one\n");
    stage_path(&repo, "shared.txt").unwrap();
    create_commit(&mut repo, "base").unwrap();
    let base_branch = repo.head().unwrap().name().unwrap().to_string();

    repo.branch(
        "feature",
        &repo.head().unwrap().peel_to_commit().unwrap(),
        false,
    )
    .unwrap();
    checkout(&repo, "refs/heads/feature");
    write_file(&dir, "shared.txt", "one\nfrom feature\n");
    stage_path(&repo, "shared.txt").unwrap();
    let feature_commit = create_commit(&mut repo, "edit on feature").unwrap();

    checkout(&repo, &base_branch);
    write_file(&dir, "shared.txt", "one\nfrom main\n");
    stage_path(&repo, "shared.txt").unwrap();
    create_commit(&mut repo, "edit on main").unwrap();

    checkout(&repo, "refs/heads/feature");

    let mut rebase = start_rebase(&repo, &base_branch, None, None).unwrap();
    let step = RebaseStep {
        commit: feature_commit,
        action: RebaseAction::Pick,
    };
    let status1 = drive_rebase_step(&repo, &mut rebase, &step).unwrap();
    let RebaseStatus::Conflict { paths, .. } = status1 else {
        panic!("expected Conflict, got {status1:?}");
    };
    assert_eq!(paths, vec!["shared.txt".to_string()]);

    // Resolve by taking the feature-branch content verbatim, then stage it.
    write_file(&dir, "shared.txt", "one\nfrom feature\n");
    stage_path(&repo, "shared.txt").unwrap();

    let status2 = continue_rebase_step(&repo, &mut rebase, &step).unwrap();
    assert_eq!(status2, RebaseStatus::Done);
    drop(rebase);

    let content = std::fs::read_to_string(dir.path().join("shared.txt")).unwrap();
    assert_eq!(content, "one\nfrom feature\n");
    assert!(status(&repo).unwrap().is_empty());
}

/// `crates/app/src/worker.rs` deliberately does NOT hold a live `Rebase`
/// handle across commands (see its module doc comment) — instead each
/// rebase-related command reopens the in-progress rebase fresh via
/// `repo.open_rebase(None)`. That relies on `start_rebase`'s underlying
/// `Repository::rebase()` call persisting enough state to disk immediately
/// (before any `next()`), and on that persisted state surviving being
/// reopened mid-sequence. This test exercises exactly that pattern directly,
/// rather than trusting it as an assumption behind the worker's design.
#[test]
fn rebase_can_be_reopened_from_disk_between_steps_like_the_worker_does() {
    let (dir, repo, oids, upstream) = three_commit_fixture();

    {
        let rebase = start_rebase(&repo, &upstream, None, None).unwrap();
        drop(rebase); // persisted to `.git/rebase-merge/`, not kept alive
    }

    for &oid in &oids {
        let mut rebase = repo.open_rebase(None).unwrap();
        let step = RebaseStep {
            commit: oid,
            action: RebaseAction::Pick,
        };
        let result = drive_rebase_step(&repo, &mut rebase, &step).unwrap();
        assert!(!matches!(result, RebaseStatus::Conflict { .. }));
        drop(rebase); // reopened fresh next iteration, exactly like the worker
    }

    for i in 1..=3 {
        assert!(dir.path().join(format!("f{i}.txt")).exists());
    }
    assert!(status(&repo).unwrap().is_empty());
}

#[test]
fn abort_rebase_restores_the_original_branch_tip() {
    let (_dir, repo, oids, upstream) = three_commit_fixture();
    let original_tip = repo.head().unwrap().peel_to_commit().unwrap().id();
    assert_eq!(original_tip, *oids.last().unwrap());

    let mut rebase = start_rebase(&repo, &upstream, None, None).unwrap();
    let step1 = RebaseStep {
        commit: oids[0],
        action: RebaseAction::Pick,
    };
    drive_to_completion(&repo, &mut rebase, &step1);

    abort_rebase(&repo, rebase).unwrap();

    let head = repo.head().unwrap();
    assert_eq!(head.name().unwrap(), "refs/heads/feature");
    assert_eq!(head.peel_to_commit().unwrap().id(), original_tip);
    assert!(status(&repo).unwrap().is_empty());
}
