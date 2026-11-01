mod common;

use common::{commit_all, init_repo, write_file};
use git_core::worktree::{
    create_worktree, list_worktrees, prune_worktrees, remove_worktree, roll_back_destination,
    WorktreeError,
};

#[test]
fn creates_a_linked_worktree_for_an_existing_local_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let linked = dir.path().join("feature-tree");

    let worktrees = list_worktrees(&repo).unwrap();

    assert_eq!(worktrees.len(), 1);
    let main = worktrees.iter().find(|worktree| worktree.is_main).unwrap();
    create_worktree(&repo, "feature-tree", &linked, "feature", None).unwrap();
    let linked = linked.canonicalize().unwrap();
    assert!(list_worktrees(&repo)
        .unwrap()
        .iter()
        .any(|worktree| worktree.path == linked));
    assert!(matches!(
        remove_worktree(&repo, &main.path),
        Err(WorktreeError::MainWorktree)
    ));
}

#[test]
fn lists_the_main_worktree_when_the_git_directory_is_separate() {
    let dir = tempfile::TempDir::new().unwrap();
    let workdir = dir.path().join("working-copy");
    let git_dir = dir.path().join("repository-metadata");
    let mut options = git2::RepositoryInitOptions::new();
    options.no_dotgit_dir(true).workdir_path(&workdir);
    let repo = git2::Repository::init_opts(&git_dir, &options).unwrap();

    let worktrees = list_worktrees(&repo).unwrap();

    assert_eq!(worktrees.len(), 1);
    let main = &worktrees[0];
    assert!(main.is_main);
    assert_eq!(main.path, workdir.canonicalize().unwrap());
}

#[test]
fn creates_a_missing_branch_at_the_requested_start_point() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    let start_point = repo.head().unwrap().peel_to_commit().unwrap().id();
    write_file(dir.path(), "file.txt", "later");
    commit_all(&repo, "later commit");
    let linked = dir.path().join("new-feature-tree");

    create_worktree(
        &repo,
        "new-feature-tree",
        &linked,
        "new-feature",
        Some(&start_point.to_string()),
    )
    .unwrap();

    let linked_repo = git2::Repository::open(&linked).unwrap();
    assert_eq!(linked_repo.head().unwrap().target(), Some(start_point));
}

#[test]
fn rejects_an_existing_worktree_destination_without_removing_it() {
    let (dir, repo) = init_repo();
    let destination = dir.path().join("existing-destination");
    std::fs::create_dir(&destination).unwrap();

    let result = create_worktree(
        &repo,
        "existing-tree",
        &destination,
        "new-feature",
        Some("HEAD"),
    );

    assert!(matches!(result, Err(WorktreeError::PathExists)));
    assert!(destination.exists());
}

#[test]
fn refuses_to_remove_a_dirty_linked_worktree_without_deleting_it() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let linked = dir.path().join("dirty-feature-tree");
    create_worktree(&repo, "dirty-feature-tree", &linked, "feature", None).unwrap();
    write_file(&linked, "untracked.txt", "dirty");

    let result = remove_worktree(&repo, &linked);

    assert!(matches!(result, Err(WorktreeError::Dirty)));
    assert!(linked.exists());
}

#[test]
fn prunes_stale_worktree_metadata_idempotently() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let linked = dir.path().join("stale-feature-tree");
    create_worktree(&repo, "stale-feature-tree", &linked, "feature", None).unwrap();
    let linked = linked.canonicalize().unwrap();
    std::fs::remove_dir_all(&linked).unwrap();
    let stale_worktree = list_worktrees(&repo)
        .unwrap()
        .into_iter()
        .find(|worktree| worktree.path.file_name() == linked.file_name())
        .unwrap();
    assert!(stale_worktree.is_prunable);

    prune_worktrees(&repo).unwrap();
    assert_eq!(list_worktrees(&repo).unwrap().len(), 1);
    prune_worktrees(&repo).unwrap();
    assert_eq!(list_worktrees(&repo).unwrap().len(), 1);
}

/// Adds the branch `evil`: HEAD's tree plus the link `D` and the file `d/x`, built without a
/// file-system link.
fn add_evil_branch(repo: &git2::Repository) {
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let mut inner = repo.treebuilder(None).unwrap();
    inner
        .insert("x", repo.blob(b"x").unwrap(), 0o100644)
        .unwrap();
    let mut outer = repo.treebuilder(Some(&head.tree().unwrap())).unwrap();
    outer.insert("d", inner.write().unwrap(), 0o040000).unwrap();
    outer
        .insert("D", repo.blob(b"/x").unwrap(), 0o120000)
        .unwrap();
    let tree = repo.find_tree(outer.write().unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    let evil = repo
        .commit(None, &sig, &sig, "evil", &tree, &[&head])
        .unwrap();
    repo.branch("evil", &repo.find_commit(evil).unwrap(), false)
        .unwrap();
}

#[test]
fn creating_a_worktree_on_a_branch_with_a_collision_is_refused() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    add_evil_branch(&repo);
    let linked = dir.path().join("evil-tree");

    let error = create_worktree(&repo, "evil-tree", &linked, "evil", None).unwrap_err();
    assert!(matches!(error, WorktreeError::CaseCollision(_)), "{error}");
    assert!(!linked.exists());
    assert!(!repo.path().join("worktrees").join("evil-tree").exists());

    // A new branch at such a start point isn't created either.
    let error = create_worktree(
        &repo,
        "new-tree",
        &dir.path().join("new-tree"),
        "new",
        Some("evil"),
    )
    .unwrap_err();
    assert!(matches!(error, WorktreeError::CaseCollision(_)));
    assert!(repo.find_branch("new", git2::BranchType::Local).is_err());
}

/// A repository whose config says case matters (`core.ignorecase=false`: names differing only
/// in case are different files), with the branch `evil` holding a collision.
#[cfg(target_os = "linux")]
fn case_sensitive_repo_with_evil_branch() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", false)
        .unwrap();
    add_evil_branch(&repo);
    (dir, repo)
}

/// #162: the repository's config says case matters (it was probed where the repository lives),
/// but the worktree's destination folds case (a USB stick, a casefold folder).
#[cfg(target_os = "linux")]
#[test]
fn a_collision_is_refused_when_the_destination_folds_case() {
    let (dir, repo) = case_sensitive_repo_with_evil_branch();
    let linked = dir.path().join("evil-tree");
    let mut probed = None;
    let error =
        git_core::worktree::create_worktree_with(&repo, "evil-tree", &linked, "evil", None, |at| {
            probed = Some(at.to_path_buf());
            Ok(true)
        })
        .unwrap_err();
    assert!(matches!(error, WorktreeError::CaseCollision(_)), "{error}");
    assert_eq!(probed.as_deref(), Some(dir.path()));
    assert!(!linked.exists());
    assert!(!repo.path().join("worktrees").join("evil-tree").exists());

    // A destination that keeps case apart (or a failed probe) takes the tree as before.
    let failing = |_: &std::path::Path| Err(std::io::Error::other("probe failed"));
    git_core::worktree::create_worktree_with(&repo, "evil-tree", &linked, "evil", None, failing)
        .unwrap();
    assert!(linked.join("d").join("x").is_file());
}

/// The probe runs only when it can change the outcome: not when the path already exists, and
/// not when the repository's setting already says case folds.
#[cfg(target_os = "linux")]
#[test]
fn the_destination_is_probed_only_when_it_matters() {
    let (dir, repo) = case_sensitive_repo_with_evil_branch();
    let never = |_: &std::path::Path| -> std::io::Result<bool> { panic!("probed") };
    let error =
        git_core::worktree::create_worktree_with(&repo, "tree", dir.path(), "evil", None, never)
            .unwrap_err();
    assert!(matches!(error, WorktreeError::PathExists), "{error}");

    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    let error = git_core::worktree::create_worktree_with(
        &repo,
        "evil-tree",
        &dir.path().join("evil-tree"),
        "evil",
        None,
        never,
    )
    .unwrap_err();
    assert!(matches!(error, WorktreeError::CaseCollision(_)), "{error}");
}

/// Through the real probe: a Linux temp dir keeps case apart, so the repository's setting
/// decides, as before #162.
#[cfg(target_os = "linux")]
#[test]
fn a_case_sensitive_destination_keeps_the_repository_setting() {
    let (dir, repo) = case_sensitive_repo_with_evil_branch();
    let linked = dir.path().join("evil-tree");
    create_worktree(&repo, "evil-tree", &linked, "evil", None).unwrap();
    assert!(linked.join("d").join("x").is_file());
}

/// A committed repository with its main worktree's branch name.
fn committed_repo() -> (tempfile::TempDir, git2::Repository, String) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    let main_branch = repo.head().unwrap().shorthand().unwrap().to_string();
    (dir, repo, main_branch)
}

fn git(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn has_branch(repo: &git2::Repository, name: &str) -> bool {
    repo.find_branch(name, git2::BranchType::Local).is_ok()
}

#[test]
fn creates_missing_parent_folders_of_the_destination() {
    let (dir, repo, _) = committed_repo();
    let linked = dir.path().join("a").join("b").join("wt");

    create_worktree(&repo, "wt", &linked, "x", Some("HEAD")).unwrap();

    let linked = linked.canonicalize().unwrap();
    assert!(list_worktrees(&repo)
        .unwrap()
        .iter()
        .any(|worktree| worktree.path == linked));
    assert_eq!(
        std::fs::read_to_string(linked.join("file.txt")).unwrap(),
        "initial"
    );

    // git does the same.
    let (twin, _twin_repo, _) = committed_repo();
    let twin_linked = twin.path().join("a").join("b").join("wt");
    let output = git(
        twin.path(),
        &["worktree", "add", twin_linked.to_str().unwrap(), "-b", "x"],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(twin_linked.join("file.txt").is_file());
}

#[test]
fn a_failed_add_removes_the_parent_folders_it_created() {
    let (dir, repo, main_branch) = committed_repo();
    let linked = dir.path().join("a").join("b").join("wt");

    // The main worktree's branch is already checked out, so libgit2 refuses it.
    let result = create_worktree(&repo, "wt", &linked, &main_branch, None);

    assert!(result.is_err());
    assert!(!dir.path().join("a").exists());
    assert!(!repo.path().join("worktrees").join("wt").exists());
}

#[test]
fn a_file_on_the_way_is_a_clear_error() {
    let (dir, repo, _) = committed_repo();
    write_file(dir.path(), "a", "not a folder");
    let linked = dir.path().join("a").join("b").join("wt");

    let result = create_worktree(&repo, "wt", &linked, "x", Some("HEAD"));

    let Err(error @ WorktreeError::CreateFolder { .. }) = result else {
        panic!("expected CreateFolder, got {result:?}");
    };
    let message = error.to_string();
    let folder = dir.path().join("a");
    assert_eq!(
        message,
        format!(
            "Can't create the folder '{}': a file with that name is in the way.",
            folder.display()
        )
    );
    assert!(!has_branch(&repo, "x"));
    assert!(!repo.path().join("worktrees").join("wt").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a")).unwrap(),
        "not a folder"
    );
}

#[cfg(unix)]
#[test]
fn a_failed_add_leaves_no_half_made_worktree_or_new_branch() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let (dir, repo, _) = committed_repo();
    let parent = dir.path().join("p");
    std::fs::create_dir(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o555)).unwrap();
    let linked = parent.join("wt");

    let result = create_worktree(&repo, "wt", &linked, "x", Some("HEAD"));

    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert!(!repo.path().join("worktrees").join("wt").exists());
    assert!(!has_branch(&repo, "x"));
    assert!(!linked.exists());

    create_worktree(&repo, "wt", &linked, "x", Some("HEAD")).unwrap();
    assert!(linked.join("file.txt").is_file());
}

#[test]
fn a_failed_add_keeps_an_empty_parent_folder_it_did_not_create() {
    let (dir, repo, main_branch) = committed_repo();
    let existing = dir.path().join("p");
    std::fs::create_dir(&existing).unwrap();
    let linked = existing.join("q").join("wt");

    // The main worktree's branch is already checked out, so libgit2 refuses it.
    let result = create_worktree(&repo, "wt", &linked, &main_branch, None);

    assert!(result.is_err());
    assert!(existing.is_dir());
    assert!(!existing.join("q").exists());
}

/// A stale `.git/worktrees/<name>` that libgit2 refuses to reuse.
fn stale_worktree_entry(repo: &git2::Repository) -> std::path::PathBuf {
    let admin = repo.path().join("worktrees").join("wt");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::write(admin.join("marker"), "kept").unwrap();
    admin
}

#[test]
fn a_failed_add_keeps_a_worktree_entry_it_did_not_create() {
    let (dir, repo, _) = committed_repo();
    let admin = stale_worktree_entry(&repo);
    let linked = dir.path().join("a").join("wt");

    let result = create_worktree(&repo, "wt", &linked, "x", Some("HEAD"));

    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(admin.join("marker")).unwrap(),
        "kept"
    );
    assert!(!has_branch(&repo, "x"));
    assert!(!dir.path().join("a").exists());
}

#[test]
fn a_failed_add_keeps_a_branch_it_did_not_create() {
    let (dir, repo, _) = committed_repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    stale_worktree_entry(&repo);
    let linked = dir.path().join("a").join("wt");

    let result = create_worktree(&repo, "wt", &linked, "feature", None);

    assert!(result.is_err());
    assert!(has_branch(&repo, "feature"));
    assert!(!dir.path().join("a").exists());
}

/// Commits HEAD's tree plus a file called `name` on a new branch `bad`, leaving HEAD alone. A
/// name the file system can't hold makes the checkout of a worktree on `bad` fail (#245): one
/// that's too long fails while writing it, after `file.txt` was written; on Windows a reserved
/// name (`aux`) or a trailing dot (`a.`) is refused while the checkout is planned
/// (`checkout_verify_paths`), before any file is written.
fn branch_with_unwritable_name(repo: &git2::Repository, name: &str) {
    branch_with_entries(repo, &[(name.as_bytes(), Entry::File)]);
}

enum Entry<'a> {
    File,
    /// A folder holding one file of this name.
    Folder(&'a str),
}

/// Commits HEAD's tree plus `entries` on a new branch `bad`, leaving HEAD alone.
fn branch_with_entries(repo: &git2::Repository, entries: &[(&[u8], Entry<'_>)]) {
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let blob = repo.blob(b"unwritable").unwrap();
    let mut builder = repo.treebuilder(Some(&head.tree().unwrap())).unwrap();
    for (name, entry) in entries {
        match entry {
            Entry::File => builder.insert(name.to_vec(), blob, 0o100644).unwrap(),
            Entry::Folder(file) => {
                let mut folder = repo.treebuilder(None).unwrap();
                folder.insert(*file, blob, 0o100644).unwrap();
                builder
                    .insert(name.to_vec(), folder.write().unwrap(), 0o040000)
                    .unwrap()
            }
        };
    }
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let signature = git2::Signature::now("Test", "test@example.com").unwrap();
    let commit = repo
        .commit(None, &signature, &signature, "bad", &tree, &[&head])
        .unwrap();
    repo.branch("bad", &repo.find_commit(commit).unwrap(), false)
        .unwrap();
}

/// A name longer than any file system allows for one component.
fn too_long_name() -> String {
    "x".repeat(300)
}

/// The add failed with [`WorktreeError::AddFailed`] and left no entry and no folder.
fn assert_add_failed_and_nothing_left(
    result: Result<(), WorktreeError>,
    repo: &git2::Repository,
    linked: &std::path::Path,
) {
    let Err(error @ WorktreeError::AddFailed { .. }) = result else {
        panic!("expected AddFailed, got {result:?}");
    };
    let message = error.to_string();
    assert!(
        message.starts_with("Couldn't create the worktree: "),
        "{message}"
    );
    assert!(!message.contains("git operation failed"), "{message}");
    assert!(!message.ends_with(".."), "{message}");
    assert!(!repo.path().join("worktrees").join("wt").exists());
    assert!(!linked.exists());
    assert_eq!(list_worktrees(repo).unwrap().len(), 1);
}

#[test]
fn a_checkout_failure_is_rolled_back() {
    let (dir, repo, _) = committed_repo();
    branch_with_unwritable_name(&repo, &too_long_name());
    let linked = dir.path().join("wt");

    let result = create_worktree(&repo, "wt", &linked, "newb", Some("bad"));

    assert_add_failed_and_nothing_left(result, &repo, &linked);
    assert!(!has_branch(&repo, "newb"));
    create_worktree(&repo, "wt", &linked, "good", Some("HEAD")).unwrap();
    assert!(linked.join("file.txt").is_file());

    // git removes the destination and the entry too, and keeps the branch it created.
    let (twin, twin_repo, _) = committed_repo();
    branch_with_unwritable_name(&twin_repo, &too_long_name());
    let twin_linked = twin.path().join("wt");
    let output = git(
        twin.path(),
        &[
            "worktree",
            "add",
            "-b",
            "newb",
            twin_linked.to_str().unwrap(),
            "bad",
        ],
    );
    assert!(!output.status.success(), "{output:?}");
    assert!(!twin_linked.exists());
    assert!(!twin_repo.path().join("worktrees").join("wt").exists());
    assert!(has_branch(&twin_repo, "newb"));
}

#[test]
fn a_checkout_failure_on_an_existing_branch_frees_it() {
    let (dir, repo, _) = committed_repo();
    branch_with_unwritable_name(&repo, &too_long_name());
    let linked = dir.path().join("wt");

    let result = create_worktree(&repo, "wt", &linked, "bad", None);

    assert_add_failed_and_nothing_left(result, &repo, &linked);
    let mut bad = repo.find_branch("bad", git2::BranchType::Local).unwrap();
    bad.delete().unwrap();
}

#[test]
fn a_checkout_failure_in_missing_parents_removes_them() {
    let (dir, repo, _) = committed_repo();
    branch_with_unwritable_name(&repo, &too_long_name());
    let linked = dir.path().join("a").join("b").join("wt");

    let result = create_worktree(&repo, "wt", &linked, "newb", Some("bad"));

    assert_add_failed_and_nothing_left(result, &repo, &linked);
    assert!(!dir.path().join("a").exists());
    assert!(!has_branch(&repo, "newb"));
}

/// The too-long name is in a folder: the folder checked out for it is removed too.
#[test]
fn a_checkout_failure_in_a_nested_folder_is_rolled_back() {
    let (dir, repo, _) = committed_repo();
    let long = too_long_name();
    branch_with_entries(&repo, &[(b"zdir", Entry::Folder(&long))]);
    let linked = dir.path().join("wt");

    let result = create_worktree(&repo, "wt", &linked, "newb", Some("bad"));

    assert_add_failed_and_nothing_left(result, &repo, &linked);
    assert!(!has_branch(&repo, "newb"));
    create_worktree(&repo, "wt", &linked, "good", Some("HEAD")).unwrap();
}

/// The work tree walker takes UTF-8 paths, so a checked-out file whose name isn't UTF-8 is not
/// removed: the folder is kept and named; the entry and the new branch still go.
#[cfg(target_os = "linux")]
#[test]
fn a_checked_out_file_with_a_non_utf8_name_keeps_the_folder() {
    let (dir, repo, _) = committed_repo();
    let long = too_long_name();
    // Sorted before the too-long name, so it is written before the checkout fails.
    branch_with_entries(
        &repo,
        &[(b"a\x80", Entry::File), (long.as_bytes(), Entry::File)],
    );
    let linked = dir.path().join("wt");

    let result = create_worktree(&repo, "wt", &linked, "newb", Some("bad"));

    let Err(WorktreeError::AddFailedFolderLeft { ref path, .. }) = result else {
        panic!("expected AddFailedFolderLeft, got {result:?}");
    };
    assert_eq!(path, &linked);
    assert!(!linked.join("file.txt").exists());
    assert!(!linked.join(".git").exists());
    assert!(!repo.path().join("worktrees").join("wt").exists());
    assert!(!has_branch(&repo, "newb"));
}

#[test]
fn an_admin_folder_naming_another_destination_is_left_alone() {
    let (dir, repo, _) = committed_repo();
    branch_with_unwritable_name(&repo, &too_long_name());
    let admin = stale_worktree_entry(&repo);
    let elsewhere = dir.path().join("elsewhere").join(".git");
    std::fs::write(admin.join("gitdir"), elsewhere.to_str().unwrap()).unwrap();
    let linked = dir.path().join("wt");

    let result = create_worktree(&repo, "wt", &linked, "newb", Some("bad"));

    assert!(
        matches!(result, Err(WorktreeError::AddFailed { .. })),
        "{result:?}"
    );
    assert_eq!(
        std::fs::read_to_string(admin.join("marker")).unwrap(),
        "kept"
    );
    assert_eq!(
        std::fs::read_to_string(admin.join("gitdir")).unwrap(),
        elsewhere.to_str().unwrap()
    );
    assert!(!linked.exists());
    assert!(!has_branch(&repo, "newb"));
}

/// What a checkout of HEAD wrote into `folder` before it failed: the tree's files and the `.git`
/// file pointing at the worktree entry.
fn half_made_worktree(repo: &git2::Repository, folder: &std::path::Path) -> git2::Oid {
    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    std::fs::create_dir(folder).unwrap();
    write_file(folder, "file.txt", "initial");
    std::fs::write(folder.join(".git"), "gitdir: /somewhere/worktrees/wt\n").unwrap();
    tree.id()
}

#[test]
fn rolling_back_removes_the_checked_out_files_and_the_folder() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    write_file(dir.path(), "d/e/nested.txt", "nested");
    commit_all(&repo, "initial commit");
    let linked = dir.path().join("wt");
    let tree = repo.find_tree(half_made_worktree(&repo, &linked)).unwrap();
    write_file(&linked, "d/e/nested.txt", "nested");

    let error = roll_back_destination(&repo, &linked, &tree, "disk full".to_string());

    assert!(
        matches!(error, WorktreeError::AddFailed { .. }),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "Couldn't create the worktree: disk full."
    );
    assert!(!linked.exists());
}

#[test]
fn a_foreign_file_in_the_new_folder_is_kept() {
    let (dir, repo, _) = committed_repo();
    let linked = dir.path().join("wt");
    let tree = repo.find_tree(half_made_worktree(&repo, &linked)).unwrap();
    write_file(&linked, "zzz-foreign", "not ours");

    let error = roll_back_destination(&repo, &linked, &tree, "disk full".to_string());

    let WorktreeError::AddFailedFolderLeft { ref path, .. } = error else {
        panic!("expected AddFailedFolderLeft, got {error:?}");
    };
    assert_eq!(path, &linked);
    assert_eq!(
        error.to_string(),
        format!(
            "Couldn't create the worktree: disk full. Its folder '{}' couldn't be removed \
             completely; delete it by hand.",
            linked.display()
        )
    );
    assert!(!linked.join("file.txt").exists());
    assert!(!linked.join(".git").exists());
    assert_eq!(
        std::fs::read_to_string(linked.join("zzz-foreign")).unwrap(),
        "not ours"
    );
}

/// Adds a worktree on a new branch from `bad` (see [`branch_with_unwritable_name`]) and checks
/// that nothing of it is left: no entry, no folder, no new branch, and the same name works again.
#[cfg(windows)]
fn assert_an_unwritable_name_is_rolled_back(name: &str) {
    let (dir, repo, _) = committed_repo();
    branch_with_unwritable_name(&repo, name);
    let linked = dir.path().join("wt");

    let result = create_worktree(&repo, "wt", &linked, "newb", Some("bad"));

    assert_add_failed_and_nothing_left(result, &repo, &linked);
    assert!(!has_branch(&repo, "newb"));
    create_worktree(&repo, "wt", &linked, "good", Some("HEAD")).unwrap();
    assert!(linked.join("file.txt").is_file());
}

#[cfg(windows)]
#[test]
fn a_reserved_name_in_the_branch_is_rolled_back() {
    assert_an_unwritable_name_is_rolled_back("aux");
}

#[cfg(windows)]
#[test]
fn a_trailing_dot_in_the_branch_is_rolled_back() {
    assert_an_unwritable_name_is_rolled_back("a.");
}
