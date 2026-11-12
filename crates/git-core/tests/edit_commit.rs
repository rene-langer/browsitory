//! Edit commit… (#882): a rebase that stops at an older commit with that commit's changes
//! staged on its parent (like `git reset --soft HEAD~1` at an edit stop), the message kept for
//! the recommit, which Continue may replace.
mod common;

use common::{commit_all, init_repo, write_file};
use git_core::hooks::HookOptions;
use git_core::rebase::{
    RebaseAction, RebaseError, RebaseHealth, RebasePlanEntry, RebaseState, RebaseStepResult,
};
use std::path::Path;

fn entry(commit_id: &str, action: RebaseAction) -> RebasePlanEntry {
    RebasePlanEntry {
        commit_id: commit_id.to_string(),
        action,
        combined_message: None,
    }
}

/// `base` (onto; f.txt = "base\n") → "add a\n\nbody\n" (a.txt = "a\n") → "add b" (b.txt) on the
/// checked-out branch. Returns `(onto, [a, b], branch ref)`.
fn base_a_b(dir: &Path, repo: &git2::Repository) -> (String, Vec<String>, String) {
    write_file(dir, "f.txt", "base\n");
    commit_all(repo, "base");
    let onto = common::head_id(repo).to_string();
    write_file(dir, "a.txt", "a\n");
    commit_all(repo, "add a\n\nbody\n");
    let a = common::head_id(repo).to_string();
    write_file(dir, "b.txt", "b\n");
    commit_all(repo, "add b");
    let b = common::head_id(repo).to_string();
    let branch = repo.head().unwrap().name().unwrap().to_string();
    (onto, vec![a, b], branch)
}

/// Starts Edit commit… on `ids[0]`: the plan the commit menu builds, Edit then Pick the rest.
fn edit_first(repo: &git2::Repository, onto: &str, ids: &[String]) -> RebaseState {
    let mut plan = vec![entry(&ids[0], RebaseAction::Edit)];
    plan.extend(ids[1..].iter().map(|id| entry(id, RebaseAction::Pick)));
    let (state, result) = git_core::rebase::start_rebase(repo, onto, plan).unwrap();
    assert_eq!(result, RebaseStepResult::PausedForEdit);
    state
}

fn stage(repo: &git2::Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.read(false).unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

fn continue_with(
    repo: &git2::Repository,
    state: &mut RebaseState,
    message: Option<&str>,
) -> Result<RebaseStepResult, RebaseError> {
    git_core::rebase::rebase_continue_with_message(repo, state, message, HookOptions::default())
}

/// The branch's commits after `onto`, oldest first, as (message, tree of (path, content)).
fn history(
    repo: &git2::Repository,
    branch: &str,
    onto: &str,
) -> Vec<(String, Vec<(String, String)>)> {
    let mut walk = repo.revwalk().unwrap();
    walk.push(repo.find_reference(branch).unwrap().target().unwrap())
        .unwrap();
    walk.hide(git2::Oid::from_str(onto).unwrap()).unwrap();
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)
        .unwrap();
    walk.map(|id| {
        let commit = repo.find_commit(id.unwrap()).unwrap();
        let tree = commit.tree().unwrap();
        let mut files = Vec::new();
        tree.walk(git2::TreeWalkMode::PreOrder, |root, item| {
            if item.kind() == Some(git2::ObjectType::Blob) {
                let blob = repo.find_blob(item.id()).unwrap();
                files.push((
                    format!("{root}{}", item.name().unwrap()),
                    String::from_utf8_lossy(blob.content()).into_owned(),
                ));
            }
            git2::TreeWalkResult::Ok
        })
        .unwrap();
        (commit.message().unwrap().to_string(), files)
    })
    .collect()
}

fn files(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(path, content)| (path.to_string(), content.to_string()))
        .collect()
}

#[test]
fn the_edit_stop_has_the_commits_changes_staged_on_its_parent() {
    let (dir, repo) = init_repo();
    let (onto, ids, _) = base_a_b(dir.path(), &repo);

    let state = edit_first(&repo, &onto, &ids);

    // HEAD is the commit's parent, detached; the commit's change is staged, nothing unstaged.
    assert!(repo.head_detached().unwrap());
    assert_eq!(common::head_id(&repo).to_string(), onto);
    let statuses = repo.statuses(None).unwrap();
    let changes: Vec<(String, git2::Status)> = statuses
        .iter()
        .map(|e| (e.path().unwrap().to_string(), e.status()))
        .collect();
    assert_eq!(changes, [("a.txt".to_string(), git2::Status::INDEX_NEW)]);
    assert_eq!(common::read(dir.path(), "a.txt"), "a\n");
    assert!(!dir.path().join("b.txt").exists());
    // The UI names the commit being edited.
    let info = git_core::repo::RebaseInfo::new(&state, RebaseHealth::Paused);
    assert_eq!(info.editing.as_deref(), Some(ids[0].as_str()));
    assert_eq!((info.current_step, info.total_steps), (1, 2));
}

#[test]
fn a_conflict_stop_names_no_commit_being_edited() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "base\n");
    commit_all(&repo, "base");
    let onto = common::head_id(&repo).to_string();
    write_file(dir.path(), "f.txt", "one\n");
    commit_all(&repo, "one");
    write_file(dir.path(), "f.txt", "two\n");
    commit_all(&repo, "two");
    let two = common::head_id(&repo).to_string();
    let (state, result) =
        git_core::rebase::start_rebase(&repo, &onto, vec![entry(&two, RebaseAction::Pick)])
            .unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));

    let info = git_core::repo::RebaseInfo::new(&state, RebaseHealth::Paused);
    assert_eq!(info.editing, None);
}

#[test]
fn continue_recommits_the_changed_files_with_a_new_message_and_replays_the_rest() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_a_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    write_file(dir.path(), "a.txt", "a, edited\n");
    write_file(dir.path(), "new.txt", "new\n");
    stage(&repo, "a.txt");
    stage(&repo, "new.txt");

    let result = continue_with(&repo, &mut state, Some("add a, edited\n")).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(repo.head().unwrap().name().unwrap(), branch);
    assert_eq!(
        history(&repo, &branch, &onto),
        [
            (
                "add a, edited\n".to_string(),
                files(&[
                    ("a.txt", "a, edited\n"),
                    ("f.txt", "base\n"),
                    ("new.txt", "new\n")
                ])
            ),
            (
                "add b".to_string(),
                files(&[
                    ("a.txt", "a, edited\n"),
                    ("b.txt", "b\n"),
                    ("f.txt", "base\n"),
                    ("new.txt", "new\n")
                ])
            ),
        ]
    );
    // The recommitted commit keeps its author.
    let tip = repo
        .find_reference(&branch)
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let edited = tip.parent(0).unwrap();
    let original = repo
        .find_commit(git2::Oid::from_str(&ids[0]).unwrap())
        .unwrap();
    assert_eq!(edited.author().name(), original.author().name());
    assert_eq!(edited.author().when(), original.author().when());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn continue_without_a_message_keeps_the_commits_full_message() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_a_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    write_file(dir.path(), "a.txt", "a, edited\n");
    stage(&repo, "a.txt");

    let result = continue_with(&repo, &mut state, None).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let messages: Vec<String> = history(&repo, &branch, &onto)
        .into_iter()
        .map(|(message, _)| message)
        .collect();
    assert_eq!(messages, ["add a\n\nbody\n", "add b"]);
}

#[test]
fn continue_refuses_an_empty_message_and_stays_paused() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_a_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    let before = state.clone();

    let error = continue_with(&repo, &mut state, Some("  \n")).unwrap_err();

    assert!(matches!(error, RebaseError::EmptyMessage), "{error:?}");
    assert_eq!(state, before);
    assert_eq!(common::head_id(&repo).to_string(), onto);
    assert_eq!(
        git_core::rebase::rebase_health(&repo, &state).unwrap(),
        RebaseHealth::Paused
    );
    // A real message then goes on.
    assert_eq!(
        continue_with(&repo, &mut state, Some("add a\n")).unwrap(),
        RebaseStepResult::Done
    );
    assert_eq!(history(&repo, &branch, &onto)[0].0, "add a\n".to_string());
}

#[test]
fn abort_after_changing_files_puts_the_branch_back() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_a_b(dir.path(), &repo);
    let tip = repo.find_reference(&branch).unwrap().target().unwrap();
    let state = edit_first(&repo, &onto, &ids);
    write_file(dir.path(), "a.txt", "a, edited\n");
    stage(&repo, "a.txt");
    write_file(dir.path(), "f.txt", "unstaged\n");

    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert!(abort.restored);
    assert_eq!(repo.head().unwrap().name().unwrap(), branch);
    assert_eq!(common::head_id(&repo), tip);
    assert_eq!(common::read(dir.path(), "a.txt"), "a\n");
    assert_eq!(common::read(dir.path(), "f.txt"), "base\n");
    assert_eq!(common::read(dir.path(), "b.txt"), "b\n");
    assert!(repo.statuses(None).unwrap().is_empty());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn a_later_step_that_conflicts_with_the_edit_pauses_and_then_finishes() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "base\n");
    commit_all(&repo, "base");
    let onto = common::head_id(&repo).to_string();
    write_file(dir.path(), "f.txt", "one\n");
    commit_all(&repo, "one");
    let one = common::head_id(&repo).to_string();
    write_file(dir.path(), "f.txt", "two\n");
    commit_all(&repo, "two");
    let two = common::head_id(&repo).to_string();
    let branch = repo.head().unwrap().name().unwrap().to_string();
    let mut state = edit_first(&repo, &onto, &[one.clone(), two.clone()]);
    write_file(dir.path(), "f.txt", "one, edited\n");
    stage(&repo, "f.txt");

    let result = continue_with(&repo, &mut state, Some("one, edited\n")).unwrap();

    assert_eq!(
        result,
        RebaseStepResult::Conflicted {
            files: vec!["f.txt".to_string()]
        }
    );
    let info = git_core::repo::RebaseInfo::new(&state, RebaseHealth::Paused);
    assert_eq!(
        info.editing, None,
        "the stop is a conflict now, not the edit"
    );
    assert_eq!((info.current_step, info.total_steps), (2, 2));
    write_file(dir.path(), "f.txt", "two\n");
    stage(&repo, "f.txt");
    assert_eq!(
        continue_with(&repo, &mut state, None).unwrap(),
        RebaseStepResult::Done
    );
    assert_eq!(
        history(&repo, &branch, &onto),
        [
            (
                "one, edited\n".to_string(),
                files(&[("f.txt", "one, edited\n")])
            ),
            ("two".to_string(), files(&[("f.txt", "two\n")])),
        ]
    );
}

#[test]
fn a_paused_edit_rebuilt_after_a_restart_continues_with_the_new_message() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_a_b(dir.path(), &repo);
    let state = edit_first(&repo, &onto, &ids);
    write_file(dir.path(), "a.txt", "a, edited\n");
    stage(&repo, "a.txt");
    // A restart: the stored state is rebuilt field by field and the repository opened again.
    let mut rebuilt = RebaseState {
        plan: state.plan.clone(),
        cursor: state.cursor,
        original_branch_ref: state.original_branch_ref.clone(),
        original_tip: state.original_tip,
        onto: state.onto,
        group_start_parent: state.group_start_parent,
        expected_head: state.expected_head,
        stop: state.stop,
    };
    drop(repo);
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_eq!(
        git_core::repo::RebaseInfo::new(&rebuilt, RebaseHealth::Paused)
            .editing
            .as_deref(),
        Some(ids[0].as_str())
    );

    let result = continue_with(&repo, &mut rebuilt, Some("add a, edited\n")).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let messages: Vec<String> = history(&repo, &branch, &onto)
        .into_iter()
        .map(|(message, _)| message)
        .collect();
    assert_eq!(messages, ["add a, edited\n", "add b"]);
    assert_eq!(common::read(dir.path(), "a.txt"), "a, edited\n");
}

#[test]
fn edit_commit_on_the_latest_commit_recommits_it() {
    let (dir, repo) = init_repo();
    let (_, ids, branch) = base_a_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &ids[0], &ids[1..]);
    write_file(dir.path(), "b.txt", "b, edited\n");
    stage(&repo, "b.txt");

    let result = continue_with(&repo, &mut state, Some("add b, edited")).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(
        history(&repo, &branch, &ids[0]),
        [(
            "add b, edited".to_string(),
            files(&[
                ("a.txt", "a\n"),
                ("b.txt", "b, edited\n"),
                ("f.txt", "base\n")
            ])
        )]
    );
}
