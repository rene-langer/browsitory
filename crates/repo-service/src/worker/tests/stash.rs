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
        Err(crate::wire::ErrorDto::from(expected.clone()))
    );
    assert_eq!(
        handle.pop_stash(0, stale.clone(), true),
        Err(crate::wire::ErrorDto::from(expected.clone()))
    );
    assert_eq!(
        handle.drop_stash(0, stale),
        Err(crate::wire::ErrorDto::from(expected))
    );
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

/// #1070: a stash never changes, so its file count is worked out once per worker, not on every
/// refresh. Seen through the object store: once listed, the stash's untracked tree can go
/// missing and the list still answers with the same counts. (The tree is over libgit2's 4 KiB
/// object cache limit, so an uncached walk would have to read it again.)
#[test]
fn list_stashes_reads_a_stash_s_untracked_tree_only_once() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2");
    for n in 0..300 {
        write_file(dir.path(), &format!("untracked-{n:04}.txt"), "new");
    }
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .save_stash(git_core::stash::StashOptions::default())
        .unwrap();
    let first = handle.list_stashes().unwrap();
    assert_eq!(first[0].file_count, 301);

    let stash = repo
        .find_commit(git2::Oid::from_str(&first[0].commit_id).unwrap())
        .unwrap();
    let untracked_tree = stash.parent(2).unwrap().tree_id().to_string();
    let loose = repo
        .path()
        .join("objects")
        .join(&untracked_tree[..2])
        .join(&untracked_tree[2..]);
    let mut permissions = std::fs::metadata(&loose).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&loose, permissions).unwrap();
    std::fs::remove_file(&loose).unwrap();

    write_file(dir.path(), "file.txt", "v3");
    handle
        .save_stash(git_core::stash::StashOptions::default())
        .unwrap();
    let again = handle.list_stashes().unwrap();
    assert_eq!(again.len(), 2);
    assert_eq!(again[0].file_count, 1);
    assert_eq!(again[1].commit_id, first[0].commit_id);
    assert_eq!(again[1].file_count, 301);
    assert_eq!(again[1].has_staged_changes, first[0].has_staged_changes);
}
