mod common;

use std::path::{Path, PathBuf};

use common::{commit_all, init_repo, write_file};
use git_core::worktree::{
    create_worktree, list_worktrees, prune_worktrees, remove_worktree, remove_worktree_named,
    resolve_destination, roll_back_destination, WorktreeError,
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

use test_support::git;

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

/// Checked-out files and folders whose names aren't UTF-8 (possible on Linux) are removed like
/// any others, and so is the destination (#262).
// Linux only, not all of Unix: macOS (APFS) refuses a name that isn't UTF-8 (EILSEQ), so the
// checkout would fail on `a\x80` itself.
#[cfg(target_os = "linux")]
#[test]
fn a_checked_out_file_with_a_non_utf8_name_is_rolled_back() {
    let (dir, repo, _) = committed_repo();
    let long = too_long_name();
    // Sorted before the too-long name, so they are written before the checkout fails.
    branch_with_entries(
        &repo,
        &[
            (b"a\x80", Entry::File),
            (b"b\xff", Entry::Folder("c")),
            (long.as_bytes(), Entry::File),
        ],
    );
    let linked = dir.path().join("wt");

    let result = create_worktree(&repo, "wt", &linked, "newb", Some("bad"));

    // The checkout stops at its first failure, so a failure on the too-long name (sorted last)
    // means `a\x80` and `b\xff/c` were written, and the rollback had them to remove.
    let Err(WorktreeError::AddFailed { ref reason }) = result else {
        panic!("expected AddFailed, got {result:?}");
    };
    assert!(reason.contains(&long), "{reason}");
    assert_add_failed_and_nothing_left(result, &repo, &linked);
    assert!(!has_branch(&repo, "newb"));
}

/// The rollback itself, on files that are certainly there: a file and a folder whose names
/// aren't UTF-8 are removed, and so is the folder (#262).
#[cfg(target_os = "linux")]
#[test]
fn rolling_back_removes_files_and_folders_whose_names_are_not_utf8() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let (dir, repo, _) = committed_repo();
    branch_with_entries(
        &repo,
        &[(b"a\x80", Entry::File), (b"b\xff", Entry::Folder("c"))],
    );
    let bad = repo.find_branch("bad", git2::BranchType::Local).unwrap();
    let tree = bad.get().peel_to_tree().unwrap();
    let linked = dir.path().join("wt");
    half_made_worktree(&repo, &linked);
    let file = linked.join(OsStr::from_bytes(b"a\x80"));
    let folder = linked.join(OsStr::from_bytes(b"b\xff"));
    std::fs::write(&file, "unwritable").unwrap();
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("c"), "unwritable").unwrap();
    assert!(file.is_file() && folder.join("c").is_file());

    let error = roll_back_destination(&repo, &linked, &tree, "disk full".to_string());

    assert!(
        matches!(error, WorktreeError::AddFailed { .. }),
        "{error:?}"
    );
    assert!(!linked.exists());
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

/// The names of the entries in `dir`, as stored (a trailing dot or space kept).
fn entry_names(dir: &Path) -> Vec<std::ffi::OsString> {
    std::fs::read_dir(dir)
        .map(|entries| entries.map(|entry| entry.unwrap().file_name()).collect())
        .unwrap_or_default()
}

/// On Windows, libgit2 would create a name ending in a dot or a space literally while Windows
/// paths elsewhere drop it, so the worktree couldn't be found or removed (#261); a reserved
/// device name can't be a folder at all. Such a name or destination is refused before anything
/// is created.
#[cfg(windows)]
#[test]
fn a_name_or_destination_windows_cannot_use_is_refused() {
    let (dir, repo, _) = committed_repo();
    let before = entry_names(dir.path());
    let cases = [
        ("wt.", dir.path().join("wt")),
        ("wt ", dir.path().join("wt")),
        ("aux", dir.path().join("wt")),
        ("wt", dir.path().join("wt.")),
        ("wt", dir.path().join("wt ")),
        ("wt", dir.path().join("nul.txt")),
        ("wt", dir.path().join("a.").join("wt")),
        ("wt", PathBuf::from("wt.")),
        ("wt", PathBuf::from(r"a \wt")),
    ];
    for (name, path) in cases {
        let result = create_worktree(&repo, name, &path, "newb", Some("HEAD"));

        let Err(error @ WorktreeError::UnusableWindowsName { .. }) = result else {
            panic!("{name:?} {path:?}: {result:?}");
        };
        assert!(
            error.to_string().starts_with("Windows can't use '"),
            "{error}"
        );
        assert_eq!(entry_names(dir.path()), before, "{name:?} {path:?}");
        assert!(entry_names(&repo.path().join("worktrees")).is_empty());
        assert!(!has_branch(&repo, "newb"));
    }

    create_worktree(&repo, "wt", &dir.path().join("wt"), "newb", Some("HEAD")).unwrap();
    assert!(dir.path().join("wt").join("file.txt").is_file());
}

/// A drive, a verbatim (`\\?\`) prefix and `.` components aren't folder names: such
/// destinations still work on Windows.
#[cfg(windows)]
#[test]
fn windows_roots_and_dot_components_are_accepted() {
    let (dir, repo, _) = committed_repo();
    let verbatim = dir.path().canonicalize().unwrap();
    assert!(
        verbatim.to_string_lossy().starts_with(r"\\?\"),
        "{verbatim:?}"
    );
    let cases = [
        ("abs", dir.path().join("abs")),
        ("dotted", dir.path().join(".").join("dotted")),
        ("verb", verbatim.join("verb")),
        ("rel", PathBuf::from(r".\rel")),
    ];
    for (name, path) in cases {
        create_worktree(&repo, name, &path, name, Some("HEAD"))
            .unwrap_or_else(|error| panic!("{path:?}: {error}"));
        assert!(dir.path().join(name).join("file.txt").is_file(), "{path:?}");
    }
}

/// Elsewhere such names work, as in git, and stay allowed.
#[cfg(not(windows))]
#[test]
fn a_name_ending_in_a_dot_or_a_space_works_outside_windows() {
    let (dir, repo, _) = committed_repo();
    for (name, branch) in [("wt.", "b1"), ("wt ", "b2"), ("aux", "b3")] {
        let linked = dir.path().join(name);
        create_worktree(&repo, name, &linked, branch, Some("HEAD")).unwrap();
        assert!(linked.join("file.txt").is_file(), "{name:?}");
        remove_worktree_named(&repo, name, &linked.canonicalize().unwrap()).unwrap();
        assert!(!linked.exists(), "{name:?}");
    }
    assert!(entry_names(&repo.path().join("worktrees")).is_empty());
}

/// A repository in `<temp>/repo` with the linked worktrees `<temp>/kept` and `<temp>/gone` (on
/// branches of the same names); `gone`'s folder has been deleted.
fn repo_with_a_missing_worktree() -> (tempfile::TempDir, git2::Repository, PathBuf, PathBuf) {
    let parent = tempfile::TempDir::new().unwrap();
    let root = parent.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    let repo = git2::Repository::init(&root).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    write_file(&root, "file.txt", "initial");
    commit_all(&repo, "initial commit");
    let kept = parent.path().join("kept");
    let gone = parent.path().join("gone");
    create_worktree(&repo, "kept", &kept, "kept", Some("HEAD")).unwrap();
    create_worktree(&repo, "gone", &gone, "gone", Some("HEAD")).unwrap();
    std::fs::remove_dir_all(&gone).unwrap();
    (parent, repo, kept, gone)
}

/// The same layout made with the git CLI; returns the repository's folder.
fn cli_repo_with_a_missing_worktree() -> (tempfile::TempDir, PathBuf) {
    let parent = tempfile::TempDir::new().unwrap();
    let root = parent.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "initial commit",
        ],
        &["worktree", "add", "-q", "../kept", "-b", "kept"],
        &["worktree", "add", "-q", "../gone", "-b", "gone"],
    ] {
        let output = git(&root, args);
        assert!(output.status.success(), "{args:?}: {output:?}");
    }
    std::fs::remove_dir_all(parent.path().join("gone")).unwrap();
    (parent, root)
}

/// The folder names of the worktrees `git worktree list` shows, main first.
fn cli_worktree_names(root: &Path) -> Vec<String> {
    let output = git(root, &["worktree", "list", "--porcelain"]);
    assert!(output.status.success(), "{output:?}");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .filter_map(|path| Path::new(path).file_name()?.to_str().map(str::to_string))
        .collect()
}

fn linked_names(repo: &git2::Repository) -> Vec<String> {
    let mut names: Vec<String> = list_worktrees(repo)
        .unwrap()
        .into_iter()
        .filter(|worktree| !worktree.is_main)
        .map(|worktree| worktree.name)
        .collect();
    names.sort();
    names
}

/// #252: another worktree's missing folder doesn't stop an intact one from being removed.
#[test]
fn an_intact_worktree_is_removed_when_another_worktrees_folder_is_missing() {
    let (_parent, repo, kept, _gone) = repo_with_a_missing_worktree();

    remove_worktree(&repo, &kept).unwrap();

    assert!(!kept.exists());
    assert_eq!(linked_names(&repo), ["gone"]);

    // git does the same.
    let (twin, root) = cli_repo_with_a_missing_worktree();
    let output = git(&root, &["worktree", "remove", "../kept"]);
    assert!(output.status.success(), "{output:?}");
    assert!(!twin.path().join("kept").exists());
    assert_eq!(cli_worktree_names(&root), ["repo", "gone"]);
}

/// #252: as with `git worktree remove`, a worktree whose folder is gone is removed from the
/// list, and its branch is free again.
#[test]
fn a_worktree_whose_folder_is_missing_is_removed_from_the_list() {
    let (_parent, repo, kept, gone) = repo_with_a_missing_worktree();
    let listed = list_worktrees(&repo)
        .unwrap()
        .into_iter()
        .find(|worktree| worktree.name == "gone")
        .unwrap();

    remove_worktree(&repo, &listed.path).unwrap();

    assert_eq!(linked_names(&repo), ["kept"]);
    assert!(!gone.exists());
    assert!(kept.join("file.txt").is_file());
    let mut branch = repo.find_branch("gone", git2::BranchType::Local).unwrap();
    branch.delete().unwrap();

    // git does the same.
    let (_twin, root) = cli_repo_with_a_missing_worktree();
    let output = git(&root, &["worktree", "remove", "../gone"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(cli_worktree_names(&root), ["repo", "kept"]);
    let output = git(&root, &["branch", "-d", "gone"]);
    assert!(output.status.success(), "{output:?}");
}

/// A locked worktree stays listed even when its folder is gone, as with `git worktree remove`.
#[test]
fn a_locked_worktree_whose_folder_is_missing_is_not_removed() {
    let (_parent, repo, _kept, gone) = repo_with_a_missing_worktree();
    repo.find_worktree("gone").unwrap().lock(None).unwrap();

    let result = remove_worktree(&repo, &gone);

    assert!(matches!(result, Err(WorktreeError::Locked)), "{result:?}");
    assert_eq!(linked_names(&repo), ["gone", "kept"]);

    // git refuses too.
    let (_twin, root) = cli_repo_with_a_missing_worktree();
    let output = git(&root, &["worktree", "lock", "../gone"]);
    assert!(output.status.success(), "{output:?}");
    let output = git(&root, &["worktree", "remove", "../gone"]);
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(cli_worktree_names(&root), ["repo", "gone", "kept"]);
}

/// #231: a relative destination starts from the top folder of the open worktree, as with
/// `git worktree add` run there, never from the app's working directory. Each resolution is
/// checked before anything is created, so a regression can't write outside the temp folder.
#[test]
fn a_relative_destination_is_resolved_against_the_repository() {
    let (parent, repo, kept, _gone) = repo_with_a_missing_worktree();
    let expected = parent.path().join("rel").join("wt");
    let resolved = resolve_destination(&repo, Path::new("../rel/wt")).unwrap();
    assert_eq!(comparable(&resolved), comparable(&expected));

    create_worktree(&repo, "rel", Path::new("../rel/wt"), "rel", Some("HEAD")).unwrap();

    assert!(expected.join("file.txt").is_file());
    let expected = expected.canonicalize().unwrap();
    assert!(list_worktrees(&repo)
        .unwrap()
        .iter()
        .any(|worktree| worktree.path == expected));

    // From a linked worktree, it is that worktree's top folder.
    let linked_repo = git2::Repository::open(&kept).unwrap();
    let resolved = resolve_destination(&linked_repo, Path::new("inner")).unwrap();
    assert_eq!(comparable(&resolved), comparable(&kept.join("inner")));
    create_worktree(
        &linked_repo,
        "inner",
        Path::new("inner"),
        "inner",
        Some("HEAD"),
    )
    .unwrap();
    assert!(kept.join("inner").join("file.txt").is_file());

    // git does the same, run from the top folder.
    let (twin, root) = cli_repo_with_a_missing_worktree();
    let output = git(&root, &["worktree", "add", "-q", "../rel/wt", "-b", "rel"]);
    assert!(output.status.success(), "{output:?}");
    assert!(twin.path().join("rel").join("wt").is_dir());
    let output = git(
        &twin.path().join("kept"),
        &["worktree", "add", "-q", "inner", "-b", "inner"],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(twin.path().join("kept").join("inner").is_dir());
}

/// `path` with its existing part resolved, so two spellings of one folder compare equal.
fn comparable(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut base = path.to_path_buf();
    while !base.exists() {
        missing.push(base.file_name().unwrap().to_owned());
        base.pop();
    }
    let mut resolved = base.canonicalize().unwrap();
    for part in missing.into_iter().rev() {
        resolved.push(part);
    }
    resolved
}

fn bare_repo() -> (tempfile::TempDir, git2::Repository) {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = git2::Repository::init_bare(dir.path().join("bare.git")).unwrap();
    {
        let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
        let mut builder = repo.treebuilder(None).unwrap();
        let blob = repo.blob(b"initial").unwrap();
        builder.insert("file.txt", blob, 0o100644).unwrap();
        let tree = repo.find_tree(builder.write().unwrap()).unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
    }
    (dir, repo)
}

/// A bare repository has no top folder to start a relative destination from.
#[test]
fn a_relative_destination_in_a_bare_repository_is_refused() {
    let (_dir, repo) = bare_repo();

    let resolved = resolve_destination(&repo, Path::new("rel-wt"));
    assert!(
        matches!(resolved, Err(WorktreeError::RelativePath)),
        "{resolved:?}"
    );
    let result = create_worktree(&repo, "rel", Path::new("rel-wt"), "rel", Some("HEAD"));

    assert!(
        matches!(result, Err(WorktreeError::RelativePath)),
        "{result:?}"
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "Enter the worktree's full path: this repository has no working folder to start a \
         relative path from."
    );
    assert!(!has_branch(&repo, "rel"));
}

/// Every file below `root` (`.git` included) with its bytes, sorted.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                out.push((path.strip_prefix(root).unwrap().to_path_buf(), bytes));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// What can be at a removed worktree's old place by the time Remove is clicked.
#[derive(Clone, Copy, Debug)]
enum Replacement {
    /// An unrelated repository, made with `git init` and committed to.
    UnrelatedRepository,
    /// A plain folder without `.git`.
    PlainFolder,
    /// A folder whose `.git` file points at another worktree's entry.
    OtherWorktreesGitFile,
}

fn replace_folder(folder: &Path, other_worktree: &Path, replacement: Replacement) {
    std::fs::remove_dir_all(folder).unwrap();
    std::fs::create_dir(folder).unwrap();
    std::fs::write(folder.join("keep.txt"), "precious").unwrap();
    match replacement {
        Replacement::UnrelatedRepository => {
            let unrelated = git2::Repository::init(folder).unwrap();
            let mut index = unrelated.index().unwrap();
            index.add_path(Path::new("keep.txt")).unwrap();
            index.write().unwrap();
            let tree = unrelated.find_tree(index.write_tree().unwrap()).unwrap();
            let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
            unrelated
                .commit(Some("HEAD"), &signature, &signature, "own", &tree, &[])
                .unwrap();
        }
        Replacement::PlainFolder => {}
        Replacement::OtherWorktreesGitFile => {
            std::fs::copy(other_worktree.join(".git"), folder.join(".git")).unwrap();
        }
    }
}

/// A folder at a worktree's place that isn't that worktree (it doesn't point back at the
/// worktree's entry) is never deleted, not even when it's clean; like `git worktree remove`
/// (even with `-f -f`), the removal is refused and the entry kept.
#[test]
fn a_folder_that_is_no_longer_the_worktree_is_never_deleted() {
    for replacement in [
        Replacement::UnrelatedRepository,
        Replacement::PlainFolder,
        Replacement::OtherWorktreesGitFile,
    ] {
        let (parent, repo, kept, _gone) = repo_with_a_missing_worktree();
        let other = parent.path().join("other");
        create_worktree(&repo, "other", &other, "other", Some("HEAD")).unwrap();
        replace_folder(&kept, &other, replacement);
        let before = snapshot(&kept);

        let by_path = remove_worktree(&repo, &kept);
        let by_name = remove_worktree_named(&repo, "kept", &kept);

        for result in [by_path, by_name] {
            assert!(
                matches!(&result, Err(WorktreeError::NotThisWorktree { path }) if path == &kept),
                "{replacement:?}: {result:?}"
            );
        }
        assert_eq!(snapshot(&kept), before, "{replacement:?}");
        assert_eq!(linked_names(&repo), ["gone", "kept", "other"]);
        assert!(other.join("file.txt").is_file());

        // git refuses too, even forced.
        let (twin, root) = cli_repo_with_a_missing_worktree();
        let output = git(&root, &["worktree", "add", "-q", "../other", "-b", "other"]);
        assert!(output.status.success(), "{output:?}");
        let twin_kept = twin.path().join("kept");
        replace_folder(&twin_kept, &twin.path().join("other"), replacement);
        let twin_before = snapshot(&twin_kept);
        let output = git(&root, &["worktree", "remove", "-f", "-f", "../kept"]);
        assert!(!output.status.success(), "{replacement:?}: {output:?}");
        assert_eq!(snapshot(&twin_kept), twin_before);
    }
}

/// The message says what to do.
#[test]
fn a_folder_that_is_no_longer_the_worktree_is_named() {
    let error = WorktreeError::NotThisWorktree {
        path: PathBuf::from("/work/feature"),
    };
    assert_eq!(
        error.to_string(),
        "The folder '/work/feature' is no longer this worktree. Move it away or delete it, then \
         remove the worktree again."
    );
}

/// A file where a parent folder of a worktree was (ENOTDIR) doesn't break the list, and that
/// worktree can be removed by name like one whose folder is gone. (`git worktree list` lists it
/// as prunable.)
#[test]
fn a_file_in_place_of_a_worktrees_parent_folder_does_not_break_the_list() {
    let (parent, repo, kept, _gone) = repo_with_a_missing_worktree();
    let nested = parent.path().join("p").join("q");
    create_worktree(&repo, "q", &nested, "q", Some("HEAD")).unwrap();
    std::fs::remove_dir_all(parent.path().join("p")).unwrap();
    std::fs::write(parent.path().join("p"), "a file").unwrap();

    let listed = list_worktrees(&repo).unwrap();
    let q = listed.iter().find(|worktree| worktree.name == "q").unwrap();
    assert!(q.is_prunable);

    remove_worktree_named(&repo, "q", &q.path).unwrap();
    remove_worktree(&repo, &kept).unwrap();
    assert_eq!(linked_names(&repo), ["gone"]);
    assert_eq!(
        std::fs::read_to_string(parent.path().join("p")).unwrap(),
        "a file"
    );

    // git lists it too.
    let (twin, root) = cli_repo_with_a_missing_worktree();
    let output = git(&root, &["worktree", "add", "-q", "../p/q", "-b", "q"]);
    assert!(output.status.success(), "{output:?}");
    std::fs::remove_dir_all(twin.path().join("p")).unwrap();
    std::fs::write(twin.path().join("p"), "a file").unwrap();
    assert_eq!(cli_worktree_names(&root), ["repo", "gone", "kept", "q"]);
}

/// Two entries can name one folder: an old one whose folder was deleted by hand, and a newer
/// one made at the same place. Removing by name removes that entry, never the other.
#[test]
fn removing_by_name_picks_that_entry_when_two_name_the_same_folder() {
    let (parent, repo, _kept, _gone) = repo_with_a_missing_worktree();
    let folder = parent.path().join("shared");
    create_worktree(&repo, "a-old", &folder, "a-old", Some("HEAD")).unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
    create_worktree(&repo, "b-new", &folder, "b-new", Some("HEAD")).unwrap();
    assert_eq!(linked_names(&repo), ["a-old", "b-new", "gone", "kept"]);

    // The old entry's folder is now the new worktree's: removing the old one keeps it.
    let result = remove_worktree_named(&repo, "a-old", &folder);
    assert!(
        matches!(result, Err(WorktreeError::NotThisWorktree { .. })),
        "{result:?}"
    );
    assert!(folder.join("file.txt").is_file());

    remove_worktree_named(&repo, "b-new", &folder).unwrap();
    assert!(!folder.exists());
    assert!(linked_names(&repo).contains(&"a-old".to_string()));
    assert!(!linked_names(&repo).contains(&"b-new".to_string()));

    // Now the old entry's folder is gone, so it can go too.
    remove_worktree_named(&repo, "a-old", &folder).unwrap();
    assert!(!linked_names(&repo).contains(&"a-old".to_string()));
}

/// A bare repository with linked worktrees (a "bare hub") lists them and removes them, as
/// `git worktree remove` does there.
#[test]
fn worktrees_of_a_bare_repository_are_listed_and_removed() {
    let (dir, repo) = bare_repo();
    let linked = dir.path().join("linked");
    create_worktree(&repo, "linked", &linked, "feature", Some("HEAD")).unwrap();

    let listed = list_worktrees(&repo).unwrap();
    let main = listed.iter().find(|worktree| worktree.is_main).unwrap();
    assert_eq!(main.path, repo.path().canonicalize().unwrap());
    assert!(matches!(
        remove_worktree(&repo, &main.path),
        Err(WorktreeError::MainWorktree)
    ));
    let entry = listed
        .iter()
        .find(|worktree| worktree.name == "linked")
        .unwrap();

    remove_worktree_named(&repo, "linked", &entry.path).unwrap();

    assert!(!linked.exists());
    assert_eq!(list_worktrees(&repo).unwrap().len(), 1);

    // git does the same.
    let twin = tempfile::TempDir::new().unwrap();
    let output = git(twin.path(), &["init", "-q", "--bare", "bare.git"]);
    assert!(output.status.success(), "{output:?}");
    let bare = twin.path().join("bare.git");
    let output = git(
        &bare,
        &["worktree", "add", "-q", "../linked", "-b", "feature"],
    );
    assert!(output.status.success(), "{output:?}");
    let output = git(&bare, &["worktree", "remove", "../linked"]);
    assert!(output.status.success(), "{output:?}");
    assert!(!twin.path().join("linked").exists());
}

/// git lets `git worktree add .git/x` put a worktree inside the repository's git folder;
/// Browsitory refuses it (where git keeps its own data, never a working folder). A worktree
/// inside the main work tree is allowed, as in git.
#[test]
fn a_destination_inside_the_git_folder_is_refused() {
    let (parent, repo, kept, _gone) = repo_with_a_missing_worktree();
    let root = parent.path().join("repo");
    let linked_repo = git2::Repository::open(&kept).unwrap();

    for (repo, path) in [
        (&repo, root.join(".git").join("x")),
        (&repo, PathBuf::from(".git/x")),
        (&repo, root.join(".git").join("worktrees").join("y")),
        (&linked_repo, root.join(".git").join("x")),
    ] {
        let result = create_worktree(repo, "x", &path, "x", Some("HEAD"));
        assert!(
            matches!(result, Err(WorktreeError::InsideGitFolder)),
            "{path:?}: {result:?}"
        );
        assert!(!has_branch(repo, "x"));
    }
    assert!(!root.join(".git").join("x").exists());
    assert_eq!(
        WorktreeError::InsideGitFolder.to_string(),
        "A worktree can't be inside the repository's .git folder. Choose another folder."
    );

    create_worktree(&repo, "sub", Path::new("sub/x"), "sub", Some("HEAD")).unwrap();
    assert!(root.join("sub").join("x").join("file.txt").is_file());

    // git allows both.
    let (_twin, twin_root) = cli_repo_with_a_missing_worktree();
    let output = git(&twin_root, &["worktree", "add", "-q", ".git/x", "-b", "x"]);
    assert!(output.status.success(), "{output:?}");
    let output = git(&twin_root, &["worktree", "add", "-q", "sub/x", "-b", "sub"]);
    assert!(output.status.success(), "{output:?}");
}
