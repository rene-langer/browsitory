use super::*;

#[test]
fn commits_since_and_start_rebase_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let commits = handle.commits_since(onto.clone()).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].summary, "add a");

    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: commits[0].id.clone(),
        action: git_core::rebase::RebaseAction::Pick,
        combined_message: None,
    }];
    let result = handle.start_rebase(onto, plan, None).unwrap();

    assert_eq!(result, git_core::rebase::RebaseStepResult::Done);
    assert_eq!(rebase_progress(&handle), None);
}

#[test]
fn rebase_continue_and_abort_rebase_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let commits = handle.commits_since(onto.clone()).unwrap();
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: commits[0].id.clone(),
        action: git_core::rebase::RebaseAction::Edit,
        combined_message: None,
    }];
    let result = handle.start_rebase(onto, plan, None).unwrap();
    assert_eq!(result, git_core::rebase::RebaseStepResult::PausedForEdit);
    // 1-indexed: the first (and here only) paused step reports "Step 1 of 1", not "0 of 1".
    assert_eq!(rebase_progress(&handle), Some((1, 1)));

    handle.abort_rebase().unwrap();

    assert_eq!(rebase_progress(&handle), None);
    assert!(handle.get_status().unwrap().is_empty());
}

/// #473: a one-click history edit's undo entry names the edit, also once a paused one finishes.
#[test]
fn a_history_edit_records_its_own_undo_label() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    let head = |repo: &Repository| {
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string()
    };
    commit_all(&repo, "base commit");
    let onto = head(&repo);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    let a = head(&repo);
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");
    let b = head(&repo);

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let entry = |commit_id: &str, action| git_core::rebase::RebasePlanEntry {
        commit_id: commit_id.to_string(),
        action,
        combined_message: None,
    };
    let edit = |kind, commit_id: &str| {
        Some(crate::worker::HistoryEdit {
            kind,
            commit_id: commit_id.to_string(),
        })
    };

    let plan = vec![
        entry(&a, git_core::rebase::RebaseAction::Drop),
        entry(&b, git_core::rebase::RebaseAction::Pick),
    ];
    let result = handle
        .start_rebase(
            onto.clone(),
            plan,
            edit(crate::worker::HistoryEditKind::Drop, &a),
        )
        .unwrap();
    assert_eq!(result, git_core::rebase::RebaseStepResult::Done);
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some(format!("Drop {}", &a[..7]))
    );
    handle.undo().unwrap();
    assert!(dir.path().join("a.txt").exists());

    // Paused on the way (an Edit stop stands in for a conflict), then continued: the entry
    // recorded at the start keeps the edit's label.
    let plan = vec![
        entry(&b, git_core::rebase::RebaseAction::Edit),
        entry(&a, git_core::rebase::RebaseAction::Pick),
    ];
    let result = handle
        .start_rebase(onto, plan, edit(crate::worker::HistoryEditKind::MoveUp, &a))
        .unwrap();
    assert_eq!(result, git_core::rebase::RebaseStepResult::PausedForEdit);
    assert_eq!(
        handle.rebase_continue().unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some(format!("Move {} up", &a[..7]))
    );
}

/// The index entry of `path`'s content, as text.
fn staged_content(path: &Path, file: &str) -> String {
    let repo = Repository::open(path).unwrap();
    let index = repo.index().unwrap();
    let entry = index.get_path(Path::new(file), 0).unwrap();
    let blob = repo.find_blob(entry.id).unwrap();
    let content = String::from_utf8_lossy(blob.content()).into_owned();
    content
}

/// #883: folding staged changes into an older commit is one Undo step, which brings the
/// changes back staged on the old tip.
#[test]
fn folding_staged_changes_into_a_commit_is_one_undo_step() {
    let (dir, repo) = init_repo();
    let head = |repo: &Repository| repo.head().unwrap().target().unwrap();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    let a = head(&repo).to_string();
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");
    let b = head(&repo);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    write_file(dir.path(), "a.txt", "a, folded\n");
    handle.stage_file("a.txt".to_string()).unwrap();

    assert_eq!(
        handle.fold_staged(a.clone()).unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );

    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some(format!("Fold staged changes into {}", &a[..7]))
    );
    let reopened = Repository::open(dir.path()).unwrap();
    let tip = reopened.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(tip.summary().unwrap(), Some("add b"));
    let folded = tip.parent(0).unwrap();
    assert_eq!(folded.summary().unwrap(), Some("add a"));
    let blob = folded
        .tree()
        .unwrap()
        .get_path(Path::new("a.txt"))
        .unwrap()
        .to_object(&reopened)
        .unwrap()
        .peel_to_blob()
        .unwrap();
    assert_eq!(blob.content(), b"a, folded\n");
    assert!(handle.get_status().unwrap().is_empty());

    handle.undo().unwrap();

    assert_eq!(head(&Repository::open(dir.path()).unwrap()), b);
    assert_eq!(staged_content(dir.path(), "a.txt"), "a, folded\n");
}

/// `base` → "add a and c" (a.txt, c.txt) → "add b" (b.txt). Returns `(onto, plan stopping at
/// "add a and c" then picking "add b", its id, the tip)`.
fn split_fixture(
    dir: &Path,
    repo: &Repository,
) -> (
    String,
    Vec<git_core::rebase::RebasePlanEntry>,
    String,
    git2::Oid,
) {
    use git_core::rebase::{RebaseAction, RebasePlanEntry};
    let head = |repo: &Repository| repo.head().unwrap().target().unwrap();
    write_file(dir, "base.txt", "v1\n");
    commit_all(repo, "base commit");
    let onto = head(repo).to_string();
    write_file(dir, "a.txt", "a\n");
    write_file(dir, "c.txt", "c\n");
    commit_all(repo, "add a and c");
    let ac = head(repo).to_string();
    write_file(dir, "b.txt", "b\n");
    commit_all(repo, "add b");
    let b = head(repo);
    let entry = |id: &str, action| RebasePlanEntry {
        commit_id: id.to_string(),
        action,
        combined_message: None,
    };
    let plan = vec![
        entry(&ac, RebaseAction::Edit),
        entry(&b.to_string(), RebaseAction::Pick),
    ];
    (onto, plan, ac, b)
}

/// The branch's commit summaries after `onto`, oldest first.
fn summaries_since(path: &Path, onto: &str) -> Vec<String> {
    let repo = Repository::open(path).unwrap();
    let mut walk = repo.revwalk().unwrap();
    walk.push_head().unwrap();
    walk.hide(git2::Oid::from_str(onto).unwrap()).unwrap();
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)
        .unwrap();
    walk.map(|id| {
        let commit = repo.find_commit(id.unwrap()).unwrap();
        let summary = commit.summary().unwrap().unwrap().to_string();
        summary
    })
    .collect()
}

/// #884: Split commit… through the worker. A part is committed at the stop, Continue with a
/// rest left is refused with `RestLeft` (the rest named by `get_edit_rest`), and Continue
/// committing the rest finishes it. The whole split is one Undo step, "Split 1a2b3c4".
#[test]
fn splitting_a_commit_is_one_undo_step() {
    use crate::wire::ErrorKind;
    use git_core::rebase::{EditRest, RebaseStepResult};
    let (dir, repo) = init_repo();
    let (onto, plan, ac, b) = split_fixture(dir.path(), &repo);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let split = crate::worker::HistoryEdit {
        kind: crate::worker::HistoryEditKind::Split,
        commit_id: ac.clone(),
    };
    assert_eq!(
        handle
            .start_rebase(onto.clone(), plan, Some(split))
            .unwrap(),
        RebaseStepResult::PausedForEdit
    );
    handle.unstage_file("c.txt".to_string()).unwrap();

    handle.rebase_commit_part("add a".to_string()).unwrap();

    let info = handle.get_head_info().unwrap().rebase.unwrap();
    assert_eq!(info.editing.as_deref(), Some(ac.as_str()));
    assert_eq!(info.split_parts, 1);
    let refused = handle
        .rebase_continue_with_rest(Some("add c".to_string()), None)
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::RestLeft);
    assert_eq!(handle.get_edit_rest().unwrap(), ["c.txt"]);

    assert_eq!(
        handle
            .rebase_continue_with_rest(Some("add c".to_string()), Some(EditRest::Commit))
            .unwrap(),
        RebaseStepResult::Done
    );

    assert_eq!(
        summaries_since(dir.path(), &onto),
        ["add a", "add c", "add b"]
    );
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some(format!("Split {}", &ac[..7]))
    );
    handle.undo().unwrap();
    let reopened = Repository::open(dir.path()).unwrap();
    assert_eq!(reopened.head().unwrap().target().unwrap(), b);
}

/// #884: a part refused for an empty message changes nothing, and outside an edit stop there
/// is no rest.
#[test]
fn a_part_with_an_empty_message_is_refused_and_no_rest_without_a_rebase() {
    use git_core::rebase::RebaseStepResult;
    let (dir, repo) = init_repo();
    let (onto, plan, _, _) = split_fixture(dir.path(), &repo);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert!(handle.get_edit_rest().unwrap().is_empty());
    assert_eq!(
        handle.start_rebase(onto.clone(), plan, None).unwrap(),
        RebaseStepResult::PausedForEdit
    );
    handle.unstage_file("c.txt".to_string()).unwrap();

    let error = handle.rebase_commit_part(" ".to_string()).unwrap_err();

    assert!(error.message.contains("message is empty"), "{error:?}");
    assert_eq!(
        handle.get_head_info().unwrap().rebase.unwrap().split_parts,
        0
    );
    assert_eq!(rebase_progress(&handle), Some((1, 2)));
}
