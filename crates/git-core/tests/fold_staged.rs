//! Fold staged changes into an older commit (#883): like `git commit --fixup <target>` and
//! `git rebase --autosquash` in one step. The target keeps its message and author.
mod common;

use common::{commit_all, init_repo, write_file};
use git_core::hooks::HookOptions;
use git_core::rebase::{RebaseError, RebaseState, RebaseStepResult};
use std::path::Path;

fn fold(
    repo: &git2::Repository,
    target: &str,
) -> Result<(Option<RebaseState>, RebaseStepResult), RebaseError> {
    git_core::fold::fold_staged_into(repo, target, HookOptions::default())
}

fn stage(repo: &git2::Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.read(false).unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

/// `base` (f.txt = "base\n") → "add a\n\nbody\n" (a.txt = "a\n", by "Original Author") → "add b"
/// (b.txt = "b\n"). Returns `(base, a, b, branch ref)`.
fn base_a_b(dir: &Path, repo: &git2::Repository) -> (String, String, String, String) {
    write_file(dir, "f.txt", "base\n");
    commit_all(repo, "base");
    let base = common::head_id(repo);
    write_file(dir, "a.txt", "a\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let author = git2::Signature::new(
        "Original Author",
        "original@example.com",
        &git2::Time::new(1_600_000_000, 0),
    )
    .unwrap();
    let committer = repo.signature().unwrap();
    let parent = repo.find_commit(base).unwrap();
    repo.commit(
        Some("HEAD"),
        &author,
        &committer,
        "add a\n\nbody\n",
        &tree,
        &[&parent],
    )
    .unwrap();
    let a = common::head_id(repo).to_string();
    write_file(dir, "b.txt", "b\n");
    commit_all(repo, "add b");
    let b = common::head_id(repo).to_string();
    let branch = repo.head().unwrap().name().unwrap().to_string();
    (base.to_string(), a, b, branch)
}

/// The branch's commits after `onto`, oldest first, as (message, files).
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
        let mut files = Vec::new();
        commit
            .tree()
            .unwrap()
            .walk(git2::TreeWalkMode::PreOrder, |root, item| {
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

fn statuses(repo: &git2::Repository) -> Vec<(String, git2::Status)> {
    repo.statuses(None)
        .unwrap()
        .iter()
        .map(|e| (e.path().unwrap().to_string(), e.status()))
        .collect()
}

#[test]
fn a_clean_fold_puts_the_staged_changes_into_the_older_commit() {
    let (dir, repo) = init_repo();
    let (base, a, _, branch) = base_a_b(dir.path(), &repo);
    write_file(dir.path(), "a.txt", "a, folded\n");
    write_file(dir.path(), "new.txt", "new\n");
    stage(&repo, "a.txt");
    stage(&repo, "new.txt");

    let (state, result) = fold(&repo, &a).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert!(state.is_none());
    assert_eq!(repo.head().unwrap().name().unwrap(), branch);
    assert_eq!(
        history(&repo, &branch, &base),
        [
            (
                "add a\n\nbody\n".to_string(),
                files(&[
                    ("a.txt", "a, folded\n"),
                    ("f.txt", "base\n"),
                    ("new.txt", "new\n")
                ])
            ),
            (
                "add b".to_string(),
                files(&[
                    ("a.txt", "a, folded\n"),
                    ("b.txt", "b\n"),
                    ("f.txt", "base\n"),
                    ("new.txt", "new\n")
                ])
            ),
        ]
    );
    // The folded commit keeps its author.
    let tip = repo
        .find_reference(&branch)
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let folded = tip.parent(0).unwrap();
    let original = repo.find_commit(git2::Oid::from_str(&a).unwrap()).unwrap();
    assert_eq!(folded.author().name().ok(), Some("Original Author"));
    assert_eq!(folded.author().when(), original.author().when());
    assert!(statuses(&repo).is_empty());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn folding_into_the_latest_commit_amends_it() {
    let (dir, repo) = init_repo();
    let (_, a, b, branch) = base_a_b(dir.path(), &repo);
    write_file(dir.path(), "b.txt", "b, folded\n");
    stage(&repo, "b.txt");

    let (state, result) = fold(&repo, &b).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert!(state.is_none());
    assert_eq!(
        history(&repo, &branch, &a),
        [(
            "add b".to_string(),
            files(&[
                ("a.txt", "a\n"),
                ("b.txt", "b, folded\n"),
                ("f.txt", "base\n")
            ])
        )]
    );
    let original = repo.find_commit(git2::Oid::from_str(&b).unwrap()).unwrap();
    let amended = repo
        .find_reference(&branch)
        .unwrap()
        .peel_to_commit()
        .unwrap();
    assert_ne!(amended.id(), original.id());
    assert_eq!(amended.author().when(), original.author().when());
    assert_eq!(repo.head().unwrap().name().unwrap(), branch);
    assert!(statuses(&repo).is_empty());
}

/// `base` (f.txt = "base\n") → "one" (f.txt = "one\n") → "two" (f.txt = "two\n"); `f.txt` staged
/// as "staged\n", which conflicts with "two" once folded into "one". Returns `(base, one,
/// two, branch)`.
fn conflicting_fold(dir: &Path, repo: &git2::Repository) -> (String, String, String, String) {
    write_file(dir, "f.txt", "base\n");
    commit_all(repo, "base");
    let base = common::head_id(repo).to_string();
    write_file(dir, "f.txt", "one\n");
    commit_all(repo, "one");
    let one = common::head_id(repo).to_string();
    write_file(dir, "f.txt", "two\n");
    commit_all(repo, "two");
    let two = common::head_id(repo).to_string();
    write_file(dir, "f.txt", "staged\n");
    stage(repo, "f.txt");
    let branch = repo.head().unwrap().name().unwrap().to_string();
    (base, one, two, branch)
}

#[test]
fn a_fold_that_conflicts_with_a_later_commit_pauses_and_continues() {
    let (dir, repo) = init_repo();
    let (base, one, _, branch) = conflicting_fold(dir.path(), &repo);

    let (state, result) = fold(&repo, &one).unwrap();

    assert_eq!(
        result,
        RebaseStepResult::Conflicted {
            files: vec!["f.txt".to_string()]
        }
    );
    let mut state = state.expect("a paused rebase");
    write_file(dir.path(), "f.txt", "one, folded\n");
    stage(&repo, "f.txt");
    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();
    // "two" changes the same line next.
    assert!(
        matches!(result, RebaseStepResult::Conflicted { .. }),
        "{result:?}"
    );
    write_file(dir.path(), "f.txt", "two, after\n");
    stage(&repo, "f.txt");
    assert_eq!(
        git_core::rebase::rebase_continue(&repo, &mut state).unwrap(),
        RebaseStepResult::Done
    );
    assert_eq!(
        history(&repo, &branch, &base),
        [
            ("one".to_string(), files(&[("f.txt", "one, folded\n")])),
            ("two".to_string(), files(&[("f.txt", "two, after\n")])),
        ]
    );
}

#[test]
fn abort_of_a_paused_fold_puts_the_branch_back_with_the_changes_staged() {
    let (dir, repo) = init_repo();
    let (_, one, two, branch) = conflicting_fold(dir.path(), &repo);
    let (state, result) = fold(&repo, &one).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    let state = state.unwrap();

    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert!(abort.restored);
    assert_eq!(repo.head().unwrap().name().unwrap(), branch);
    assert_eq!(common::head_id(&repo).to_string(), two);
    assert_eq!(common::read(dir.path(), "f.txt"), "staged\n");
    assert_eq!(
        statuses(&repo),
        [("f.txt".to_string(), git2::Status::INDEX_MODIFIED)]
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn a_paused_fold_restored_from_its_file_still_aborts_to_the_staged_changes() {
    let (dir, repo) = init_repo();
    let (_, one, two, branch) = conflicting_fold(dir.path(), &repo);
    let (state, _) = fold(&repo, &one).unwrap();
    let state = state.unwrap();
    assert!(state.fold_base.is_some());
    // A restart: the state goes through `rebase.json`.
    let bytes = git_core::rebase_file::encode(&state).unwrap();
    drop(repo);
    let repo = git2::Repository::open(dir.path()).unwrap();
    let restored = git_core::rebase_file::decode(&bytes, &repo).unwrap();
    assert_eq!(restored, state);

    git_core::rebase::abort_rebase(&repo, &restored).unwrap();

    assert_eq!(repo.head().unwrap().name().unwrap(), branch);
    assert_eq!(common::head_id(&repo).to_string(), two);
    assert_eq!(
        statuses(&repo),
        [("f.txt".to_string(), git2::Status::INDEX_MODIFIED)]
    );
}

#[test]
fn a_fold_refuses_with_nothing_staged() {
    let (dir, repo) = init_repo();
    let (_, a, b, _) = base_a_b(dir.path(), &repo);

    let error = fold(&repo, &a).unwrap_err();

    assert!(matches!(error, RebaseError::NothingStaged), "{error:?}");
    assert_eq!(common::head_id(&repo).to_string(), b);
}

#[test]
fn a_fold_refuses_unstaged_changes_to_tracked_files_and_changes_nothing() {
    let (dir, repo) = init_repo();
    let (_, a, b, _) = base_a_b(dir.path(), &repo);
    write_file(dir.path(), "a.txt", "a, staged\n");
    stage(&repo, "a.txt");
    write_file(dir.path(), "f.txt", "unstaged\n");
    // Untracked files are fine.
    write_file(dir.path(), "untracked.txt", "u\n");

    let error = fold(&repo, &a).unwrap_err();

    match error {
        RebaseError::UnstagedChanges { paths } => assert_eq!(paths, ["f.txt"]),
        other => panic!("{other:?}"),
    }
    assert_eq!(common::head_id(&repo).to_string(), b);
    assert_eq!(
        statuses(&repo),
        [
            ("a.txt".to_string(), git2::Status::INDEX_MODIFIED),
            ("f.txt".to_string(), git2::Status::WT_MODIFIED),
            ("untracked.txt".to_string(), git2::Status::WT_NEW),
        ]
    );
}

#[test]
fn a_fold_refuses_a_commit_that_is_not_on_the_branch() {
    let (dir, repo) = init_repo();
    let (base, _, b, _) = base_a_b(dir.path(), &repo);
    // A commit on another branch, off base.
    let parent = repo
        .find_commit(git2::Oid::from_str(&base).unwrap())
        .unwrap();
    let tree = parent.tree().unwrap();
    let signature = repo.signature().unwrap();
    let elsewhere = repo
        .commit(None, &signature, &signature, "elsewhere", &tree, &[&parent])
        .unwrap();
    write_file(dir.path(), "a.txt", "a, staged\n");
    stage(&repo, "a.txt");

    let error = fold(&repo, &elsewhere.to_string()).unwrap_err();

    assert!(matches!(error, RebaseError::InvalidPlan(_)), "{error:?}");
    assert_eq!(common::head_id(&repo).to_string(), b);
    assert_eq!(
        statuses(&repo),
        [("a.txt".to_string(), git2::Status::INDEX_MODIFIED)]
    );
}
