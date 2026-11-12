use super::*;

#[test]
fn list_branches_reflects_the_initial_branch_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let branches = worker.handle().list_branches().unwrap();

    assert_eq!(branches.len(), 1);
    assert!(branches[0].is_current);
}

#[test]
fn create_then_remove_worktree_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    let linked = dir.path().join("feature-tree");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(handle.list_worktrees().unwrap().len(), 1);
    commit_all(&repo, "initial commit");
    handle
        .create_worktree(
            "feature-tree".into(),
            linked.clone(),
            "feature".into(),
            Some("HEAD".into()),
        )
        .unwrap();

    assert!(handle
        .list_worktrees()
        .unwrap()
        .iter()
        .any(|worktree| worktree.name == "feature-tree"));

    handle.remove_worktree("feature-tree".into()).unwrap();

    assert!(!linked.exists());
    assert_eq!(handle.list_worktrees().unwrap().len(), 1);
}

#[test]
fn initializes_and_updates_a_submodule_through_the_worker() {
    let dir = TempDir::new().expect("create parent directory");
    let child_path = dir.path().join("child-source");
    let child = Repository::init(&child_path).expect("init child repository");
    {
        let mut config = child.config().expect("child config");
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    write_file(&child_path, "child.txt", "initial child commit");
    commit_all(&child, "child commit");

    let parent_path = dir.path().join("parent-source");
    let parent = Repository::init(&parent_path).expect("init parent repository");
    {
        let mut config = parent.config().expect("parent config");
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    let mut submodule = parent
        .submodule(
            child_path.to_str().expect("child path"),
            Path::new("deps/child"),
            true,
        )
        .expect("configure submodule");
    submodule.clone(None).expect("clone submodule");
    submodule.add_to_index(true).expect("stage submodule");
    submodule.add_finalize().expect("finalize submodule");
    drop(submodule);
    commit_all(&parent, "add child submodule");

    let checkout_path = dir.path().join("checkout");
    Repository::clone(parent_path.to_str().expect("parent path"), &checkout_path)
        .expect("clone parent checkout");
    let worker = Worker::spawn(checkout_path).expect("start worker");
    let handle = worker.handle();

    let before = handle.list_submodules().expect("list submodules");
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].path, "deps/child");
    assert!(!before[0].initialized);

    handle
        .init_submodule("deps/child".into())
        .expect("initialize submodule");
    handle
        .update_submodule("deps/child".into(), false)
        .expect("update submodule");

    let after = handle
        .list_submodules()
        .expect("list initialized submodule");
    assert!(after[0].initialized);
    assert!(after[0].head_id.is_some());
}

#[test]
fn lists_and_restores_head_reflog_entries_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first commit");
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "second commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).expect("start worker");
    let handle = worker.handle();

    assert!(handle
        .list_reflog_refs()
        .expect("list reflog references")
        .iter()
        .any(|reference| reference == "HEAD"));
    let entries = handle.get_reflog("HEAD".into()).expect("read HEAD reflog");
    assert_eq!(entries[0].summary.as_deref(), Some("second commit"));
    let first_commit = entries[1].new_id.clone();

    handle
        .restore_reflog_entry("HEAD".into(), first_commit.clone())
        .expect("restore prior HEAD reflog entry");

    assert_eq!(
        repo.head()
            .expect("read HEAD")
            .target()
            .expect("HEAD target")
            .to_string(),
        first_commit
    );
}

#[test]
fn refuses_to_remove_the_worktree_open_in_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let linked = dir.path().join("feature-tree");
    git_core::worktree::create_worktree(&repo, "feature-tree", &linked, "feature", None).unwrap();
    let worker = Worker::spawn(linked.clone()).unwrap();

    let result = worker.handle().remove_worktree("feature-tree".into());

    assert!(result.is_err());
    assert!(linked.exists());
    assert!(worker.handle().get_status().is_ok());
}

#[test]
fn remove_worktree_ignores_the_synthetic_main_row_when_names_collide() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let linked = dir.path().join("linked-called-main");
    git_core::worktree::create_worktree(&repo, "main", &linked, "feature", None).unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

    let result = worker.handle().remove_worktree("main".into());

    assert!(result.is_ok());
    assert!(!linked.exists());
}

/// #252: a worktree whose folder is gone is removed by name, and doesn't stop another one
/// from being removed.
#[test]
fn removes_worktrees_by_name_when_one_folder_is_missing() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let kept = dir.path().join("kept");
    let gone = dir.path().join("gone");
    git_core::worktree::create_worktree(&repo, "kept", &kept, "kept", Some("HEAD")).unwrap();
    git_core::worktree::create_worktree(&repo, "gone", &gone, "gone", Some("HEAD")).unwrap();
    std::fs::remove_dir_all(&gone).unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    handle.remove_worktree("kept".into()).unwrap();
    assert!(!kept.exists());
    handle.remove_worktree("gone".into()).unwrap();

    assert_eq!(handle.list_worktrees().unwrap().len(), 1);
}

/// Two entries can name one folder (an old one whose folder was deleted by hand, and a new
/// one made there): Remove acts on the entry with that name, never the other one.
#[test]
fn removes_the_named_entry_when_two_name_the_same_folder() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let folder = dir.path().join("shared");
    git_core::worktree::create_worktree(&repo, "a-old", &folder, "a-old", Some("HEAD")).unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
    git_core::worktree::create_worktree(&repo, "b-new", &folder, "b-new", Some("HEAD")).unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let names = |handle: &super::super::WorkerHandle| -> Vec<String> {
        handle
            .list_worktrees()
            .unwrap()
            .into_iter()
            .map(|worktree| worktree.name)
            .collect()
    };
    assert!(names(&handle).contains(&"b-new".to_string()));
    assert!(names(&handle).contains(&"a-old".to_string()));

    // The old entry's folder is the new worktree now: it is kept.
    assert!(handle.remove_worktree("a-old".into()).is_err());
    assert!(folder.join("file.txt").is_file());

    handle.remove_worktree("b-new".into()).unwrap();
    assert!(!folder.exists());
    assert!(!names(&handle).contains(&"b-new".to_string()));
    handle.remove_worktree("a-old".into()).unwrap();
    assert_eq!(names(&handle), ["main"]);
}

/// A bare repository with linked worktrees: they are listed and removed by name.
#[test]
fn removes_a_worktree_of_a_bare_repository() {
    let dir = TempDir::new().unwrap();
    let bare = dir.path().join("bare.git");
    let repo = Repository::init_bare(&bare).unwrap();
    {
        let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
        let tree = repo.treebuilder(None).unwrap().write().unwrap();
        let tree = repo.find_tree(tree).unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
    }
    let linked = dir.path().join("linked");
    git_core::worktree::create_worktree(&repo, "linked", &linked, "feature", Some("HEAD")).unwrap();
    let worker = Worker::spawn(bare).unwrap();
    let handle = worker.handle();
    assert_eq!(handle.list_worktrees().unwrap().len(), 2);

    handle.remove_worktree("linked".into()).unwrap();

    assert!(!linked.exists());
    assert_eq!(handle.list_worktrees().unwrap().len(), 1);
}

#[test]
fn create_then_switch_branch_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .create_branch("feature".into(), "HEAD".into(), true)
        .unwrap();

    let branches = handle.list_branches().unwrap();
    let feature = branches.iter().find(|b| b.name == "feature").unwrap();
    assert!(feature.is_current);

    let initial_branch_name = branches
        .iter()
        .find(|b| b.name != "feature")
        .unwrap()
        .name
        .clone();
    handle
        .switch_branch(initial_branch_name.clone(), no_events())
        .unwrap();

    let branches_after = handle.list_branches().unwrap();
    assert!(
        branches_after
            .iter()
            .find(|b| b.name == initial_branch_name)
            .unwrap()
            .is_current
    );
}

#[test]
fn rename_branch_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let initial_branch_name = handle.list_branches().unwrap()[0].name.clone();

    handle
        .rename_branch(initial_branch_name, "renamed".into())
        .unwrap();

    let branches = handle.list_branches().unwrap();
    assert_eq!(branches[0].name, "renamed");
}

#[test]
fn delete_branch_with_force_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .create_branch("feature".into(), "HEAD".into(), true)
        .unwrap();
    let initial_branch_name = handle
        .list_branches()
        .unwrap()
        .into_iter()
        .find(|b| b.name != "feature")
        .unwrap()
        .name;
    handle
        .switch_branch(initial_branch_name, no_events())
        .unwrap();

    handle.delete_branch("feature".into(), true).unwrap();

    assert!(!handle
        .list_branches()
        .unwrap()
        .iter()
        .any(|b| b.name == "feature"));
}

#[test]
fn the_branch_list_and_upstreams_count_a_moved_upstream_again_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("topic", &base, false).unwrap();
    repo.reference("refs/remotes/origin/topic", base.id(), true, "test")
        .unwrap();
    repo.remote("origin", "https://example.com/owner/repo.git")
        .unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("branch.topic.remote", "origin").unwrap();
    config
        .set_str("branch.topic.merge", "refs/heads/topic")
        .unwrap();
    drop(config);
    let counts = |handle: &crate::worker::WorkerHandle| {
        let branch = handle
            .list_branches()
            .unwrap()
            .into_iter()
            .find(|branch| branch.name == "topic")
            .unwrap();
        let upstream = handle
            .get_remote_upstreams("origin".into())
            .unwrap()
            .into_iter()
            .find(|upstream| upstream.local_branch == "topic")
            .unwrap();
        assert_eq!(
            (branch.ahead, branch.behind),
            (upstream.ahead, upstream.behind)
        );
        (branch.ahead, branch.behind)
    };

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(counts(&handle), (Some(0), Some(0)));

    let tree = base.tree().unwrap();
    let sig = repo.signature().unwrap();
    let ahead = repo
        .commit(None, &sig, &sig, "fetched", &tree, &[&base])
        .unwrap();
    repo.reference("refs/remotes/origin/topic", ahead, true, "fetch")
        .unwrap();

    assert_eq!(counts(&handle), (Some(0), Some(1)));
}
