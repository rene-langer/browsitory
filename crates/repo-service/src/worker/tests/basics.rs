use super::*;

#[test]
fn get_status_reflects_an_untracked_file() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "untracked.txt", "hello");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let entries = worker.handle().get_status().unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "untracked.txt");
}

#[test]
fn spawn_fails_on_a_non_repository_path() {
    let dir = TempDir::new().unwrap();

    let result = Worker::spawn(dir.path().to_path_buf());

    assert!(result.is_err());
}

#[test]
fn get_commit_graph_reflects_a_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "hello");
    commit_all(&repo, "initial commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let commits = worker
        .handle()
        .get_commit_graph(10, git_core::graph::GraphSelection::default())
        .unwrap();

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].summary, "initial commit");
}

#[test]
fn locate_commit_finds_a_commit_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "hello");
    commit_all(&repo, "initial commit");
    let id = repo.head().unwrap().target().unwrap().to_string();

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let found = handle
        .locate_commit(
            id[..7].to_string(),
            git_core::graph::GraphSelection::default(),
        )
        .unwrap()
        .unwrap();
    let missing = handle
        .locate_commit(
            "no-such-branch".into(),
            git_core::graph::GraphSelection::default(),
        )
        .unwrap();

    assert_eq!(found.commit_id, id);
    assert_eq!(found.position, Some(0));
    assert_eq!(missing, None);
}

#[test]
fn commit_stats_count_a_commit_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "one\ntwo\n");
    commit_all(&repo, "initial commit");
    let id = repo.head().unwrap().target().unwrap().to_string();

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let entries = handle.get_commit_stats(vec![id.clone()], true).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].commit_id, id);
    let stats = entries[0].stats.as_ref().unwrap();
    assert_eq!(stats.files_changed, 1);
    assert_eq!(stats.insertions, 2);
    assert_eq!(stats.deletions, 0);
    assert_eq!(stats.files[0].path, "file.txt");
}

#[test]
fn stage_then_commit_round_trips_through_the_worker() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "new.txt", "hello");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("new.txt".into()).unwrap();
    let result = handle.commit("message".into(), false, Vec::new(), mpsc::channel().0);

    assert!(result.is_ok());
    assert!(handle.get_status().unwrap().is_empty());
}

#[test]
fn get_commit_signature_reports_unsigned_for_a_plain_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "plain");
    let id = repo.head().unwrap().target().unwrap().to_string();

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let signature = worker.handle().get_commit_signature(id).unwrap();

    assert_eq!(
        signature.status,
        git_core::signing::SignatureStatus::Unsigned
    );
}

#[test]
fn workdir_is_the_work_tree_root_even_when_opened_from_a_subdirectory() {
    let (dir, repo) = init_repo();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    write_file(dir.path(), "sub/a.txt", "a");
    commit_all(&repo, "initial");
    let worker = Worker::spawn(dir.path().join("sub")).unwrap();
    assert_eq!(
        worker.handle().workdir().unwrap().canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
}

#[test]
fn external_tool_commands_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "old");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "new");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    assert_eq!(
        handle
            .read_file_version("a.txt".into(), git_core::tools::FileVersion::Index)
            .unwrap(),
        Some(b"old".to_vec())
    );
    // Hermetic error paths: both are checked before any git config is read.
    assert_eq!(
        handle
            .prepare_diff_tool("../a.txt".into(), git_core::tools::DiffSource::Unstaged)
            .unwrap_err(),
        crate::wire::ErrorDto::from("Invalid path.")
    );
    assert_eq!(
        handle.prepare_merge_tool("a.txt".into()).unwrap_err(),
        crate::wire::ErrorDto::from("no conflict found for path 'a.txt'")
    );
    assert!(handle.get_git_tools().is_ok());
    assert_eq!(
        handle.check_merge_target("a.txt".into()).unwrap_err(),
        crate::wire::ErrorDto::from("no conflict found for path 'a.txt'")
    );
}

#[test]
fn mark_resolved_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    let main = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "a.txt", "theirs\n");
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    write_file(dir.path(), "a.txt", "ours\n");
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.check_merge_target("a.txt".into()).unwrap();
    assert_eq!(
        handle.mark_resolved("a.txt".into(), false).unwrap(),
        git_core::merge::MarkResolvedOutcome::MarkersRemain
    );
    assert_eq!(
        handle.mark_resolved("a.txt".into(), true).unwrap(),
        git_core::merge::MarkResolvedOutcome::Resolved
    );
}

#[test]
fn whole_file_resolution_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    let main = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "a.txt", "theirs\n");
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    write_file(dir.path(), "a.txt", "ours\n");
    commit_all(&repo, "main");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions::default(),
            no_events(),
        )
        .unwrap();
    handle
        .resolve_whole_file("a.txt".into(), git_core::merge::ConflictSide::Theirs)
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "theirs\n"
    );
    assert!(!repo.index().unwrap().has_conflicts());
}

#[test]
fn root_files_are_listed_and_created_through_the_worker() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), ".gitattributes", "* text=auto\n");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(handle.root_files(), Some(vec![".gitattributes".into()]));

    handle.create_root_file(".gitignore".into()).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
        ""
    );
    handle.create_root_file(".gitattributes".into()).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".gitattributes")).unwrap(),
        "* text=auto\n",
        "an existing file is left as it is"
    );
    assert_eq!(
        handle.create_root_file("notes.txt".into()).unwrap_err(),
        crate::wire::ErrorDto::from("Only .gitignore and .gitattributes can be created.")
    );
    assert!(!dir.path().join("notes.txt").exists());
}

#[test]
fn a_bare_repository_has_no_root_files() {
    let dir = TempDir::new().unwrap();
    Repository::init_bare(dir.path()).unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(handle.root_files(), None);
    assert!(handle.create_root_file(".gitignore".into()).is_err());
    assert!(!dir.path().join(".gitignore").exists());
}
