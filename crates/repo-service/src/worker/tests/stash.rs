use super::*;

#[test]
fn save_then_list_stash_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .save_stash(git_core::stash::StashOptions::default())
        .unwrap();

    let stashes = handle.list_stashes().unwrap();
    assert_eq!(stashes.len(), 1);
}

#[test]
fn apply_then_drop_stash_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .save_stash(git_core::stash::StashOptions::default())
        .unwrap();
    let id = handle.list_stashes().unwrap()[0].commit_id.clone();

    assert_eq!(
        handle.apply_stash(0, id.clone(), false).unwrap(),
        git_core::stash::StashApplyOutcome::Applied
    );
    let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(contents, "v2");

    handle.drop_stash(0, id).unwrap();
    assert!(handle.list_stashes().unwrap().is_empty());
}

#[test]
fn stash_commands_refuse_a_stale_stash_id_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .save_stash(git_core::stash::StashOptions::default())
        .unwrap();
    let stale = handle.list_stashes().unwrap()[0].commit_id.clone();
    write_file(dir.path(), "file.txt", "v3");
    handle
        .save_stash(git_core::stash::StashOptions::default())
        .unwrap();

    let expected = "The stash list changed; refresh and try again.".to_string();
    assert_eq!(
        handle.apply_stash(0, stale.clone(), true),
        Err(expected.clone())
    );
    assert_eq!(
        handle.pop_stash(0, stale.clone(), true),
        Err(expected.clone())
    );
    assert_eq!(handle.drop_stash(0, stale), Err(expected));
    assert_eq!(handle.list_stashes().unwrap().len(), 2);
}

#[test]
fn stash_options_pop_and_files_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "v1");
    write_file(dir.path(), "b.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "a.txt", "mine");
    write_file(dir.path(), "b.txt", "other");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    handle
        .save_stash(git_core::stash::StashOptions {
            message: Some("only a".into()),
            paths: vec!["a.txt".into()],
            ..Default::default()
        })
        .unwrap();

    let stashes = handle.list_stashes().unwrap();
    assert!(stashes[0].message.ends_with(": only a"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "other"
    );
    let files = handle
        .get_stash_files(stashes[0].commit_id.clone())
        .unwrap();
    assert_eq!(files.tracked, vec!["a.txt".to_string()]);
    assert_eq!(files.untracked_commit_id, None);
    assert_eq!(
        handle
            .pop_stash(0, stashes[0].commit_id.clone(), true)
            .unwrap(),
        git_core::stash::StashApplyOutcome::Applied
    );
    assert!(handle.list_stashes().unwrap().is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "mine"
    );
}
