mod common;

use common::{commit_all, init_repo, write_file};
use git_core::rebase::{RebaseAction, RebaseError, RebasePlanEntry, RebaseStepResult};

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

/// Stages `relative_path` (already written to disk) and commits it on `HEAD` with an explicit
/// author distinct from the repo's ambient `user.name`/`user.email` — `commit_all` always uses
/// the ambient signature for both author and committer, so it can't be used to pin the
/// author-preservation behavior this test group covers.
fn commit_with_author(
    repo: &git2::Repository,
    relative_path: &str,
    message: &str,
    author_name: &str,
    author_email: &str,
) {
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(relative_path)).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    let author = git2::Signature::now(author_name, author_email).unwrap();
    let committer = repo.signature().unwrap();
    repo.commit(
        Some("HEAD"),
        &author,
        &committer,
        message,
        &tree,
        &[&parent],
    )
    .unwrap();
}

#[test]
fn pick_preserves_the_original_authors_identity_but_uses_a_fresh_committer() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);

    write_file(dir.path(), "a.txt", "a\n");
    commit_with_author(&repo, "a.txt", "add a", "Alice", "alice@example.com");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![pick(&commits[0].id)];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    // Author: preserved from the original commit, per the plan's stated constraint.
    assert_eq!(head_commit.author().name().unwrap(), "Alice");
    // Committer: a fresh signature for the rebase itself, i.e. the ambient test-repo identity
    // set up by `init_repo` — not Alice's.
    assert_eq!(head_commit.committer().name().unwrap(), "Test User");
}

#[test]
fn squash_group_preserves_the_leaders_author_not_the_last_members() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);

    write_file(dir.path(), "a.txt", "a\n");
    commit_with_author(&repo, "a.txt", "add a", "Alice", "alice@example.com");
    write_file(dir.path(), "b.txt", "b\n");
    commit_with_author(&repo, "b.txt", "add b", "Bob", "bob@example.com");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![
        RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: RebaseAction::Pick,
            combined_message: Some("combined: a and b".to_string()),
        },
        RebasePlanEntry {
            commit_id: commits[1].id.clone(),
            action: RebaseAction::Squash,
            combined_message: None,
        },
    ];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    // The leader (Alice's commit) is the semantically-original commit for the whole group — the
    // collapsed commit must keep her authorship, not the last member's (Bob's).
    assert_eq!(head_commit.author().name().unwrap(), "Alice");
}

#[test]
fn start_rebase_rejects_a_plan_where_the_first_non_drop_entry_is_squash_or_fixup() {
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
        RebasePlanEntry {
            commit_id: commits[1].id.clone(),
            action: RebaseAction::Squash,
            combined_message: None,
        },
    ];

    let result = git_core::rebase::start_rebase(&repo, &onto, plan);

    assert!(result.is_err());
}

#[test]
fn a_drop_between_a_squash_groups_leader_and_member_does_not_panic() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "x.txt", "x\n");
    commit_all(&repo, "add x (to drop)");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![
        RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: RebaseAction::Pick,
            combined_message: Some("combined: a and b".to_string()),
        },
        RebasePlanEntry {
            commit_id: commits[1].id.clone(),
            action: RebaseAction::Drop,
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
        "combined: a and b"
    );
    assert!(dir.path().join("a.txt").exists());
    assert!(dir.path().join("b.txt").exists());
    assert!(!dir.path().join("x.txt").exists());
}

#[test]
fn start_rebase_rejects_starting_from_an_already_detached_head() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");

    let tip = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.set_head_detached(tip).unwrap();

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![pick(&commits[0].id)];

    let result = git_core::rebase::start_rebase(&repo, &onto, plan);

    assert!(result.is_err());
}

#[test]
fn abort_rebase_restores_the_original_branch_and_tip_exactly() {
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
    let (state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert_eq!(result, RebaseStepResult::Done);
    // Deliberately re-derive a fresh, still-in-progress-looking state isn't possible once
    // `start_rebase` already reached `Done` in one shot for a clean plan — abort a rebase that's
    // still genuinely paused instead, so this test exercises the real "abort mid-flight" case.
    let _ = state;

    // The tip to restore to is whatever the branch points at right *before* the rebase being
    // aborted starts — not the pre-everything tip captured above. `land_current_step` always
    // mints a fresh committer signature (see `pick_preserves_the_original_authors_identity_but_uses_a_fresh_committer`),
    // so the `pick` above already produced a new commit OID even though it replayed onto the
    // same parent; asserting against the original pre-rebase commit here would only pass when
    // that fresh committer timestamp happens to land in the same second as the original commit's,
    // which is a timing coincidence, not a guarantee (observed flaky on slower CI runners).
    let tip_before_second_rebase = repo.head().unwrap().peel_to_commit().unwrap().id();

    // Rebuild a genuinely paused rebase to abort: an Edit step, still open when we abort.
    write_file(dir.path(), "base.txt", "v1\n"); // no-op rewrite, just to get a clean starting point
    let commits_again = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![RebasePlanEntry {
        commit_id: commits_again[0].id.clone(),
        action: RebaseAction::Edit,
        combined_message: None,
    }];
    let (state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert_eq!(result, RebaseStepResult::PausedForEdit);

    git_core::rebase::abort_rebase(&repo, &state).unwrap();

    let head_ref = repo.head().unwrap();
    assert_eq!(head_ref.shorthand().unwrap(), branch_name);
    assert_eq!(
        head_ref.peel_to_commit().unwrap().id(),
        tip_before_second_rebase
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    let contents = std::fs::read_to_string(dir.path().join("a.txt")).unwrap();
    assert_eq!(contents, "a\n");
}

#[test]
fn abort_rebase_after_a_conflict_also_recovers_cleanly() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "shared.txt", "line one\nchanged on top\n");
    commit_all(&repo, "change on top of onto");
    write_file(dir.path(), "shared.txt", "line one\nchanged again\n");
    commit_all(&repo, "conflicting change");
    let branch_name = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    let original_tip = repo.head().unwrap().peel_to_commit().unwrap().id();

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![pick(&commits[1].id)];
    let (state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));

    git_core::rebase::abort_rebase(&repo, &state).unwrap();

    let head_ref = repo.head().unwrap();
    assert_eq!(head_ref.shorthand().unwrap(), branch_name);
    assert_eq!(head_ref.peel_to_commit().unwrap().id(), original_tip);
    assert!(!repo.index().unwrap().has_conflicts());
    let contents = std::fs::read_to_string(dir.path().join("shared.txt")).unwrap();
    assert_eq!(contents, "line one\nchanged again\n");
}

/// Creates a real merge commit on `HEAD`: branches off `HEAD`'s parent, commits there, then
/// commits a two-parent commit on `HEAD` carrying `HEAD`'s tree. Enough to exercise the
/// merge-commit guard — the merged content itself doesn't matter here.
fn make_merge_commit(repo: &git2::Repository) -> String {
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    let side_parent = head_commit.parent(0).unwrap();
    let signature = repo.signature().unwrap();
    let side_oid = repo
        .commit(
            None,
            &signature,
            &signature,
            "side branch commit",
            &side_parent.tree().unwrap(),
            &[&side_parent],
        )
        .unwrap();
    let side_commit = repo.find_commit(side_oid).unwrap();
    let merge_oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            "merge side branch",
            &head_commit.tree().unwrap(),
            &[&head_commit, &side_commit],
        )
        .unwrap();
    merge_oid.to_string()
}

#[test]
fn start_rebase_rejects_a_plan_containing_a_merge_commit_without_detaching() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    let merge_id = make_merge_commit(&repo);
    let branch_name = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    let original_tip = repo.head().unwrap().peel_to_commit().unwrap().id();

    let plan = vec![pick(&commit_id_at(&repo, 1)), pick(&merge_id)];

    let result = git_core::rebase::start_rebase(&repo, &onto, plan);

    assert!(matches!(result, Err(RebaseError::InvalidPlan(_))));
    // Rejected before any detach: the repo is untouched, still on its branch at its old tip —
    // `Repository::cherrypick` would otherwise have errored *after* the detach, stranding the
    // user on a detached HEAD with no RebaseState to abort with.
    assert!(!repo.head_detached().unwrap());
    let head_ref = repo.head().unwrap();
    assert_eq!(head_ref.shorthand().unwrap(), branch_name);
    assert_eq!(head_ref.peel_to_commit().unwrap().id(), original_tip);
}

#[test]
fn commits_since_does_not_walk_into_a_merged_side_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    make_merge_commit(&repo);

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();

    // First-parent only: "add a" and the merge itself, never the side branch's own commit.
    let summaries: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, vec!["add a", "merge side branch"]);
}

/// Commits `contents` at `path` on top of `parent` without moving any ref — a side commit to
/// build plans around that isn't on the current branch.
fn detached_commit_adding(
    repo: &git2::Repository,
    parent: &git2::Commit,
    path: &str,
    contents: &str,
    message: &str,
) -> String {
    let blob = repo.blob(contents.as_bytes()).unwrap();
    let mut builder = repo.treebuilder(Some(&parent.tree().unwrap())).unwrap();
    builder.insert(path, blob, 0o100644).unwrap();
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let signature = repo.signature().unwrap();
    repo.commit(None, &signature, &signature, message, &tree, &[parent])
        .unwrap()
        .to_string()
}

#[test]
fn start_rebase_rolls_back_to_the_original_branch_when_a_step_fails_after_detaching() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    let base_commit = repo.head().unwrap().peel_to_commit().unwrap();
    // A commit that adds `blocked.txt`, replayed while an untracked `blocked.txt` already sits
    // in the working tree: the cherry-pick's checkout refuses to clobber it, so `advance` fails
    // *after* `start_rebase` has already detached HEAD — exactly the shape of failure that used
    // to strand the repo detached with no `RebaseState` for the UI to abort with.
    let side_id = detached_commit_adding(
        &repo,
        &base_commit,
        "blocked.txt",
        "from the side commit\n",
        "add blocked.txt",
    );
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "blocked.txt", "untracked local content\n");

    let branch_name = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    let original_tip = repo.head().unwrap().peel_to_commit().unwrap().id();

    let plan = vec![pick(&side_id)];

    let result = git_core::rebase::start_rebase(&repo, &onto, plan);

    assert!(result.is_err());
    assert!(!repo.head_detached().unwrap());
    let head_ref = repo.head().unwrap();
    assert_eq!(head_ref.shorthand().unwrap(), branch_name);
    assert_eq!(head_ref.peel_to_commit().unwrap().id(), original_tip);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "a\n"
    );
}

#[test]
fn rebase_continue_past_the_end_of_the_plan_reports_not_rebasing_instead_of_panicking() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![RebasePlanEntry {
        commit_id: commits[0].id.clone(),
        action: RebaseAction::Edit,
        combined_message: None,
    }];
    let (mut state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert_eq!(result, RebaseStepResult::PausedForEdit);

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();
    assert_eq!(result, RebaseStepResult::Done);

    // A second Continue on the same, now-exhausted state (a double-click, or a caller holding a
    // state whose `finish` failed after the cursor ran off the end): an error, never an
    // index-out-of-bounds panic — that panic would kill the worker thread owning this repo.
    let result = git_core::rebase::rebase_continue(&repo, &mut state);

    assert!(matches!(result, Err(RebaseError::NotRebasing)));
}

#[test]
fn rebase_continue_refuses_when_head_moved_out_from_under_the_rebase() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");
    let elsewhere = repo.head().unwrap().peel_to_commit().unwrap().id();

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![RebasePlanEntry {
        commit_id: commits[0].id.clone(),
        action: RebaseAction::Edit,
        combined_message: None,
    }];
    let (mut state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert_eq!(result, RebaseStepResult::PausedForEdit);

    // Simulates something else (a branch switch in the UI, an external `git checkout`) moving
    // HEAD while the rebase sits paused.
    repo.set_head_detached(elsewhere).unwrap();

    let result = git_core::rebase::rebase_continue(&repo, &mut state);

    assert!(matches!(result, Err(RebaseError::HeadMoved { .. })));
    // And it refused *before* committing anything: HEAD is still where the interloper put it.
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        elsewhere
    );
}

#[test]
fn a_squash_group_without_an_explicit_combined_message_falls_back_to_the_leaders_message() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "leader message");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "last member message");

    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let plan = vec![
        RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: RebaseAction::Pick,
            combined_message: None,
        },
        RebasePlanEntry {
            commit_id: commits[1].id.clone(),
            action: RebaseAction::Squash,
            combined_message: None,
        },
    ];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    // The leader is the semantically-original commit for the whole group, so its message is the
    // fallback when no explicit `combined_message` was set — not the last member's.
    assert_eq!(
        head_commit.message().ok().unwrap_or_default(),
        "leader message"
    );
}

#[test]
fn current_step_is_one_indexed_at_the_first_pause() {
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
            action: RebaseAction::Edit,
            combined_message: None,
        },
        pick(&commits[1].id),
    ];

    let (state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();

    assert_eq!(result, RebaseStepResult::PausedForEdit);
    // Pausing on the first of two entries reads "Step 1 of 2", not "Step 0 of 2".
    assert_eq!(state.current_step(), 1);
    assert_eq!(state.total_steps(), 2);
}

/// A rebase paused on a `shared.txt` conflict; every commit also has `tc` = "plain\n" and
/// `d/f` = "inner\n".
#[cfg(unix)]
fn paused_rebase() -> (
    tempfile::TempDir,
    git2::Repository,
    git_core::rebase::RebaseState,
) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    write_file(dir.path(), "tc", "plain\n");
    write_file(dir.path(), "d/f", "inner\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "shared.txt", "line one\nchanged on top\n");
    commit_all(&repo, "change on top of onto");
    write_file(dir.path(), "shared.txt", "line one\nchanged again\n");
    commit_all(&repo, "conflicting change");
    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let (state, result) =
        git_core::rebase::start_rebase(&repo, &onto, vec![pick(&commits[1].id)]).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    (dir, repo, state)
}

#[test]
#[cfg(unix)]
fn abort_rebase_replaces_a_typechanged_symlink() {
    let out = common::outside_dir();
    let (dir, repo, state) = paused_rebase();
    common::replace_with_link(dir.path(), "tc", &out.path().join("target.txt"));

    git_core::rebase::abort_rebase(&repo, &state).unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "tc"));
    assert_eq!(common::read(dir.path(), "tc"), "plain\n");
}

#[test]
#[cfg(unix)]
fn abort_rebase_replaces_a_symlinked_directory() {
    let out = common::outside_dir();
    let (dir, repo, state) = paused_rebase();
    common::replace_with_link(dir.path(), "d", out.path());

    git_core::rebase::abort_rebase(&repo, &state).unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "d"));
    assert_eq!(common::read(dir.path(), "d/f"), "inner\n");
}

// --- Restart resilience (#108) --------------------------------------------------------------

use git_core::rebase::{RebaseState, RebaseStop};

fn edit(commit_id: &str) -> RebasePlanEntry {
    RebasePlanEntry {
        commit_id: commit_id.to_string(),
        action: RebaseAction::Edit,
        combined_message: None,
    }
}

fn head_id(repo: &git2::Repository) -> git2::Oid {
    repo.head().unwrap().peel_to_commit().unwrap().id()
}

/// `base` (onto) → "add a" (a.txt) → "add b" (b.txt) on the current branch. Returns
/// `(onto, [a, b], branch_ref)`.
fn two_commits_on_a_branch(
    dir: &std::path::Path,
    repo: &git2::Repository,
) -> (String, Vec<String>, String) {
    write_file(dir, "base.txt", "v1\n");
    commit_all(repo, "base commit");
    let onto = commit_id_at(repo, 0);
    write_file(dir, "a.txt", "a\n");
    commit_all(repo, "add a");
    write_file(dir, "b.txt", "b\n");
    commit_all(repo, "add b");
    let branch = repo.head().unwrap().name().unwrap().to_string();
    let ids = git_core::rebase::commits_since(repo, &onto)
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    (onto, ids, branch)
}

/// A rebase of one commit that conflicts with `onto` on `shared.txt`.
fn conflicting_rebase() -> (tempfile::TempDir, git2::Repository, RebaseState, String) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "shared.txt", "line one\nchanged on top\n");
    commit_all(&repo, "change on top of onto");
    write_file(dir.path(), "shared.txt", "line one\nchanged again\n");
    commit_all(&repo, "conflicting change");
    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();
    let (state, result) =
        git_core::rebase::start_rebase(&repo, &onto, vec![pick(&commits[1].id)]).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    (dir, repo, state, onto)
}

#[test]
fn a_conflict_pause_records_stop_conflict_onto_and_the_original_tip() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "shared.txt", "line one\nchanged on top\n");
    commit_all(&repo, "change on top of onto");
    write_file(dir.path(), "shared.txt", "line one\nchanged again\n");
    commit_all(&repo, "conflicting change");
    let branch = repo.head().unwrap().name().unwrap().to_string();
    let tip = head_id(&repo);
    let commits = git_core::rebase::commits_since(&repo, &onto).unwrap();

    let (state, result) =
        git_core::rebase::start_rebase(&repo, &onto, vec![pick(&commits[1].id)]).unwrap();

    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    assert_eq!(state.stop, RebaseStop::Conflict);
    assert_eq!(state.onto.to_string(), onto);
    assert_eq!(state.original_tip, tip);
    assert_eq!(state.original_branch_ref, branch);
    assert_eq!(state.expected_head, head_id(&repo));
    assert_eq!(state.cursor, 0);
    assert_eq!(state.group_start_parent, None);
}

#[test]
fn an_edit_pause_records_stop_edit() {
    let (dir, repo) = init_repo();
    let (onto, ids, _) = two_commits_on_a_branch(dir.path(), &repo);
    let (state, result) =
        git_core::rebase::start_rebase(&repo, &onto, vec![edit(&ids[0]), pick(&ids[1])]).unwrap();
    assert_eq!(result, RebaseStepResult::PausedForEdit);
    assert_eq!(state.stop, RebaseStop::Edit);
    // A clone compares equal: the state is plain data a store can rebuild.
    assert_eq!(state.clone(), state);
}

#[test]
fn progress_reports_the_paused_step_one_based() {
    let (dir, repo) = init_repo();
    let (onto, ids, _) = two_commits_on_a_branch(dir.path(), &repo);
    let (state, _) =
        git_core::rebase::start_rebase(&repo, &onto, vec![edit(&ids[0]), pick(&ids[1])]).unwrap();
    let progress = state.progress();
    assert_eq!((progress.current_step, progress.total_steps), (1, 2));
}

/// `base` (onto, f.txt = "base\n") → "a" (a.txt) → "b" (f.txt = "from b\n"). Returns
/// `(onto, [a, b])`.
fn a_then_b_changing_f(dir: &std::path::Path, repo: &git2::Repository) -> (String, Vec<String>) {
    write_file(dir, "f.txt", "base\n");
    commit_all(repo, "base commit");
    let onto = commit_id_at(repo, 0);
    write_file(dir, "a.txt", "a\n");
    commit_all(repo, "a");
    write_file(dir, "f.txt", "from b\n");
    commit_all(repo, "b");
    let ids = git_core::rebase::commits_since(repo, &onto)
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    (onto, ids)
}

/// `git cherry-pick --abort` in a terminal: forget the pick and reset to HEAD.
fn abort_pick_outside(repo: &git2::Repository) {
    repo.cleanup_state().unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.reset(head.as_object(), git2::ResetType::Hard, None)
        .unwrap();
}

#[test]
fn continue_after_an_error_stop_retries_the_step_instead_of_committing_it_empty() {
    let (dir, repo) = init_repo();
    let (onto, ids) = a_then_b_changing_f(dir.path(), &repo);
    let (mut state, result) =
        git_core::rebase::start_rebase(&repo, &onto, vec![edit(&ids[0]), pick(&ids[1])]).unwrap();
    assert_eq!(result, RebaseStepResult::PausedForEdit);

    // A local edit in the way of b: a lands, b's cherry-pick is refused.
    write_file(dir.path(), "f.txt", "local\n");
    assert!(git_core::rebase::rebase_continue(&repo, &mut state).is_err());
    assert_eq!(state.stop, RebaseStop::Error);
    assert_eq!(state.cursor, 1);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);

    write_file(dir.path(), "f.txt", "base\n");
    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.summary().unwrap(), Some("b"));
    assert_ne!(
        head.tree_id(),
        head.parent(0).unwrap().tree_id(),
        "b is empty"
    );
    assert_eq!(common::read(dir.path(), "f.txt"), "from b\n");
    assert_eq!(head.parent(0).unwrap().summary().unwrap(), Some("a"));
}

#[test]
fn continue_after_the_paused_pick_was_aborted_outside_reapplies_the_step() {
    let (_dir, repo, mut state, _) = conflicting_rebase();
    abort_pick_outside(&repo);

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    assert_eq!(state.cursor, 0);
    assert_eq!(state.stop, RebaseStop::Conflict);
}

#[test]
fn continue_refuses_while_another_cherry_pick_is_paused() {
    let (dir, repo) = init_repo();
    let (onto, ids, _) = two_commits_on_a_branch(dir.path(), &repo);
    let (mut state, _) =
        git_core::rebase::start_rebase(&repo, &onto, vec![edit(&ids[0]), pick(&ids[1])]).unwrap();
    let before = state.clone();
    abort_pick_outside(&repo);
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let unrelated = detached_commit_adding(&repo, &head, "z.txt", "z\n", "unrelated");
    repo.cherrypick(
        &repo
            .find_commit(git2::Oid::from_str(&unrelated).unwrap())
            .unwrap(),
        None,
    )
    .unwrap();

    let result = git_core::rebase::rebase_continue(&repo, &mut state);

    assert!(matches!(result, Err(RebaseError::OperationInProgress)));
    assert_eq!(
        result.unwrap_err().to_string(),
        "Another merge, cherry-pick or revert is in progress. Finish or abort it before \
         continuing the rebase."
    );
    assert_eq!(state, before);
    assert_eq!(head_id(&repo), head.id());
}

#[test]
fn continue_refuses_during_a_merge() {
    let (dir, repo) = init_repo();
    let (onto, ids, _) = two_commits_on_a_branch(dir.path(), &repo);
    let (mut state, _) =
        git_core::rebase::start_rebase(&repo, &onto, vec![edit(&ids[0]), pick(&ids[1])]).unwrap();
    repo.cleanup_state().unwrap();
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{}\n", ids[1])).unwrap();
    assert_eq!(repo.state(), git2::RepositoryState::Merge);

    let result = git_core::rebase::rebase_continue(&repo, &mut state);

    assert!(matches!(result, Err(RebaseError::OperationInProgress)));
    assert!(repo.path().join("MERGE_HEAD").exists());
}

/// `git commit` in a terminal during the pause: commits the index on HEAD with `author` and
/// ends git's paused pick.
fn commit_by_hand(repo: &git2::Repository, author: &git2::Signature, message: &str) -> git2::Oid {
    let mut index = repo.index().unwrap();
    index.read(false).unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let committer = repo.signature().unwrap();
    let id = repo
        .commit(Some("HEAD"), author, &committer, message, &tree, &[&head])
        .unwrap();
    repo.cleanup_state().unwrap();
    id
}

fn resolve_shared(repo: &git2::Repository) {
    let workdir = repo.workdir().unwrap();
    std::fs::write(workdir.join("shared.txt"), "line one\nresolved\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("shared.txt")).unwrap();
    index.write().unwrap();
}

fn author_of(repo: &git2::Repository, id: &str) -> git2::Signature<'static> {
    repo.find_commit(git2::Oid::from_str(id).unwrap())
        .unwrap()
        .author()
        .to_owned()
}

fn branch_tip(repo: &git2::Repository, branch_ref: &str) -> git2::Oid {
    repo.find_reference(branch_ref).unwrap().target().unwrap()
}

#[test]
fn continue_adopts_a_conflicted_step_committed_with_git() {
    let (_dir, repo, mut state, onto) = conflicting_rebase();
    let picked = state.plan[0].commit_id.clone();
    resolve_shared(&repo);
    let by_hand = commit_by_hand(&repo, &author_of(&repo, &picked), "conflicting change");

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let tip = branch_tip(&repo, &state.original_branch_ref);
    assert_eq!(
        tip, by_hand,
        "the hand-made commit is the step, not a duplicate"
    );
    let tip = repo.find_commit(tip).unwrap();
    assert_eq!(tip.parent_id(0).unwrap().to_string(), onto);
    assert!(!repo.head_detached().unwrap());
}

#[test]
fn continue_adopts_an_edit_step_committed_with_git() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = two_commits_on_a_branch(dir.path(), &repo);
    let (mut state, _) =
        git_core::rebase::start_rebase(&repo, &onto, vec![edit(&ids[0]), pick(&ids[1])]).unwrap();
    write_file(dir.path(), "a.txt", "amended\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();
    commit_by_hand(&repo, &author_of(&repo, &ids[0]), "add a");

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let summaries: Vec<String> = git_core::rebase::commits_since(&repo, &onto)
        .unwrap()
        .into_iter()
        .map(|c| c.summary)
        .collect();
    assert_eq!(summaries, ["add a", "add b"]);
    assert_eq!(repo.head().unwrap().name().unwrap(), branch);
    assert_eq!(common::read(dir.path(), "a.txt"), "amended\n");
}

#[test]
fn continue_gives_an_adopted_reword_step_its_planned_message() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "shared.txt", "line one\nchanged on top\n");
    commit_all(&repo, "change on top of onto");
    write_file(dir.path(), "shared.txt", "line one\nchanged again\n");
    commit_all(&repo, "conflicting change");
    let picked = commit_id_at(&repo, 0);
    let plan = vec![RebasePlanEntry {
        commit_id: picked.clone(),
        action: RebaseAction::Reword {
            message: "planned message".into(),
        },
        combined_message: None,
    }];
    let (mut state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    resolve_shared(&repo);
    // git keeps the picked message during the pause.
    commit_by_hand(&repo, &author_of(&repo, &picked), "conflicting change");

    assert_eq!(
        git_core::rebase::rebase_continue(&repo, &mut state).unwrap(),
        RebaseStepResult::Done
    );

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().unwrap(), "planned message");
    assert_eq!(head.parent_id(0).unwrap().to_string(), onto);
    assert_eq!(
        common::read(dir.path(), "shared.txt"),
        "line one\nresolved\n"
    );
}

#[test]
fn continue_adopts_a_squash_member_and_still_collapses_the_group() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "shared.txt", "line one\nchanged on top\n");
    commit_all(&repo, "change on top of onto");
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "leader");
    write_file(dir.path(), "shared.txt", "line one\nchanged again\n");
    commit_all(&repo, "member");
    let (leader, member) = (commit_id_at(&repo, 1), commit_id_at(&repo, 0));
    let plan = vec![
        RebasePlanEntry {
            commit_id: leader,
            action: RebaseAction::Pick,
            combined_message: Some("combined".into()),
        },
        RebasePlanEntry {
            commit_id: member.clone(),
            action: RebaseAction::Squash,
            combined_message: None,
        },
    ];
    let (mut state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    resolve_shared(&repo);
    commit_by_hand(&repo, &author_of(&repo, &member), "member");

    assert_eq!(
        git_core::rebase::rebase_continue(&repo, &mut state).unwrap(),
        RebaseStepResult::Done
    );

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().unwrap(), "combined");
    assert_eq!(head.parent_id(0).unwrap().to_string(), onto);
    assert_eq!(common::read(dir.path(), "a.txt"), "a\n");
    assert_eq!(
        common::read(dir.path(), "shared.txt"),
        "line one\nresolved\n"
    );
}

#[test]
fn continue_does_not_adopt_after_an_error_stop() {
    let (dir, repo) = init_repo();
    let (onto, ids) = a_then_b_changing_f(dir.path(), &repo);
    let (mut state, _) =
        git_core::rebase::start_rebase(&repo, &onto, vec![edit(&ids[0]), pick(&ids[1])]).unwrap();
    write_file(dir.path(), "f.txt", "local\n");
    assert!(git_core::rebase::rebase_continue(&repo, &mut state).is_err());
    assert_eq!(state.stop, RebaseStop::Error);
    // A commit with b's author on top of the stop, but b was never applied by the rebase.
    write_file(dir.path(), "f.txt", "from b\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("f.txt")).unwrap();
    index.write().unwrap();
    commit_by_hand(&repo, &author_of(&repo, &ids[1]), "b");

    let result = git_core::rebase::rebase_continue(&repo, &mut state);

    assert!(matches!(result, Err(RebaseError::HeadMoved { .. })));
}

#[test]
fn continue_does_not_adopt_a_commit_with_another_author() {
    let (_dir, repo, mut state, _) = conflicting_rebase();
    resolve_shared(&repo);
    let other = git2::Signature::now("Someone Else", "else@example.com").unwrap();
    commit_by_hand(&repo, &other, "conflicting change");
    let before = state.clone();

    let result = git_core::rebase::rebase_continue(&repo, &mut state);

    assert!(matches!(result, Err(RebaseError::HeadMoved { .. })));
    assert_eq!(state, before);
}

/// An Edit pause on `a` of `[Edit a, Pick b]` on a branch named `topic`.
fn edit_pause_on_topic() -> (tempfile::TempDir, git2::Repository, RebaseState, String) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("topic", &base, false).unwrap();
    }
    repo.set_head("refs/heads/topic").unwrap();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");
    let ids: Vec<String> = git_core::rebase::commits_since(&repo, &onto)
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    let (state, result) =
        git_core::rebase::start_rebase(&repo, &onto, vec![edit(&ids[0]), pick(&ids[1])]).unwrap();
    assert_eq!(result, RebaseStepResult::PausedForEdit);
    (dir, repo, state, onto)
}

#[test]
fn finish_refuses_when_the_branch_moved_during_the_pause() {
    let (_dir, repo, mut state, onto) = edit_pause_on_topic();
    let other = git2::Oid::from_str(&onto).unwrap();
    repo.reference("refs/heads/topic", other, true, "moved elsewhere")
        .unwrap();

    let result = git_core::rebase::rebase_continue(&repo, &mut state);

    assert!(matches!(result, Err(RebaseError::BranchMoved { .. })));
    assert_eq!(branch_tip(&repo, "refs/heads/topic"), other);
    assert!(repo.head_detached().unwrap());
    assert_eq!(state.cursor, state.plan.len());
    assert_eq!(state.stop, RebaseStop::Error);
}

#[test]
fn continue_retries_finish_after_the_branch_is_put_back() {
    let (_dir, repo, mut state, onto) = edit_pause_on_topic();
    let other = git2::Oid::from_str(&onto).unwrap();
    repo.reference("refs/heads/topic", other, true, "moved elsewhere")
        .unwrap();
    assert!(git_core::rebase::rebase_continue(&repo, &mut state).is_err());
    repo.reference("refs/heads/topic", state.original_tip, true, "put back")
        .unwrap();

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(repo.head().unwrap().name().unwrap(), "refs/heads/topic");
    assert_eq!(branch_tip(&repo, "refs/heads/topic"), state.expected_head);
    let summaries: Vec<String> = git_core::rebase::commits_since(&repo, &onto)
        .unwrap()
        .into_iter()
        .map(|c| c.summary)
        .collect();
    assert_eq!(summaries, ["add a", "add b"]);
}

#[test]
fn finish_refuses_when_the_branch_was_deleted() {
    let (_dir, repo, mut state, _) = edit_pause_on_topic();
    repo.find_reference("refs/heads/topic")
        .unwrap()
        .delete()
        .unwrap();

    let result = git_core::rebase::rebase_continue(&repo, &mut state);

    assert!(matches!(result, Err(RebaseError::BranchMissing { .. })));
    assert!(repo.head_detached().unwrap());
    assert_eq!(state.cursor, state.plan.len());
}

#[test]
fn the_branch_errors_name_the_short_branch() {
    assert_eq!(
        RebaseError::BranchMoved {
            branch: "topic".into()
        }
        .to_string(),
        "topic moved since the rebase started. Abort the rebase and start again."
    );
    assert_eq!(
        RebaseError::BranchMissing {
            branch: "feature/x".into()
        }
        .to_string(),
        "feature/x no longer exists. Abort the rebase."
    );
}

use git_core::rebase::{rebase_health, RebaseDivergence, RebaseHealth};

fn diverged(reason: RebaseDivergence) -> RebaseHealth {
    RebaseHealth::Diverged(reason)
}

/// `git switch <name>` (created at `at`) in a terminal.
fn switch_outside(repo: &git2::Repository, name: &str, at: git2::Oid) {
    let commit = repo.find_commit(at).unwrap();
    repo.branch(name, &commit, true).unwrap();
    repo.set_head(&format!("refs/heads/{name}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
}

#[test]
fn health_is_paused_at_a_conflict_an_edit_and_an_error_stop() {
    let (_dir, repo, state, _) = conflicting_rebase();
    assert_eq!(rebase_health(&repo, &state).unwrap(), RebaseHealth::Paused);

    let (_dir, repo, state, _) = edit_pause_on_topic();
    assert_eq!(rebase_health(&repo, &state).unwrap(), RebaseHealth::Paused);

    let (dir, repo) = init_repo();
    let (onto, ids) = a_then_b_changing_f(dir.path(), &repo);
    let (mut state, _) =
        git_core::rebase::start_rebase(&repo, &onto, vec![edit(&ids[0]), pick(&ids[1])]).unwrap();
    write_file(dir.path(), "f.txt", "local\n");
    assert!(git_core::rebase::rebase_continue(&repo, &mut state).is_err());
    assert_eq!(rebase_health(&repo, &state).unwrap(), RebaseHealth::Paused);
}

#[test]
fn health_is_head_moved_after_switching_to_a_branch_outside() {
    let (_dir, repo, state, onto) = edit_pause_on_topic();
    switch_outside(&repo, "elsewhere", git2::Oid::from_str(&onto).unwrap());
    assert_eq!(
        rebase_health(&repo, &state).unwrap(),
        diverged(RebaseDivergence::HeadMoved)
    );
}

#[test]
fn health_is_head_moved_after_a_commit_that_is_not_the_step() {
    let (_dir, repo, state, _) = conflicting_rebase();
    resolve_shared(&repo);
    let other = git2::Signature::now("Someone Else", "else@example.com").unwrap();
    commit_by_hand(&repo, &other, "not the step");
    assert_eq!(
        rebase_health(&repo, &state).unwrap(),
        diverged(RebaseDivergence::HeadMoved)
    );
}

#[test]
fn health_is_paused_when_the_step_was_committed_by_hand() {
    let (_dir, repo, state, _) = conflicting_rebase();
    resolve_shared(&repo);
    commit_by_hand(
        &repo,
        &author_of(&repo, &state.plan[0].commit_id),
        "conflicting change",
    );
    assert_eq!(rebase_health(&repo, &state).unwrap(), RebaseHealth::Paused);
}

#[test]
fn health_is_branch_moved_and_branch_missing() {
    let (_dir, repo, state, onto) = edit_pause_on_topic();
    repo.reference(
        "refs/heads/topic",
        git2::Oid::from_str(&onto).unwrap(),
        true,
        "moved",
    )
    .unwrap();
    assert_eq!(
        rebase_health(&repo, &state).unwrap(),
        diverged(RebaseDivergence::BranchMoved)
    );
    repo.find_reference("refs/heads/topic")
        .unwrap()
        .delete()
        .unwrap();
    assert_eq!(
        rebase_health(&repo, &state).unwrap(),
        diverged(RebaseDivergence::BranchMissing)
    );
}

#[test]
fn health_is_other_operation_for_a_foreign_cherry_pick_and_a_merge() {
    let (_dir, repo, state, _) = edit_pause_on_topic();
    abort_pick_outside(&repo);
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let unrelated = detached_commit_adding(&repo, &head, "z.txt", "z\n", "unrelated");
    repo.cherrypick(
        &repo
            .find_commit(git2::Oid::from_str(&unrelated).unwrap())
            .unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(
        rebase_health(&repo, &state).unwrap(),
        diverged(RebaseDivergence::OtherOperation)
    );

    abort_pick_outside(&repo);
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{unrelated}\n")).unwrap();
    assert_eq!(
        rebase_health(&repo, &state).unwrap(),
        diverged(RebaseDivergence::OtherOperation)
    );
}

use git_core::rebase::RebaseAbort;

#[test]
fn abort_restores_the_branch_when_head_is_where_the_rebase_left_it() {
    let (_dir, repo, state, _) = conflicting_rebase();
    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();
    assert_eq!(abort, RebaseAbort { restored: true });
    assert_eq!(
        repo.head().unwrap().name().unwrap(),
        state.original_branch_ref
    );
    assert_eq!(head_id(&repo), state.original_tip);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn abort_after_an_error_stop_restores_the_branch() {
    let (dir, repo) = init_repo();
    let (onto, ids) = a_then_b_changing_f(dir.path(), &repo);
    let (mut state, _) =
        git_core::rebase::start_rebase(&repo, &onto, vec![edit(&ids[0]), pick(&ids[1])]).unwrap();
    write_file(dir.path(), "f.txt", "local\n");
    assert!(git_core::rebase::rebase_continue(&repo, &mut state).is_err());

    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert_eq!(abort, RebaseAbort { restored: true });
    assert!(!repo.head_detached().unwrap());
    assert_eq!(head_id(&repo), state.original_tip);
}

#[test]
fn abort_after_head_moved_cleans_up_only_its_own_step_and_leaves_head() {
    let (dir, repo, state, onto) = edit_pause_on_topic();
    switch_outside(&repo, "elsewhere", git2::Oid::from_str(&onto).unwrap());
    assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
    write_file(dir.path(), "base.txt", "my unrelated edit\n");

    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert_eq!(abort, RebaseAbort { restored: false });
    assert_eq!(repo.head().unwrap().name().unwrap(), "refs/heads/elsewhere");
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert_eq!(common::read(dir.path(), "base.txt"), "my unrelated edit\n");
    assert_eq!(branch_tip(&repo, "refs/heads/topic"), state.original_tip);
}

#[test]
fn abort_with_the_branch_deleted_leaves_head_detached() {
    let (dir, repo, state, _) = edit_pause_on_topic();
    repo.find_reference("refs/heads/topic")
        .unwrap()
        .delete()
        .unwrap();

    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert_eq!(abort, RebaseAbort { restored: false });
    assert!(repo.head_detached().unwrap());
    assert_eq!(head_id(&repo), state.expected_head);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    // The paused step's change is gone from the index and work tree, like `--abort`.
    assert!(!dir.path().join("a.txt").exists());
}

/// D3: a branch moved outside is a divergence, so Abort leaves HEAD (and an uncommitted edit
/// made during the pause) alone instead of force-checking-out the moved branch.
#[test]
fn abort_with_the_branch_moved_leaves_head_and_work_alone() {
    let (dir, repo, state, onto) = edit_pause_on_topic();
    let moved = git2::Oid::from_str(&onto).unwrap();
    repo.reference("refs/heads/topic", moved, true, "moved")
        .unwrap();
    write_file(dir.path(), "base.txt", "edit made during the pause\n");

    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert_eq!(abort, RebaseAbort { restored: false });
    assert!(repo.head_detached().unwrap());
    assert_eq!(head_id(&repo), state.expected_head);
    assert_eq!(branch_tip(&repo, "refs/heads/topic"), moved);
    assert_eq!(
        common::read(dir.path(), "base.txt"),
        "edit made during the pause\n"
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn abort_after_head_moved_keeps_work_staged_on_the_other_branch() {
    let (dir, repo, state, onto) = edit_pause_on_topic();
    switch_outside(&repo, "elsewhere", git2::Oid::from_str(&onto).unwrap());
    write_file(dir.path(), "staged.txt", "staged on elsewhere\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("staged.txt")).unwrap();
    index.write().unwrap();

    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert_eq!(abort, RebaseAbort { restored: false });
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    assert!(index
        .get_path(std::path::Path::new("staged.txt"), 0)
        .is_some());
    assert_eq!(
        common::read(dir.path(), "staged.txt"),
        "staged on elsewhere\n"
    );
}

/// A Reword step committed by hand whose re-commit with the planned message fails (here an
/// unusable signing config) must stay adoptable once the problem is fixed.
#[test]
fn a_failed_reword_of_an_adopted_step_can_be_retried() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    write_file(dir.path(), "shared.txt", "line one\nchanged on top\n");
    commit_all(&repo, "change on top of onto");
    write_file(dir.path(), "shared.txt", "line one\nchanged again\n");
    commit_all(&repo, "conflicting change");
    let picked = commit_id_at(&repo, 0);
    let plan = vec![RebasePlanEntry {
        commit_id: picked.clone(),
        action: RebaseAction::Reword {
            message: "planned message".into(),
        },
        combined_message: None,
    }];
    let (mut state, _) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    resolve_shared(&repo);
    // git keeps the picked message during the pause.
    commit_by_hand(&repo, &author_of(&repo, &picked), "conflicting change");
    {
        let mut config = repo.config().unwrap();
        config.set_bool("commit.gpgSign", true).unwrap();
        config.set_str("gpg.format", "no-such-format").unwrap();
    }

    assert!(git_core::rebase::rebase_continue(&repo, &mut state).is_err());
    assert_eq!(rebase_health(&repo, &state).unwrap(), RebaseHealth::Paused);

    repo.config()
        .unwrap()
        .set_bool("commit.gpgSign", false)
        .unwrap();
    assert_eq!(
        git_core::rebase::rebase_continue(&repo, &mut state).unwrap(),
        RebaseStepResult::Done
    );
    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .message()
            .unwrap(),
        "planned message"
    );
}

#[test]
fn abort_leaves_a_foreign_operation_alone() {
    let (_dir, repo, state, onto) = edit_pause_on_topic();
    abort_pick_outside(&repo);
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{onto}\n")).unwrap();

    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert_eq!(abort, RebaseAbort { restored: false });
    assert!(repo.path().join("MERGE_HEAD").exists());
    assert!(repo.head_detached().unwrap());
}

/// Commits `path` = `contents` on HEAD with a fixed author (same name, email and time for every
/// call), so two commits are indistinguishable by author alone.
fn commit_with_fixed_author(
    repo: &git2::Repository,
    path: &str,
    contents: &str,
    message: &str,
) -> String {
    let workdir = repo.workdir().unwrap().to_path_buf();
    write_file(&workdir, path, contents);
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(path)).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    let author = git2::Signature::new(
        "Same",
        "same@example.com",
        &git2::Time::new(1_700_000_000, 0),
    )
    .unwrap();
    repo.commit(Some("HEAD"), &author, &author, message, &tree, &[&parent])
        .unwrap()
        .to_string()
}

/// Verifier repro: the edit stop's pick is aborted outside and *another* plan commit with the
/// same author and time is committed instead. That is not the step: Continue must refuse, not
/// adopt it and drop a.txt.
#[test]
fn continue_does_not_adopt_another_commit_with_the_same_author() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = commit_id_at(&repo, 0);
    let a = commit_with_fixed_author(&repo, "a.txt", "a\n", "a");
    let b = commit_with_fixed_author(&repo, "b.txt", "b\n", "b");
    let (mut state, _) =
        git_core::rebase::start_rebase(&repo, &onto, vec![edit(&a), pick(&b)]).unwrap();
    abort_pick_outside(&repo);
    repo.cherrypick(
        &repo.find_commit(git2::Oid::from_str(&b).unwrap()).unwrap(),
        None,
    )
    .unwrap();
    commit_by_hand(&repo, &author_of(&repo, &b), "b");

    let result = git_core::rebase::rebase_continue(&repo, &mut state);

    assert!(
        matches!(result, Err(RebaseError::HeadMoved { .. })),
        "{result:?}"
    );
    assert_ne!(rebase_health(&repo, &state).unwrap(), RebaseHealth::Paused);
}

#[test]
fn continue_does_not_adopt_the_step_under_another_message() {
    let (_dir, repo, mut state, _) = conflicting_rebase();
    resolve_shared(&repo);
    commit_by_hand(
        &repo,
        &author_of(&repo, &state.plan[0].commit_id),
        "something else",
    );

    let result = git_core::rebase::rebase_continue(&repo, &mut state);

    assert!(matches!(result, Err(RebaseError::HeadMoved { .. })));
}

#[test]
fn continue_adopts_a_conflict_resolved_to_ours() {
    // Conflicted paths may be resolved any way, including back to HEAD's side.
    let (dir, repo, mut state, onto) = conflicting_rebase();
    let ours = repo
        .find_commit(git2::Oid::from_str(&onto).unwrap())
        .unwrap();
    let ours = ours
        .tree()
        .unwrap()
        .get_path(std::path::Path::new("shared.txt"))
        .unwrap();
    let blob = repo.find_blob(ours.id()).unwrap();
    std::fs::write(dir.path().join("shared.txt"), blob.content()).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("shared.txt")).unwrap();
    index.write().unwrap();
    commit_by_hand(
        &repo,
        &author_of(&repo, &state.plan[0].commit_id),
        "conflicting change",
    );

    assert_eq!(
        git_core::rebase::rebase_continue(&repo, &mut state).unwrap(),
        RebaseStepResult::Done
    );
}

/// Health says Paused for a step committed by hand, so Abort must treat it like a pause too:
/// back to the branch, not "HEAD had moved".
#[test]
fn abort_restores_the_branch_when_the_step_was_committed_by_hand() {
    let (_dir, repo, state, _) = conflicting_rebase();
    resolve_shared(&repo);
    commit_by_hand(
        &repo,
        &author_of(&repo, &state.plan[0].commit_id),
        "conflicting change",
    );
    assert_eq!(rebase_health(&repo, &state).unwrap(), RebaseHealth::Paused);

    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert_eq!(abort, RebaseAbort { restored: true });
    assert_eq!(
        repo.head().unwrap().name().unwrap(),
        state.original_branch_ref
    );
    assert_eq!(head_id(&repo), state.original_tip);
}

/// #170: the branch is `base(f.txt, shared.txt) → g(g.txt)`; `onto` is `base → h(h.txt)`.
/// Returns `(dir, repo, onto, g)`.
fn local_changes_fixture() -> (tempfile::TempDir, git2::Repository, String, String) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "f0\n");
    write_file(dir.path(), "shared.txt", "s0\n");
    commit_all(&repo, "base");
    let onto = {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        let mut tree = repo.treebuilder(Some(&base.tree().unwrap())).unwrap();
        tree.insert("h.txt", repo.blob(b"h\n").unwrap(), 0o100644)
            .unwrap();
        let tree = repo.find_tree(tree.write().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(None, &sig, &sig, "h", &tree, &[&base])
            .unwrap()
            .to_string()
    };
    write_file(dir.path(), "g.txt", "g\n");
    commit_all(&repo, "g");
    let g = commit_id_at(&repo, 0);
    (dir, repo, onto, g)
}

fn assert_nothing_started(repo: &git2::Repository, branch: &str, tip: &str) {
    assert!(!repo.head_detached().unwrap());
    assert_eq!(repo.head().unwrap().name().unwrap(), branch);
    assert_eq!(commit_id_at(repo, 0), tip);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn start_refuses_an_unstaged_change_and_changes_nothing() {
    let (dir, repo, onto, g) = local_changes_fixture();
    let branch = repo.head().unwrap().name().unwrap().to_string();
    write_file(dir.path(), "f.txt", "DIRTY\n");
    let index_before = repo.index().unwrap().write_tree().unwrap();

    let Err(error) = git_core::rebase::start_rebase(&repo, &onto, vec![pick(&g)]) else {
        panic!("rebase started");
    };
    match &error {
        RebaseError::LocalChanges { paths } => assert_eq!(paths, &["f.txt"]),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        error.to_string(),
        "You have uncommitted changes to f.txt. Commit or stash them before starting a rebase."
    );
    assert_nothing_started(&repo, &branch, &g);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "DIRTY\n"
    );
    assert_eq!(repo.index().unwrap().write_tree().unwrap(), index_before);
}

#[test]
fn start_refuses_a_staged_change() {
    let (dir, repo, onto, g) = local_changes_fixture();
    let branch = repo.head().unwrap().name().unwrap().to_string();
    write_file(dir.path(), "f.txt", "STAGED\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("f.txt")).unwrap();
    index.write().unwrap();

    let result = git_core::rebase::start_rebase(&repo, &onto, vec![pick(&g)]);
    assert!(
        matches!(&result, Err(RebaseError::LocalChanges { paths }) if paths == &["f.txt"]),
        "{result:?}"
    );
    assert_nothing_started(&repo, &branch, &g);
    let status = repo.status_file(std::path::Path::new("f.txt")).unwrap();
    assert!(status.contains(git2::Status::INDEX_MODIFIED));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "STAGED\n"
    );
}

#[test]
fn start_refuses_a_deleted_tracked_file() {
    let (dir, repo, onto, g) = local_changes_fixture();
    let branch = repo.head().unwrap().name().unwrap().to_string();
    std::fs::remove_file(dir.path().join("shared.txt")).unwrap();

    let result = git_core::rebase::start_rebase(&repo, &onto, vec![pick(&g)]);
    assert!(
        matches!(&result, Err(RebaseError::LocalChanges { paths }) if paths == &["shared.txt"]),
        "{result:?}"
    );
    assert_nothing_started(&repo, &branch, &g);
    assert!(!dir.path().join("shared.txt").exists());
}

#[test]
fn start_allows_untracked_files() {
    let (dir, repo, onto, g) = local_changes_fixture();
    write_file(dir.path(), "notes.txt", "mine\n");
    let (_, result) = git_core::rebase::start_rebase(&repo, &onto, vec![pick(&g)]).unwrap();
    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes.txt")).unwrap(),
        "mine\n"
    );
    assert!(dir.path().join("h.txt").exists());
}

#[test]
fn start_ignores_ignored_files() {
    let (dir, repo, onto, g) = local_changes_fixture();
    // An exclude file, so the ignore rule itself is no tracked change.
    write_file(&repo.path().join("info"), "exclude", "*.log\n");
    write_file(dir.path(), "build.log", "noise\n");
    let (_, result) = git_core::rebase::start_rebase(&repo, &onto, vec![pick(&g)]).unwrap();
    assert_eq!(result, RebaseStepResult::Done);
    assert!(dir.path().join("build.log").exists());
}

#[test]
fn local_changes_copy_names_the_first_path_and_counts_the_rest() {
    let message = |paths: &[&str]| {
        RebaseError::LocalChanges {
            paths: paths.iter().map(|p| p.to_string()).collect(),
        }
        .to_string()
    };
    assert_eq!(
        message(&["a.txt"]),
        "You have uncommitted changes to a.txt. Commit or stash them before starting a rebase."
    );
    assert_eq!(
        message(&["a.txt", "b.txt"]),
        "You have uncommitted changes to a.txt and 1 other file. Commit or stash them before \
         starting a rebase."
    );
    assert_eq!(
        message(&["a.txt", "b.txt", "c.txt"]),
        "You have uncommitted changes to a.txt and 2 other files. Commit or stash them before \
         starting a rebase."
    );
}

#[test]
fn start_lists_every_changed_path_sorted() {
    let (dir, repo, onto, g) = local_changes_fixture();
    write_file(dir.path(), "shared.txt", "DIRTY\n");
    write_file(dir.path(), "f.txt", "DIRTY\n");
    let result = git_core::rebase::start_rebase(&repo, &onto, vec![pick(&g)]);
    assert!(
        matches!(&result, Err(RebaseError::LocalChanges { paths }) if paths == &["f.txt", "shared.txt"]),
        "{result:?}"
    );
}

#[test]
#[cfg(unix)]
fn a_failed_first_step_on_a_clean_tree_rolls_back_to_the_branch() {
    let (dir, repo, onto, g) = local_changes_fixture();
    let branch = repo.head().unwrap().name().unwrap().to_string();
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.program", "/bin/false").unwrap();

    let result = git_core::rebase::start_rebase(&repo, &onto, vec![pick(&g)]);
    assert!(result.is_err());
    assert_nothing_started(&repo, &branch, &g);
    assert!(!repo.path().join("CHERRY_PICK_HEAD").exists());
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true);
    assert!(repo.statuses(Some(&mut options)).unwrap().is_empty());
    assert!(!dir.path().join("h.txt").exists());
}
