//! Split a commit into several (#884): at an Edit step's stop the user commits the staged
//! changes as a part (`rebase_commit_part`), as often as needed; the rebase stays paused at the
//! same step, and Continue commits what is still staged as the last part (nothing when every
//! change went into a part), then replays the rest.
mod common;

use common::{commit_all, init_repo, write_file};
use git_core::hooks::HookOptions;
use git_core::rebase::{
    EditRest, RebaseAction, RebaseError, RebaseHealth, RebasePlanEntry, RebaseState,
    RebaseStepResult,
};
use std::path::Path;

fn entry(commit_id: &str, action: RebaseAction) -> RebasePlanEntry {
    RebasePlanEntry {
        commit_id: commit_id.to_string(),
        action,
        combined_message: None,
    }
}

/// `base` (onto; f.txt) → "add a and c" (a.txt, c.txt) → "add b" (b.txt) on the checked-out
/// branch. Returns `(onto, [ac, b], branch ref)`.
fn base_ac_b(dir: &Path, repo: &git2::Repository) -> (String, Vec<String>, String) {
    write_file(dir, "f.txt", "base\n");
    commit_all(repo, "base");
    let onto = common::head_id(repo).to_string();
    write_file(dir, "a.txt", "a\n");
    write_file(dir, "c.txt", "c\n");
    commit_all(repo, "add a and c\n");
    let ac = common::head_id(repo).to_string();
    write_file(dir, "b.txt", "b\n");
    commit_all(repo, "add b\n");
    let b = common::head_id(repo).to_string();
    let branch = repo.head().unwrap().name().unwrap().to_string();
    (onto, vec![ac, b], branch)
}

/// Starts the rebase the commit menu builds for `ids[0]`: Edit, then Pick the rest.
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

/// Takes `path` out of the index, back to HEAD's version (gone, for a file HEAD lacks).
fn unstage(repo: &git2::Repository, path: &str) {
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.reset_default(Some(head.as_object()), [path]).unwrap();
}

fn commit_part(
    repo: &git2::Repository,
    state: &mut RebaseState,
    message: &str,
) -> Result<String, RebaseError> {
    git_core::rebase::rebase_commit_part(repo, state, message, HookOptions::default())
}

fn continue_with(
    repo: &git2::Repository,
    state: &mut RebaseState,
    message: Option<&str>,
) -> Result<RebaseStepResult, RebaseError> {
    git_core::rebase::rebase_continue_with_message(repo, state, message, HookOptions::default())
}

/// The commits after `onto` up to `tip`, oldest first, as (message, files with content).
fn history_from(
    repo: &git2::Repository,
    tip: git2::Oid,
    onto: &str,
) -> Vec<(String, Vec<(String, String)>)> {
    let mut walk = repo.revwalk().unwrap();
    walk.push(tip).unwrap();
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

fn history(
    repo: &git2::Repository,
    branch: &str,
    onto: &str,
) -> Vec<(String, Vec<(String, String)>)> {
    let tip = repo.find_reference(branch).unwrap().target().unwrap();
    history_from(repo, tip, onto)
}

fn files(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(path, content)| (path.to_string(), content.to_string()))
        .collect()
}

/// The history every split of "add a and c" into "add a" and "add c" ends with.
fn split_history() -> Vec<(String, Vec<(String, String)>)> {
    vec![
        (
            "add a\n".to_string(),
            files(&[("a.txt", "a\n"), ("f.txt", "base\n")]),
        ),
        (
            "add c\n".to_string(),
            files(&[("a.txt", "a\n"), ("c.txt", "c\n"), ("f.txt", "base\n")]),
        ),
        (
            "add b\n".to_string(),
            files(&[
                ("a.txt", "a\n"),
                ("b.txt", "b\n"),
                ("c.txt", "c\n"),
                ("f.txt", "base\n"),
            ]),
        ),
    ]
}

#[test]
fn a_part_is_committed_on_the_stop_and_the_rebase_stays_paused_at_the_same_step() {
    let (dir, repo) = init_repo();
    let (onto, ids, _) = base_ac_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    unstage(&repo, "c.txt");

    let part = commit_part(&repo, &mut state, "add a\n").unwrap();

    // HEAD is the part, detached on the commit's parent, holding only what was staged.
    assert!(repo.head_detached().unwrap());
    assert_eq!(common::head_id(&repo).to_string(), part);
    assert_eq!(
        history_from(&repo, common::head_id(&repo), &onto),
        [(
            "add a\n".to_string(),
            files(&[("a.txt", "a\n"), ("f.txt", "base\n")])
        )]
    );
    // c.txt, left out of the part, is still in the work tree (untracked now).
    assert_eq!(common::read(dir.path(), "c.txt"), "c\n");
    // Still the same edit stop, one part committed.
    assert_eq!(state.cursor, 0);
    assert_eq!(state.split_parts, 1);
    assert_eq!(state.expected_head.to_string(), part);
    assert_eq!(
        git_core::rebase::rebase_health(&repo, &state).unwrap(),
        RebaseHealth::Paused
    );
    let info = git_core::repo::RebaseInfo::new(&state, RebaseHealth::Paused);
    assert_eq!(info.editing.as_deref(), Some(ids[0].as_str()));
    assert_eq!(info.split_parts, 1);
    assert_eq!((info.current_step, info.total_steps), (1, 2));
}

#[test]
fn continue_commits_the_staged_rest_as_the_last_part_and_replays_the_rest() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_ac_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    unstage(&repo, "c.txt");
    commit_part(&repo, &mut state, "add a\n").unwrap();
    stage(&repo, "c.txt");

    let result = continue_with(&repo, &mut state, Some("add c\n")).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(repo.head().unwrap().name().unwrap(), branch);
    assert_eq!(history(&repo, &branch, &onto), split_history());
    assert!(repo.statuses(None).unwrap().is_empty());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn continue_after_every_change_went_into_a_part_adds_no_commit() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_ac_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    unstage(&repo, "c.txt");
    commit_part(&repo, &mut state, "add a\n").unwrap();
    stage(&repo, "c.txt");
    commit_part(&repo, &mut state, "add c\n").unwrap();
    assert_eq!(state.split_parts, 2);

    let result = continue_with(&repo, &mut state, None).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(history(&repo, &branch, &onto), split_history());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn a_part_keeps_the_original_commits_author() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_ac_b(dir.path(), &repo);
    let original = repo
        .find_commit(git2::Oid::from_str(&ids[0]).unwrap())
        .unwrap();
    let mut state = edit_first(&repo, &onto, &ids);
    // Someone else continues the split.
    repo.config()
        .unwrap()
        .set_str("user.name", "Someone Else")
        .unwrap();
    unstage(&repo, "c.txt");
    commit_part(&repo, &mut state, "add a\n").unwrap();
    stage(&repo, "c.txt");
    continue_with(&repo, &mut state, Some("add c\n")).unwrap();

    let tip = repo
        .find_reference(&branch)
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let last_part = tip.parent(0).unwrap();
    let first_part = last_part.parent(0).unwrap();
    for part in [&first_part, &last_part] {
        assert_eq!(part.author().name(), original.author().name());
        assert_eq!(part.author().email(), original.author().email());
        assert_eq!(part.author().when(), original.author().when());
        assert_eq!(part.committer().name().unwrap(), "Someone Else");
    }
}

#[test]
fn a_part_with_nothing_staged_is_refused_and_changes_nothing() {
    let (dir, repo) = init_repo();
    let (onto, ids, _) = base_ac_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    unstage(&repo, "a.txt");
    unstage(&repo, "c.txt");
    let before = state.clone();

    let error = commit_part(&repo, &mut state, "add a\n").unwrap_err();

    assert!(
        matches!(error, RebaseError::NothingStagedForPart),
        "{error:?}"
    );
    assert_eq!(state, before);
    assert_eq!(common::head_id(&repo).to_string(), onto);
}

#[test]
fn a_part_with_an_empty_message_is_refused_and_changes_nothing() {
    let (dir, repo) = init_repo();
    let (onto, ids, _) = base_ac_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    unstage(&repo, "c.txt");
    let before = state.clone();

    let error = commit_part(&repo, &mut state, " \n").unwrap_err();

    assert!(matches!(error, RebaseError::EmptyMessage), "{error:?}");
    assert_eq!(state, before);
    assert_eq!(common::head_id(&repo).to_string(), onto);
}

#[test]
fn a_part_is_refused_at_a_conflict_stop() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "base\n");
    commit_all(&repo, "base");
    let onto = common::head_id(&repo).to_string();
    write_file(dir.path(), "f.txt", "one\n");
    commit_all(&repo, "one");
    write_file(dir.path(), "f.txt", "two\n");
    commit_all(&repo, "two");
    let two = common::head_id(&repo).to_string();
    let (mut state, result) =
        git_core::rebase::start_rebase(&repo, &onto, vec![entry(&two, RebaseAction::Pick)])
            .unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    write_file(dir.path(), "f.txt", "resolved\n");
    stage(&repo, "f.txt");
    let before = state.clone();

    let error = commit_part(&repo, &mut state, "part\n").unwrap_err();

    assert!(matches!(error, RebaseError::NotEditing), "{error:?}");
    assert_eq!(state, before);
    assert_eq!(common::head_id(&repo).to_string(), onto);
}

#[test]
fn abort_after_a_part_puts_the_branch_back() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_ac_b(dir.path(), &repo);
    let tip = repo.find_reference(&branch).unwrap().target().unwrap();
    let mut state = edit_first(&repo, &onto, &ids);
    unstage(&repo, "c.txt");
    commit_part(&repo, &mut state, "add a\n").unwrap();

    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert!(abort.restored);
    assert_eq!(repo.head().unwrap().name().unwrap(), branch);
    assert_eq!(common::head_id(&repo), tip);
    for (path, content) in [("a.txt", "a\n"), ("b.txt", "b\n"), ("c.txt", "c\n")] {
        assert_eq!(common::read(dir.path(), path), content);
    }
    assert!(repo.statuses(None).unwrap().is_empty());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn a_split_restored_from_its_file_keeps_its_parts_and_continues() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_ac_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    unstage(&repo, "c.txt");
    commit_part(&repo, &mut state, "add a\n").unwrap();
    stage(&repo, "c.txt");
    commit_part(&repo, &mut state, "add c\n").unwrap();
    // A restart: the state goes through `rebase.json`.
    let bytes = git_core::rebase_file::encode(&state).unwrap();
    drop(repo);
    let repo = git2::Repository::open(dir.path()).unwrap();
    let mut restored = git_core::rebase_file::decode(&bytes, &repo).unwrap();
    assert_eq!(restored, state);
    assert_eq!(restored.split_parts, 2);

    let result = continue_with(&repo, &mut restored, None).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(history(&repo, &branch, &onto), split_history());
}

#[test]
fn an_edit_on_the_latest_commit_splits_too() {
    let (dir, repo) = init_repo();
    let (_, ids, branch) = base_ac_b(dir.path(), &repo);
    // Split "add b" into b.txt and a new d.txt staged at the stop.
    let mut state = edit_first(&repo, &ids[0], &ids[1..]);
    write_file(dir.path(), "d.txt", "d\n");
    commit_part(&repo, &mut state, "add b\n").unwrap();
    stage(&repo, "d.txt");

    let result = continue_with(&repo, &mut state, Some("add d\n")).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let messages: Vec<String> = history(&repo, &branch, &ids[0])
        .into_iter()
        .map(|(message, _)| message)
        .collect();
    assert_eq!(messages, ["add b\n", "add d\n"]);
}

fn continue_with_rest(
    repo: &git2::Repository,
    state: &mut RebaseState,
    message: Option<&str>,
    rest: Option<EditRest>,
) -> Result<RebaseStepResult, RebaseError> {
    git_core::rebase::rebase_continue_with_rest(repo, state, message, rest, HookOptions::default())
}

#[test]
fn continue_with_a_rest_left_is_refused_naming_it_and_changes_nothing() {
    let (dir, repo) = init_repo();
    let (onto, ids, _) = base_ac_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    unstage(&repo, "c.txt");
    let part = commit_part(&repo, &mut state, "add a\n").unwrap();
    // An untracked file of the user's own is no part of the commit.
    write_file(dir.path(), "notes.txt", "mine\n");
    let before = state.clone();

    assert_eq!(
        git_core::rebase::edit_rest(&repo, &state).unwrap(),
        ["c.txt"]
    );
    let error = continue_with(&repo, &mut state, Some("add c\n")).unwrap_err();

    match error {
        RebaseError::RestLeft { paths } => assert_eq!(paths, ["c.txt"]),
        other => panic!("expected RestLeft, got {other:?}"),
    }
    assert_eq!(state, before);
    assert_eq!(common::head_id(&repo).to_string(), part);
    assert_eq!(common::read(dir.path(), "c.txt"), "c\n");
}

#[test]
fn an_edit_stop_with_unstaged_changes_has_them_as_its_rest() {
    let (dir, repo) = init_repo();
    let (onto, ids, _) = base_ac_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    // A plain edit: a.txt changed but not staged, f.txt (not in the commit) changed too.
    write_file(dir.path(), "a.txt", "a, edited\n");
    write_file(dir.path(), "f.txt", "edited\n");

    assert_eq!(
        git_core::rebase::edit_rest(&repo, &state).unwrap(),
        ["a.txt", "f.txt"]
    );
    let error = continue_with(&repo, &mut state, None).unwrap_err();

    assert!(matches!(error, RebaseError::RestLeft { .. }), "{error:?}");
    assert_eq!(common::head_id(&repo).to_string(), onto);
}

#[test]
fn an_edit_stop_with_nothing_left_has_no_rest() {
    let (dir, repo) = init_repo();
    let (onto, ids, _) = base_ac_b(dir.path(), &repo);
    let state = edit_first(&repo, &onto, &ids);
    write_file(dir.path(), "notes.txt", "mine\n");

    assert!(git_core::rebase::edit_rest(&repo, &state)
        .unwrap()
        .is_empty());
}

#[test]
fn committing_the_rest_makes_it_the_last_part() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_ac_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    unstage(&repo, "c.txt");
    commit_part(&repo, &mut state, "add a\n").unwrap();

    let result =
        continue_with_rest(&repo, &mut state, Some("add c\n"), Some(EditRest::Commit)).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(history(&repo, &branch, &onto), split_history());
    assert!(repo.statuses(None).unwrap().is_empty());
}

#[test]
fn discarding_the_rest_leaves_it_out_of_the_history() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_ac_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    unstage(&repo, "c.txt");
    commit_part(&repo, &mut state, "add a\n").unwrap();
    write_file(dir.path(), "a.txt", "a, unstaged\n");
    write_file(dir.path(), "notes.txt", "mine\n");

    let result = continue_with_rest(&repo, &mut state, None, Some(EditRest::Discard)).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(
        history(&repo, &branch, &onto),
        [
            (
                "add a\n".to_string(),
                files(&[("a.txt", "a\n"), ("f.txt", "base\n")])
            ),
            (
                "add b\n".to_string(),
                files(&[("a.txt", "a\n"), ("b.txt", "b\n"), ("f.txt", "base\n")])
            ),
        ]
    );
    assert!(!dir.path().join("c.txt").exists());
    assert_eq!(common::read(dir.path(), "a.txt"), "a\n");
    // The user's own untracked file is no part of the rest and stays.
    assert_eq!(common::read(dir.path(), "notes.txt"), "mine\n");
}

#[test]
fn committing_the_rest_of_a_plain_edit_commits_the_unstaged_changes() {
    let (dir, repo) = init_repo();
    let (onto, ids, branch) = base_ac_b(dir.path(), &repo);
    let mut state = edit_first(&repo, &onto, &ids);
    write_file(dir.path(), "a.txt", "a, edited\n");

    let result = continue_with_rest(&repo, &mut state, None, Some(EditRest::Commit)).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let first = &history(&repo, &branch, &onto)[0];
    assert_eq!(
        first,
        &(
            "add a and c\n".to_string(),
            files(&[
                ("a.txt", "a, edited\n"),
                ("c.txt", "c\n"),
                ("f.txt", "base\n")
            ])
        )
    );
}
