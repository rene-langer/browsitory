use super::*;
const HUNK: git_core::diff::HunkOptions = git_core::diff::HunkOptions {
    context_lines: 3,
    algorithm: git_core::diff::DiffAlgorithm::Default,
};

#[test]
fn failed_signing_replies_err_and_records_no_undo_entry() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "base");
    {
        let mut config = repo.config().unwrap();
        config.set_bool("commit.gpgSign", true).unwrap();
        config
            .set_str("gpg.program", "browsitory-no-such-gpg")
            .unwrap();
    }
    write_file(dir.path(), "b.txt", "b");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("b.txt".into()).unwrap();
    let undo_before = handle.get_undo_state().unwrap().undo;
    let result = handle.commit("signed?".into(), false, Vec::new(), mpsc::channel().0);

    assert_eq!(
        result.unwrap_err(),
        crate::wire::ErrorDto::from("Signing program 'browsitory-no-such-gpg' was not found.")
    );
    assert_eq!(handle.get_undo_state().unwrap().undo, undo_before);
}

#[test]
fn stage_hunk_then_commit_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let hunks = handle
        .get_working_diff(
            "tracked.txt".into(),
            false,
            git_core::diff::DiffOptions::default(),
            None,
        )
        .unwrap();
    assert_eq!(hunks.len(), 1);

    handle
        .stage_hunk(
            "tracked.txt".into(),
            hunks[0].old_start,
            hunks[0].new_start,
            HUNK,
        )
        .unwrap();

    let staged = handle.get_status().unwrap();
    assert!(staged.iter().any(|e| e.path == "tracked.txt" && e.staged));
}

#[test]
fn unstage_hunk_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let hunks = handle
        .get_working_diff(
            "tracked.txt".into(),
            false,
            git_core::diff::DiffOptions::default(),
            None,
        )
        .unwrap();
    handle
        .stage_hunk(
            "tracked.txt".into(),
            hunks[0].old_start,
            hunks[0].new_start,
            HUNK,
        )
        .unwrap();

    handle
        .unstage_hunk(
            "tracked.txt".into(),
            hunks[0].old_start,
            hunks[0].new_start,
            HUNK,
        )
        .unwrap();

    let status = handle.get_status().unwrap();
    assert!(!status.iter().any(|e| e.path == "tracked.txt" && e.staged));
}

#[test]
fn discard_hunk_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let hunks = handle
        .get_working_diff(
            "tracked.txt".into(),
            false,
            git_core::diff::DiffOptions::default(),
            None,
        )
        .unwrap();

    handle
        .discard_hunk(
            "tracked.txt".into(),
            hunks[0].old_start,
            hunks[0].new_start,
            HUNK,
        )
        .unwrap();

    let on_disk = std::fs::read_to_string(dir.path().join("tracked.txt")).unwrap();
    assert_eq!(on_disk.replace("\r\n", "\n"), "line one\nline two\n");
}

#[test]
fn a_staged_rename_diffs_old_against_new() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "old.txt", "one\ntwo\nthree\nfour\n");
    commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("old.txt")).unwrap();
    write_file(dir.path(), "new.txt", "one\nTWO\nthree\nfour\n");
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new("old.txt")).unwrap();
    index.add_path(std::path::Path::new("new.txt")).unwrap();
    index.write().unwrap();

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let opts = git_core::diff::DiffOptions::default();
    let hunks = handle
        .get_working_diff("new.txt".into(), true, opts, Some("old.txt".into()))
        .unwrap();
    let edits: Vec<_> = hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.origin != git_core::diff::DiffLineOrigin::Context)
        .map(|l| l.content.clone())
        .collect();
    assert_eq!(edits, vec!["two\n".to_string(), "TWO\n".to_string()]);

    // Without old_path a staged rename is a whole-file add, as before.
    let added = handle
        .get_working_diff("new.txt".into(), true, opts, None)
        .unwrap();
    assert_eq!(added[0].lines.len(), 4);
}

#[test]
fn line_and_file_commands_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "a\nb\nc\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "f.txt", "a\nB\nc\nd\n");
    write_file(dir.path(), "junk.log", "x\n");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let opts = git_core::diff::DiffOptions::default();
    let hunk = handle
        .get_working_diff("f.txt".into(), false, opts, None)
        .unwrap()
        .remove(0);

    let shown = |h: &git_core::diff::DiffHunk| -> Vec<String> {
        h.lines.iter().map(|l| l.content.clone()).collect()
    };
    // A hunk shown with different content (stale view) is refused, nothing changes.
    let stale = handle.stage_lines(
        "f.txt".into(),
        hunk.old_start,
        hunk.new_start,
        HUNK,
        vec![4],
        vec!["not what is on disk\n".into()],
    );
    assert!(stale.unwrap_err().message.contains("hunk not found"));
    assert!(handle
        .get_working_diff("f.txt".into(), true, opts, None)
        .unwrap()
        .is_empty());

    handle
        .stage_lines(
            "f.txt".into(),
            hunk.old_start,
            hunk.new_start,
            HUNK,
            vec![4],
            shown(&hunk),
        )
        .unwrap();
    let staged = handle
        .get_working_diff("f.txt".into(), true, opts, None)
        .unwrap()
        .remove(0);
    handle
        .unstage_lines(
            "f.txt".into(),
            staged.old_start,
            staged.new_start,
            HUNK,
            vec![staged.lines.len() as u32 - 1],
            shown(&staged),
        )
        .unwrap();
    assert!(handle
        .get_working_diff("f.txt".into(), true, opts, None)
        .unwrap()
        .is_empty());

    let hunk = handle
        .get_working_diff("f.txt".into(), false, opts, None)
        .unwrap()
        .remove(0);
    handle
        .discard_lines(
            "f.txt".into(),
            hunk.old_start,
            hunk.new_start,
            HUNK,
            vec![4],
            shown(&hunk),
        )
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt"))
            .unwrap()
            .replace("\r\n", "\n"),
        "a\nB\nc\n"
    );

    handle.ignore_path("*.log".into()).unwrap();
    handle.discard_file("f.txt".into()).unwrap();
    write_file(dir.path(), "u.tmp", "u\n");
    write_file(dir.path(), "late.tmp", "late\n");
    handle.discard_all_unstaged(vec!["u.tmp".into()]).unwrap();
    assert!(!dir.path().join("u.tmp").exists());
    assert!(dir.path().join("late.tmp").exists(), "only listed paths");
    std::fs::remove_file(dir.path().join("late.tmp")).unwrap();
    let status = handle.get_status().unwrap();
    assert!(
        status
            .iter()
            .all(|e| e.path == ".gitignore" || e.path == "junk.log"),
        "{status:?}"
    );

    assert!(handle
        .stage_lines("f.txt".into(), 99, 99, HUNK, vec![0], vec![])
        .is_err());
    assert!(
        handle.get_status().is_ok(),
        "worker survives a failed command"
    );
}

/// #527: the operating system refusing a file access reaches the transports as
/// `ErrorKind::PermissionDenied`, decided from libgit2's typed error in the worker. Skipped
/// where the permission bits don't bind (running as root).
#[cfg(unix)]
#[test]
fn an_unreadable_file_answers_permission_denied() {
    use crate::wire::ErrorKind;
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "base");
    write_file(dir.path(), "secret.txt", "secret");
    let path = dir.path().join("secret.txt");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&path).is_ok() {
        return;
    }

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let staged = handle.stage_file("secret.txt".into());
    let diff = handle.get_working_diff(
        "secret.txt".into(),
        false,
        git_core::diff::DiffOptions::default(),
        None,
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    let error = staged.unwrap_err();
    assert_eq!(error.kind, ErrorKind::PermissionDenied, "{}", error.message);
    let error = diff.unwrap_err();
    assert_eq!(error.kind, ErrorKind::PermissionDenied, "{}", error.message);
    // Another failure keeps `Other`.
    let error = handle.stage_file("../outside.txt".into()).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Other, "{}", error.message);
}
