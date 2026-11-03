//! A branch another worktree holds (#291): checked out there, the branch a terminal `git bisect`
//! started from there, or the branch Browsitory's own paused rebase there is rebasing. Restore,
//! Undo, delete, rename, worktree add and switch all refuse it with one wording, as git does
//! (its `find_shared_symref`). Each git rule is twinned with the git CLI.
mod common;

use std::path::{Path, PathBuf};

use git2::{Oid, Repository};
use git_core::branch::{delete_branch, rename_branch, switch_branch};
use git_core::reflog::restore_reflog_entry;
use git_core::undo::{capture, restore, SnapshotScope};
use git_core::worktree::create_worktree;
use tempfile::TempDir;
use test_support::git;

fn git_ok(dir: &Path, args: &[&str]) {
    let output = git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

fn git_refuses(dir: &Path, args: &[&str]) -> String {
    let output = git(dir, args);
    assert!(
        !output.status.success(),
        "git {args:?} must fail: {output:?}"
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn target(repo: &Repository, name: &str) -> Option<Oid> {
    repo.find_reference(name).ok().and_then(|r| r.target())
}

struct Fixture {
    dir: TempDir,
    repo: Repository,
    /// Holds the linked worktree's folder.
    elsewhere: TempDir,
    linked_path: PathBuf,
    /// Commits "1" to "4" on main, oldest first.
    commits: Vec<Oid>,
    /// Undo snapshots of the main worktree: `topic` at "3", then at "4".
    topic_at_3: git_core::undo::RepoSnapshot,
    topic_at_4: git_core::undo::RepoSnapshot,
}

/// main: "1" to "4", checked out in the main worktree. `topic` moved from "3" to "4" (both in
/// its reflog), checked out in a linked worktree.
fn fixture() -> Fixture {
    let (dir, repo) = common::init_repo();
    repo.set_head("refs/heads/main").unwrap();
    let mut commits = Vec::new();
    for n in 1..=4 {
        common::write_file(dir.path(), "n.txt", &n.to_string());
        common::commit_all(&repo, &n.to_string());
        commits.push(repo.head().unwrap().target().unwrap());
    }
    repo.reference("refs/heads/topic", commits[2], false, "create")
        .unwrap();
    let topic_at_3 = capture(&repo, SnapshotScope::Full).unwrap();
    repo.reference("refs/heads/topic", commits[3], true, "move")
        .unwrap();
    let topic_at_4 = capture(&repo, SnapshotScope::Full).unwrap();
    let elsewhere = TempDir::new().unwrap();
    let linked_path = elsewhere.path().join("linked");
    git_ok(
        dir.path(),
        &[
            "worktree",
            "add",
            "-q",
            linked_path.to_str().unwrap(),
            "topic",
        ],
    );
    Fixture {
        dir,
        repo,
        elsewhere,
        linked_path,
        commits,
        topic_at_3,
        topic_at_4,
    }
}

/// Every refusal the main worktree gives for `topic`, and that nothing moved.
fn assert_all_refused(f: &Fixture, expected: &str) {
    let repo = &f.repo;
    let three = f.commits[2].to_string();
    let message = |result: Result<(), String>| result.unwrap_err();
    assert_eq!(
        message(restore_reflog_entry(repo, "refs/heads/topic", &three).map_err(|e| e.to_string())),
        expected,
        "restore"
    );
    assert_eq!(
        message(restore(repo, &f.topic_at_4, &f.topic_at_3, "undo").map_err(|e| e.to_string())),
        expected,
        "undo"
    );
    assert_eq!(
        message(delete_branch(repo, "topic", true).map_err(|e| e.to_string())),
        expected,
        "delete"
    );
    assert_eq!(
        message(switch_branch(repo, "topic").map_err(|e| e.to_string())),
        expected,
        "switch"
    );
    let added = f.elsewhere.path().join("added");
    assert_eq!(
        message(create_worktree(repo, "added", &added, "topic", None).map_err(|e| e.to_string())),
        expected,
        "worktree add"
    );
    assert!(!added.exists());
    assert_eq!(target(repo, "refs/heads/topic"), Some(f.commits[3]));
    assert_eq!(repo.head().unwrap().name().ok(), Some("refs/heads/main"));
}

/// git counts the branch a bisect started from (`BISECT_START`) as used by that worktree,
/// though HEAD is detached meanwhile: `branch -f`, `branch -D`, `branch -m`, `switch` and
/// `worktree add` refuse it. So does Browsitory, or `git bisect reset` would return to a moved,
/// renamed or deleted branch, or to one checked out twice.
#[test]
fn a_branch_bisected_in_another_worktree_is_refused_like_git() {
    let f = fixture();
    git_ok(&f.linked_path, &["bisect", "start"]);
    git_ok(&f.linked_path, &["bisect", "bad"]);
    git_ok(
        &f.linked_path,
        &["bisect", "good", &f.commits[0].to_string()],
    );
    let linked = Repository::open(&f.linked_path).unwrap();
    assert!(linked.head_detached().unwrap(), "the bisect detaches HEAD");
    let main = f.dir.path();
    let three = f.commits[2].to_string();
    assert!(git_refuses(main, &["branch", "-f", "topic", &three]).contains("used by worktree"));
    assert!(git_refuses(main, &["branch", "-D", "topic"]).contains("used by worktree"));
    assert!(git_refuses(main, &["branch", "-m", "topic", "renamed"]).contains("being bisected"));
    assert!(git_refuses(main, &["switch", "topic"]).contains("used by worktree"));

    let expected = "branch 'topic' is being bisected in another worktree";
    assert_all_refused(&f, expected);
    assert_eq!(
        rename_branch(&f.repo, "topic", "renamed")
            .unwrap_err()
            .to_string(),
        expected
    );
    assert!(target(&f.repo, "refs/heads/renamed").is_none());

    // Once the bisect is reset, the branch is checked out there again, as before.
    git_ok(&f.linked_path, &["bisect", "reset"]);
    assert_eq!(
        delete_branch(&f.repo, "topic", true)
            .unwrap_err()
            .to_string(),
        "branch 'topic' is checked out in another worktree"
    );
}

/// A bisect started on a detached HEAD (`BISECT_START` holds a commit id) holds no branch.
#[test]
fn a_bisect_started_on_a_detached_head_holds_no_branch() {
    let f = fixture();
    git_ok(&f.linked_path, &["checkout", "-q", "--detach"]);
    git_ok(&f.linked_path, &["bisect", "start"]);
    git_ok(&f.linked_path, &["bisect", "bad"]);
    git_ok(
        &f.linked_path,
        &["bisect", "good", &f.commits[0].to_string()],
    );
    git_ok(
        f.dir.path(),
        &["branch", "-f", "topic", &f.commits[2].to_string()],
    );
    git_ok(
        f.dir.path(),
        &["branch", "-f", "topic", &f.commits[3].to_string()],
    );

    restore_reflog_entry(&f.repo, "refs/heads/topic", &f.commits[2].to_string()).unwrap();

    assert_eq!(target(&f.repo, "refs/heads/topic"), Some(f.commits[2]));
}

/// Browsitory's own paused rebase (`browsitory/rebase.json` in that worktree's git folder; git
/// doesn't know it) holds the branch it rebases as a terminal rebase does: Abort and Continue
/// return to it.
#[test]
fn a_branch_rebased_by_browsitory_in_another_worktree_is_refused() {
    let f = fixture();
    let linked = Repository::open(&f.linked_path).unwrap();
    linked
        .set_head_detached(f.commits[1])
        .expect("a paused rebase detaches HEAD");
    let file = git_core::repo::stored_rebase_file(linked.path());
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        r#"{"version":1,"originalBranchRef":"refs/heads/topic"}"#,
    )
    .unwrap();

    let expected = "branch 'topic' is being rebased in another worktree";
    assert_all_refused(&f, expected);
    assert_eq!(
        rename_branch(&f.repo, "topic", "renamed")
            .unwrap_err()
            .to_string(),
        expected
    );

    // A stored rebase file that names no branch holds nothing.
    std::fs::write(&file, "not json").unwrap();
    restore_reflog_entry(&f.repo, "refs/heads/topic", &f.commits[2].to_string()).unwrap();
    assert_eq!(target(&f.repo, "refs/heads/topic"), Some(f.commits[2]));
}

/// A worktree whose folder is gone and that isn't locked still holds its branch, for git too,
/// until it is pruned: the refusal says so, and after `git worktree prune` the branch is free.
#[test]
fn a_worktree_whose_folder_is_gone_names_git_worktree_prune() {
    let f = fixture();
    std::fs::remove_dir_all(&f.linked_path).unwrap();
    let three = f.commits[2].to_string();
    assert!(git_refuses(f.dir.path(), &["branch", "-f", "topic", &three]).contains("used by"));
    let linked_path = f.linked_path.display();

    let expected = format!(
        "branch 'topic' is checked out in another worktree. Its folder '{linked_path}' no \
         longer exists: run git worktree prune to remove that worktree, then try again."
    );
    assert_all_refused(&f, &expected);

    git_ok(f.dir.path(), &["worktree", "prune"]);
    restore_reflog_entry(&f.repo, "refs/heads/topic", &three).unwrap();
    assert_eq!(target(&f.repo, "refs/heads/topic"), Some(f.commits[2]));
}

/// A locked worktree whose folder is gone (an unmounted drive) keeps its branch: `git worktree
/// prune` leaves it, so the refusal says the worktree is locked instead.
#[test]
fn a_locked_worktree_whose_folder_is_gone_says_it_is_locked() {
    let f = fixture();
    git_ok(
        f.dir.path(),
        &["worktree", "lock", f.linked_path.to_str().unwrap()],
    );
    std::fs::rename(&f.linked_path, f.elsewhere.path().join("unmounted")).unwrap();
    git_ok(f.dir.path(), &["worktree", "prune"]);
    let three = f.commits[2].to_string();
    assert!(git_refuses(f.dir.path(), &["branch", "-f", "topic", &three]).contains("used by"));
    let linked_path = f.linked_path.display();

    assert_all_refused(
        &f,
        &format!(
            "branch 'topic' is checked out in another worktree. Its folder '{linked_path}' is \
             missing, but that worktree is locked, so git keeps it. If the folder is gone for \
             good, run git worktree unlock and then git worktree prune."
        ),
    );
}

/// In a linked worktree of a bare repository (no main work tree), Switch still works and still
/// refuses a branch another linked worktree has checked out, as `git switch` does.
#[test]
fn switch_works_in_a_linked_worktree_of_a_bare_repository() {
    let (source, source_repo) = common::init_repo();
    source_repo.set_head("refs/heads/main").unwrap();
    common::write_file(source.path(), "f.txt", "one");
    common::commit_all(&source_repo, "one");
    let parent = TempDir::new().unwrap();
    let bare = parent.path().join("bare.git");
    git_ok(
        parent.path(),
        &[
            "clone",
            "-q",
            "--bare",
            source.path().to_str().unwrap(),
            "bare.git",
        ],
    );
    git_ok(&bare, &["branch", "spare", "main"]);
    git_ok(&bare, &["branch", "taken", "main"]);
    let first = parent.path().join("first");
    let second = parent.path().join("second");
    git_ok(
        &bare,
        &["worktree", "add", "-q", first.to_str().unwrap(), "main"],
    );
    git_ok(
        &bare,
        &["worktree", "add", "-q", second.to_str().unwrap(), "taken"],
    );
    assert!(git_refuses(&first, &["switch", "taken"]).contains("already used by worktree"));
    let repo = Repository::open(&first).unwrap();

    assert_eq!(
        switch_branch(&repo, "taken").unwrap_err().to_string(),
        "branch 'taken' is checked out in another worktree"
    );
    switch_branch(&repo, "spare").unwrap();

    assert_eq!(repo.head().unwrap().name().ok(), Some("refs/heads/spare"));
}
