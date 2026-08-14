mod common;

use common::{commit_all, init_repo, write_file};
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};

fn commit_id_at(repo: &git2::Repository, offset_from_head: usize) -> String {
    // Walks back `offset_from_head` first-parent steps from HEAD and returns that commit's id —
    // a small test helper for picking out a specific commit to build a plan entry around.
    let mut commit = repo.head().unwrap().peel_to_commit().unwrap();
    for _ in 0..offset_from_head {
        commit = commit.parent(0).unwrap();
    }
    commit.id().to_string()
}

fn pick(commit_id: &str) -> RebasePlanEntry {
    RebasePlanEntry {
        commit_id: commit_id.to_string(),
        action: RebaseAction::Pick,
        combined_message: None,
    }
}

#[test]
fn commits_since_lists_commits_oldest_first_between_onto_and_head() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");

    let commits = git_core::rebase::commits_since(&repo, &onto_id).unwrap();

    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].summary, "add a");
    assert_eq!(commits[1].summary, "add b");
}

#[test]
fn commits_since_returns_empty_when_onto_is_head() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");

    let commits = git_core::rebase::commits_since(&repo, "HEAD").unwrap();

    assert!(commits.is_empty());
}

#[test]
fn start_rebase_rejects_a_plan_starting_with_squash_or_fixup() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    let onto = commit_id_at(&repo, 1);
    let a_id = commit_id_at(&repo, 0);

    let plan = vec![RebasePlanEntry {
        commit_id: a_id,
        action: RebaseAction::Squash,
        combined_message: None,
    }];

    let result = git_core::rebase::start_rebase(&repo, &onto, plan);

    assert!(result.is_err());
}

#[test]
fn a_clean_multi_pick_rebase_lands_every_commit_and_finishes() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan: Vec<RebasePlanEntry> = commits.iter().map(|c| pick(&c.id)).collect();

    let (state, first_result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(first_result, RebaseStepResult::Done);
    assert_eq!(state.current_step(), state.total_steps());

    let commits_after = git_core::rebase::commits_since(&repo, &onto).unwrap();
    assert_eq!(commits_after.len(), 2);
    assert_eq!(commits_after[0].summary, "add a");
    assert_eq!(commits_after[1].summary, "add b");
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn drop_removes_a_commit_from_the_resulting_history() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![
        RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: RebaseAction::Drop,
            combined_message: None,
        },
        pick(&commits[1].id),
    ];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let commits_after = git_core::rebase::commits_since(&repo, &onto).unwrap();
    assert_eq!(commits_after.len(), 1);
    assert_eq!(commits_after[0].summary, "add b");
    assert!(!dir.path().join("a.txt").exists());
    assert!(dir.path().join("b.txt").exists());
}

#[test]
fn reword_uses_the_new_message_not_the_original() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "original message");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![RebasePlanEntry {
        commit_id: commits[0].id.clone(),
        action: RebaseAction::Reword {
            message: "reworded message".to_string(),
        },
        combined_message: None,
    }];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head_commit.message().ok().unwrap_or_default(),
        "reworded message"
    );
}

#[test]
fn squash_combines_a_group_into_one_commit_with_the_leaders_combined_message() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![
        RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: RebaseAction::Pick,
            combined_message: Some("combined: add a and b".to_string()),
        },
        RebasePlanEntry {
            commit_id: commits[1].id.clone(),
            action: RebaseAction::Squash,
            combined_message: None,
        },
    ];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let commits_after = git_core::rebase::commits_since(&repo, &onto).unwrap();
    // Exactly one commit — the intermediate "add a" step was collapsed away, not left standing
    // alongside the squashed result.
    assert_eq!(commits_after.len(), 1);
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head_commit.message().ok().unwrap_or_default(),
        "combined: add a and b"
    );
    assert_eq!(head_commit.parent_count(), 1);
    let parent = head_commit.parent(0).unwrap();
    assert_eq!(parent.id().to_string(), onto);
    // The combined tree reflects both changes.
    assert!(dir.path().join("a.txt").exists());
    assert!(dir.path().join("b.txt").exists());
}

#[test]
fn a_mixed_squash_and_fixup_group_still_collapses_to_one_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b (fixup target)");
    write_file(dir.path(), "c.txt", "c\n");
    commit_all(&repo, "add c");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![
        RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: RebaseAction::Pick,
            combined_message: Some("combined: a, b, c".to_string()),
        },
        RebasePlanEntry {
            commit_id: commits[1].id.clone(),
            action: RebaseAction::Fixup,
            combined_message: None,
        },
        RebasePlanEntry {
            commit_id: commits[2].id.clone(),
            action: RebaseAction::Squash,
            combined_message: None,
        },
    ];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let commits_after = git_core::rebase::commits_since(&repo, &onto).unwrap();
    assert_eq!(commits_after.len(), 1);
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head_commit.message().ok().unwrap_or_default(),
        "combined: a, b, c"
    );
    assert!(dir.path().join("a.txt").exists());
    assert!(dir.path().join("b.txt").exists());
    assert!(dir.path().join("c.txt").exists());
}

#[test]
fn a_conflicting_pick_pauses_and_resolving_then_continuing_lands_it() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "shared.txt", "line one\nchanged on top\n");
    commit_all(&repo, "change on top of onto");
    // A second, independent base to diverge from — so replaying "conflicting change" onto
    // "onto" (which itself has an unrelated edit to the same line) produces a real conflict.
    write_file(dir.path(), "shared.txt", "line one\nchanged again\n");
    commit_all(&repo, "conflicting change");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    // Only rebase the second commit ("conflicting change") onto the first's parent — but since
    // the first commit ("change on top of onto") is what actually landed on `onto` already, we
    // rebase starting from `onto` directly with just the conflicting commit, forcing a genuine
    // conflict against `onto`'s own content.
    let plan = vec![pick(&commits[1].id)];

    let (mut state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    let files = match result {
        RebaseStepResult::Conflicted { files } => files,
        other => panic!("expected Conflicted, got {other:?}"),
    };
    assert_eq!(files, vec!["shared.txt".to_string()]);

    // Resolve exactly like a merge conflict — same index, same write-then-stage mechanics.
    let workdir = repo.workdir().unwrap();
    std::fs::write(workdir.join("shared.txt"), "line one\nresolved\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("shared.txt")).unwrap();
    index.write().unwrap();

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let contents = std::fs::read_to_string(dir.path().join("shared.txt")).unwrap();
    assert_eq!(contents, "line one\nresolved\n");
}

#[test]
fn an_edit_step_pauses_and_continuing_after_a_manual_amend_lands_the_amended_tree() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "original\n");
    commit_all(&repo, "add a");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![RebasePlanEntry {
        commit_id: commits[0].id.clone(),
        action: RebaseAction::Edit,
        combined_message: None,
    }];

    let (mut state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert_eq!(result, RebaseStepResult::PausedForEdit);

    // Amend: change the file further and stage it, exactly like the normal Stage flow would.
    write_file(dir.path(), "a.txt", "amended\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let contents = std::fs::read_to_string(dir.path().join("a.txt")).unwrap();
    assert_eq!(contents, "amended\n");
}

#[test]
fn finishing_moves_the_original_branch_and_reattaches_head() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    let branch_name = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![pick(&commits[0].id)];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let head_ref = repo.head().unwrap();
    assert_eq!(head_ref.shorthand().unwrap(), branch_name);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}
