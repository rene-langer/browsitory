//! Branches a worktree holds (#291): checked out in another worktree, the branch a terminal
//! `git bisect` started from, the branches a terminal rebase rebases or updates
//! (`--update-refs`), and the branch Browsitory's own paused rebase in another worktree rebases.
//! Restore, Undo, delete, rename, worktree add and Switch refuse what git refuses: `branch -f`
//! and `branch -D` follow git's `branch_checked_out`; `switch` and `worktree add` its
//! `find_shared_symref`; `branch -m` its `reject_rebase_or_bisect_branch`. Each rule is twinned
//! with the git CLI.
mod common;

use std::path::{Path, PathBuf};

use git2::{Oid, Repository};
use git_core::branch::{delete_branch, rename_branch, switch_branch};
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseState, RebaseStop};
use git_core::reflog::restore_reflog_entry;
use git_core::undo::{capture, restore, RepoSnapshot, SnapshotScope};
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
    topic_at_3: RepoSnapshot,
    topic_at_4: RepoSnapshot,
}

/// main: "1" to "4", checked out in the main worktree. `topic` moved from "3" to "4" (both in
/// its reflog). A linked worktree has `linked_branch` checked out.
fn fixture_on(linked_branch: &str) -> Fixture {
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
    repo.reference("refs/heads/other", commits[3], false, "create")
        .unwrap();
    let elsewhere = TempDir::new().unwrap();
    let linked_path = elsewhere.path().join("linked");
    git_ok(
        dir.path(),
        &[
            "worktree",
            "add",
            "-q",
            linked_path.to_str().unwrap(),
            linked_branch,
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

fn fixture() -> Fixture {
    fixture_on("topic")
}

/// Starts a bisect in `dir` from its current branch; HEAD ends up detached on "2" or "3".
fn bisect(dir: &Path, good: Oid) {
    git_ok(dir, &["bisect", "start"]);
    git_ok(dir, &["bisect", "bad"]);
    git_ok(dir, &["bisect", "good", &good.to_string()]);
}

/// The folder `message` names ("branch '<branch>' is <how> in another worktree at '<folder>'",
/// maybe followed by a hint), as written, and the rest of the message after it.
fn named_folder<'a>(message: &'a str, branch: &str, how: &str) -> (&'a str, &'a str) {
    let prefix = format!("branch '{branch}' is {how} in another worktree at '");
    let rest = message
        .strip_prefix(&prefix)
        .unwrap_or_else(|| panic!("{message:?} should start with {prefix:?}"));
    rest.split_once('\'')
        .unwrap_or_else(|| panic!("{message:?}"))
}

/// Asserts `message` refuses `branch` as held (`how`) by the worktree at `folder` (which
/// exists), with nothing after the folder.
fn assert_held_at(message: &str, branch: &str, how: &str, folder: &Path) {
    let (named, rest) = named_folder(message, branch, how);
    assert_eq!(rest, "", "{message}");
    assert!(!named.contains(".."), "{message}");
    assert_eq!(
        std::fs::canonicalize(named).unwrap(),
        std::fs::canonicalize(folder).unwrap(),
        "{message}"
    );
}

/// Every refusal the main worktree gives for `topic` (switch only when `with_switch`), checked
/// by `check`, and that nothing moved.
fn assert_all_refused(f: &Fixture, with_switch: bool, check: impl Fn(&str, &str)) {
    let repo = &f.repo;
    let three = f.commits[2].to_string();
    let message = |result: Result<(), String>| result.unwrap_err();
    check(
        &message(restore_reflog_entry(repo, "refs/heads/topic", &three).map_err(|e| e.to_string())),
        "restore",
    );
    check(
        &message(restore(repo, &f.topic_at_4, &f.topic_at_3, "undo").map_err(|e| e.to_string())),
        "undo",
    );
    check(
        &message(delete_branch(repo, "topic", true).map_err(|e| e.to_string())),
        "delete",
    );
    if with_switch {
        check(
            &message(switch_branch(repo, "topic").map_err(|e| e.to_string())),
            "switch",
        );
    }
    let added = f.elsewhere.path().join("added");
    check(
        &message(create_worktree(repo, "added", &added, "topic", None).map_err(|e| e.to_string())),
        "worktree add",
    );
    assert!(!added.exists());
    assert_eq!(target(repo, "refs/heads/topic"), Some(f.commits[3]));
}

/// git counts the branch a bisect started from (`BISECT_START`) as used by that worktree while
/// its HEAD is detached: `branch -f`, `branch -D`, `branch -m`, `switch` and `worktree add`
/// refuse it. So does Browsitory, or `git bisect reset` would return to a moved, renamed or
/// deleted branch, or to one checked out twice.
#[test]
fn a_branch_bisected_in_another_worktree_is_refused_like_git() {
    let f = fixture();
    bisect(&f.linked_path, f.commits[0]);
    let linked = Repository::open(&f.linked_path).unwrap();
    assert!(linked.head_detached().unwrap(), "the bisect detaches HEAD");
    let main = f.dir.path();
    let three = f.commits[2].to_string();
    assert!(git_refuses(main, &["branch", "-f", "topic", &three]).contains("used by worktree"));
    assert!(git_refuses(main, &["branch", "-D", "topic"]).contains("used by worktree"));
    assert!(git_refuses(main, &["branch", "-m", "topic", "renamed"]).contains("being bisected"));
    assert!(git_refuses(main, &["switch", "topic"]).contains("used by worktree"));

    let check = |message: &str, what: &str| {
        assert_held_at(message, "topic", "being bisected", &f.linked_path);
        let _ = what;
    };
    assert_all_refused(&f, true, check);
    check(
        &rename_branch(&f.repo, "topic", "renamed")
            .unwrap_err()
            .to_string(),
        "rename",
    );
    assert!(target(&f.repo, "refs/heads/renamed").is_none());

    // Once the bisect is reset, the branch is checked out there again, as before.
    git_ok(&f.linked_path, &["bisect", "reset"]);
    let message = delete_branch(&f.repo, "topic", true)
        .unwrap_err()
        .to_string();
    assert_held_at(&message, "topic", "checked out", &f.linked_path);
}

/// A worktree that switched to another branch during its bisect: `branch -f` and `branch -D`
/// still refuse the branch the bisect started from (git's `branch_checked_out` doesn't look at
/// HEAD), but `switch`, `worktree add` and `branch -m` allow it (they count a bisect only while
/// that worktree's HEAD is detached).
#[test]
fn a_bisect_whose_worktree_switched_away_blocks_only_moving_and_deleting() {
    let f = fixture();
    bisect(&f.linked_path, f.commits[0]);
    git_ok(&f.linked_path, &["switch", "-q", "other"]);
    let main = f.dir.path();
    let three = f.commits[2].to_string();
    assert!(git_refuses(main, &["branch", "-f", "topic", &three]).contains("used by worktree"));
    assert!(git_refuses(main, &["branch", "-D", "topic"]).contains("used by worktree"));

    let three = f.commits[2].to_string();
    let refused = restore_reflog_entry(&f.repo, "refs/heads/topic", &three)
        .unwrap_err()
        .to_string();
    assert_held_at(&refused, "topic", "being bisected", &f.linked_path);
    let refused = delete_branch(&f.repo, "topic", true)
        .unwrap_err()
        .to_string();
    assert_held_at(&refused, "topic", "being bisected", &f.linked_path);

    // The twins and Browsitory both allow the rest.
    git_ok(main, &["branch", "-m", "topic", "renamed"]);
    git_ok(main, &["branch", "-m", "renamed", "topic"]);
    rename_branch(&f.repo, "topic", "renamed").unwrap();
    rename_branch(&f.repo, "renamed", "topic").unwrap();
    let added = f.elsewhere.path().join("added");
    create_worktree(&f.repo, "added", &added, "topic", None).unwrap();
}

/// A bisect in this worktree holds its start branch too: git refuses `branch -f`, `-D`, `-m`
/// and `worktree add` for it, and allows `switch`.
#[test]
fn a_branch_bisected_in_this_worktree_is_refused_like_git() {
    let f = fixture_on("other");
    let main = f.dir.path();
    git_ok(main, &["switch", "-q", "topic"]);
    bisect(main, f.commits[0]);
    let three = f.commits[2].to_string();
    assert!(git_refuses(main, &["branch", "-f", "topic", &three]).contains("used by worktree"));
    assert!(git_refuses(main, &["branch", "-D", "topic"]).contains("used by worktree"));
    assert!(git_refuses(main, &["branch", "-m", "topic", "renamed"]).contains("being bisected"));
    let twin_added = f.elsewhere.path().join("twin");
    assert!(git_refuses(
        main,
        &["worktree", "add", twin_added.to_str().unwrap(), "topic"]
    )
    .contains("used by worktree"));
    let repo = Repository::open(main).unwrap();
    let f = Fixture { repo, ..f };

    let expected = "branch 'topic' is being bisected in this worktree";
    assert_all_refused(&f, false, |message, what| {
        assert_eq!(message, expected, "{what}")
    });
    assert_eq!(
        rename_branch(&f.repo, "topic", "renamed")
            .unwrap_err()
            .to_string(),
        expected
    );

    // Switching to it is allowed, as `git switch` allows it.
    switch_branch(&f.repo, "topic").unwrap();
    assert_eq!(f.repo.head().unwrap().name().ok(), Some("refs/heads/topic"));
}

/// `BISECT_LOG` holds the commits' raw subjects, which needn't be UTF-8 (a subject in
/// Latin-1 with no encoding header): the bisect still holds its start branch, or `git bisect
/// reset` couldn't check it out again.
#[test]
fn a_bisect_over_a_non_utf8_subject_still_holds_its_branch() {
    use std::io::Write;
    let f = fixture();
    let tree = f.repo.find_commit(f.commits[3]).unwrap().tree_id();
    let raw = [
        format!(
            "tree {tree}\nparent {}\nauthor A <a@b> 1700000000 +0000\ncommitter A <a@b> \
             1700000000 +0000\n\n",
            f.commits[3]
        )
        .into_bytes(),
        b"caf\xe9\n".to_vec(),
    ]
    .concat();
    let mut hash = test_support::git_command(f.dir.path())
        .args(["hash-object", "-t", "commit", "-w", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    hash.stdin.take().unwrap().write_all(&raw).unwrap();
    let output = hash.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let latin1 = String::from_utf8(output.stdout).unwrap().trim().to_string();
    git_ok(&f.linked_path, &["reset", "-q", "--hard", &latin1]);
    bisect(&f.linked_path, f.commits[0]);
    let linked = Repository::open(&f.linked_path).unwrap();
    let log = std::fs::read(linked.path().join("BISECT_LOG")).unwrap();
    assert!(
        String::from_utf8(log).is_err(),
        "BISECT_LOG holds the raw subject"
    );
    assert!(git_refuses(f.dir.path(), &["branch", "-D", "topic"]).contains("used by worktree"));

    let message = delete_branch(&f.repo, "topic", true)
        .unwrap_err()
        .to_string();

    assert_held_at(&message, "topic", "being bisected", &f.linked_path);
    git_ok(&f.linked_path, &["bisect", "reset"]);
}

/// A bisect started on a detached HEAD (`BISECT_START` holds a commit id) holds no branch.
#[test]
fn a_bisect_started_on_a_detached_head_holds_no_branch() {
    let f = fixture();
    git_ok(&f.linked_path, &["checkout", "-q", "--detach"]);
    bisect(&f.linked_path, f.commits[0]);
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

/// A terminal `git rebase --update-refs` in another worktree will move the branches in its
/// `update-refs` file: `branch -f` and `branch -D` refuse them, `branch -m`, `switch` and
/// `worktree add` don't.
#[test]
fn a_branch_a_rebase_elsewhere_will_update_is_refused_like_git() {
    let f = fixture_on("other");
    let main = f.dir.path();
    // linked: other ("4") on top of topic ("4"... moved back to "3" so it is inside the range).
    git_ok(main, &["branch", "-f", "topic", &f.commits[2].to_string()]);
    let stopped = git(
        &f.linked_path,
        &[
            "rebase",
            "-f",
            "--update-refs",
            "-x",
            "false",
            &f.commits[1].to_string(),
        ],
    );
    assert!(!stopped.status.success(), "the exec must stop: {stopped:?}");
    let linked = Repository::open(&f.linked_path).unwrap();
    let update_refs =
        std::fs::read_to_string(linked.path().join("rebase-merge/update-refs")).unwrap();
    assert!(update_refs.contains("refs/heads/topic\n"), "{update_refs}");
    let two = f.commits[1].to_string();
    assert!(git_refuses(main, &["branch", "-f", "topic", &two]).contains("used by worktree"));
    assert!(git_refuses(main, &["branch", "-D", "topic"]).contains("used by worktree"));

    let four = f.commits[3].to_string();
    let refused = restore_reflog_entry(&f.repo, "refs/heads/topic", &four)
        .unwrap_err()
        .to_string();
    assert_held_at(&refused, "topic", "being rebased", &f.linked_path);
    let refused = delete_branch(&f.repo, "topic", true)
        .unwrap_err()
        .to_string();
    assert_held_at(&refused, "topic", "being rebased", &f.linked_path);
    assert_eq!(target(&f.repo, "refs/heads/topic"), Some(f.commits[2]));

    git_ok(main, &["branch", "-m", "topic", "renamed"]);
    git_ok(main, &["branch", "-m", "renamed", "topic"]);
    rename_branch(&f.repo, "topic", "renamed").unwrap();
    rename_branch(&f.repo, "renamed", "topic").unwrap();
    switch_branch(&f.repo, "topic").unwrap();
}

/// A paused rebase as Browsitory stores it in `git_dir`: `topic` ("4") rebased onto "2", stopped
/// at its first step with HEAD on "2".
fn stored_rebase(f: &Fixture, plan: Vec<RebasePlanEntry>) -> RebaseState {
    RebaseState {
        plan,
        cursor: 0,
        original_branch_ref: "refs/heads/topic".into(),
        original_tip: f.commits[3],
        onto: f.commits[1],
        group_start_parent: None,
        expected_head: f.commits[1],
        stop: RebaseStop::Edit,
    }
}

fn pick(id: Oid) -> RebasePlanEntry {
    RebasePlanEntry {
        commit_id: id.to_string(),
        action: RebaseAction::Pick,
        combined_message: None,
    }
}

/// Browsitory's own paused rebase in another worktree (`browsitory/rebase.json`; git doesn't
/// know it) holds the branch it rebases as a terminal rebase does: Abort and Continue return to
/// it. Only a file that worktree would load itself counts, and only while its HEAD is detached
/// (once the rebase ended, HEAD is back on a branch, even when the file couldn't be deleted).
#[test]
fn a_branch_rebased_by_browsitory_in_another_worktree_is_refused() {
    let f = fixture();
    let linked = Repository::open(&f.linked_path).unwrap();
    linked.set_head_detached(f.commits[1]).unwrap();
    let file = git_core::rebase_file::file(linked.path());
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let valid = stored_rebase(&f, vec![pick(f.commits[2]), pick(f.commits[3])]);
    std::fs::write(&file, git_core::rebase_file::encode(&valid).unwrap()).unwrap();

    let check =
        |message: &str, _: &str| assert_held_at(message, "topic", "being rebased", &f.linked_path);
    assert_all_refused(&f, true, check);
    check(
        &rename_branch(&f.repo, "topic", "renamed")
            .unwrap_err()
            .to_string(),
        "rename",
    );

    // Files its own worktree would ignore hold nothing.
    let mut unknown_version: serde_json::Value =
        serde_json::from_slice(&git_core::rebase_file::encode(&valid).unwrap()).unwrap();
    unknown_version["version"] = serde_json::json!(99);
    let empty_plan = git_core::rebase_file::encode(&stored_rebase(&f, Vec::new())).unwrap();
    let missing_commit = git_core::rebase_file::encode(&stored_rebase(
        &f,
        vec![pick(
            Oid::from_str("1111111111111111111111111111111111111111").unwrap(),
        )],
    ))
    .unwrap();
    for (what, bytes) in [
        (
            "unknown version",
            serde_json::to_vec(&unknown_version).unwrap(),
        ),
        ("empty plan", empty_plan),
        ("missing commit", missing_commit),
        ("not json", b"not json".to_vec()),
    ] {
        std::fs::write(&file, bytes).unwrap();
        rename_branch(&f.repo, "topic", "renamed").unwrap_or_else(|e| panic!("{what}: {e}"));
        rename_branch(&f.repo, "renamed", "topic").unwrap();
    }

    // HEAD back on a branch: the rebase is over, whatever the file says.
    std::fs::write(&file, git_core::rebase_file::encode(&valid).unwrap()).unwrap();
    linked.set_head("refs/heads/other").unwrap();
    rename_branch(&f.repo, "topic", "renamed").unwrap();
}

/// A worktree whose folder is gone and that isn't locked still holds its branch, for git too,
/// until it is pruned: the refusal says so, and after `git worktree prune` the branch is free.
#[test]
fn a_worktree_whose_folder_is_gone_names_git_worktree_prune() {
    let f = fixture();
    std::fs::remove_dir_all(&f.linked_path).unwrap();
    let three = f.commits[2].to_string();
    assert!(git_refuses(f.dir.path(), &["branch", "-f", "topic", &three]).contains("used by"));
    let recorded = recorded_folder(&f);

    assert_all_refused(&f, true, |message, what| {
        let (named, rest) = named_folder(message, "topic", "checked out");
        assert_eq!(Path::new(named), recorded, "{what}");
        assert_eq!(
            rest,
            ". Its folder no longer exists: run git worktree prune to remove that worktree, \
             then try again.",
            "{what}"
        );
    });

    git_ok(f.dir.path(), &["worktree", "prune"]);
    restore_reflog_entry(&f.repo, "refs/heads/topic", &three).unwrap();
    assert_eq!(target(&f.repo, "refs/heads/topic"), Some(f.commits[2]));
}

/// The linked worktree's folder as git recorded it (its `gitdir` file names `<folder>/.git`):
/// on Windows git writes it with forward slashes and the long user name, and the refusal shows
/// it with native separators, so the two are compared component by component.
fn recorded_folder(f: &Fixture) -> PathBuf {
    let gitdir = std::fs::read_to_string(f.repo.path().join("worktrees/linked/gitdir")).unwrap();
    let dot_git = Path::new(gitdir.trim());
    assert!(dot_git.ends_with(".git"), "unexpected gitdir {gitdir:?}");
    dot_git.parent().unwrap().to_path_buf()
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
    let recorded = recorded_folder(&f);

    assert_all_refused(&f, true, |message, what| {
        let (named, rest) = named_folder(message, "topic", "checked out");
        assert_eq!(Path::new(named), recorded, "{what}");
        assert_eq!(
            rest,
            ". Its folder no longer exists, but that worktree is locked, so git keeps it. If it \
             is gone for good, run git worktree unlock and then git worktree prune.",
            "{what}"
        );
    });
}

/// A worktree folder whose `.git` file is gone is no longer a worktree, but git still counts
/// it until it is pruned; the refusal says what is wrong with it.
#[test]
fn a_folder_that_lost_its_git_file_names_git_worktree_prune() {
    let f = fixture();
    std::fs::remove_file(f.linked_path.join(".git")).unwrap();
    let three = f.commits[2].to_string();
    assert!(git_refuses(f.dir.path(), &["branch", "-f", "topic", &three]).contains("used by"));

    let message = restore_reflog_entry(&f.repo, "refs/heads/topic", &three)
        .unwrap_err()
        .to_string();
    let (named, rest) = named_folder(&message, "topic", "checked out");
    assert_eq!(
        std::fs::canonicalize(named).unwrap(),
        std::fs::canonicalize(&f.linked_path).unwrap()
    );
    assert_eq!(
        rest,
        ". That folder is no longer a worktree (its .git file is missing): run git worktree \
         prune to remove that worktree, then try again."
    );

    git_ok(f.dir.path(), &["worktree", "prune"]);
    restore_reflog_entry(&f.repo, "refs/heads/topic", &three).unwrap();
}

/// git doesn't list a worktree whose `gitdir` file is gone, so it holds nothing (`branch -f`
/// works), and neither does Browsitory.
#[test]
fn a_worktree_without_its_gitdir_file_holds_nothing() {
    let f = fixture();
    std::fs::remove_file(f.repo.path().join("worktrees/linked/gitdir")).unwrap();
    git_ok(
        f.dir.path(),
        &["branch", "-f", "topic", &f.commits[3].to_string()],
    );

    restore_reflog_entry(&f.repo, "refs/heads/topic", &f.commits[2].to_string()).unwrap();

    assert_eq!(target(&f.repo, "refs/heads/topic"), Some(f.commits[2]));
}

/// `git worktree add --relative-paths` records the worktree's `.git` file relative to its git
/// folder; the refusal names the folder without `..`.
#[test]
fn a_relative_gitdir_names_the_worktree_folder() {
    let f = fixture();
    let gitdir = f.repo.path().join("worktrees/linked/gitdir");
    let elsewhere_name = f.elsewhere.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(
        std::fs::canonicalize(f.dir.path().parent().unwrap()).unwrap(),
        std::fs::canonicalize(f.elsewhere.path().parent().unwrap()).unwrap(),
        "both temp dirs share a parent"
    );
    std::fs::write(
        &gitdir,
        format!("../../../../{elsewhere_name}/linked/.git\n"),
    )
    .unwrap();
    let three = f.commits[2].to_string();
    assert!(git_refuses(f.dir.path(), &["branch", "-f", "topic", &three]).contains("used by"));

    let message = restore_reflog_entry(&f.repo, "refs/heads/topic", &three)
        .unwrap_err()
        .to_string();

    assert_held_at(&message, "topic", "checked out", &f.linked_path);
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

    let message = switch_branch(&repo, "taken").unwrap_err().to_string();
    assert_held_at(&message, "taken", "checked out", &second);
    switch_branch(&repo, "spare").unwrap();

    assert_eq!(repo.head().unwrap().name().ok(), Some("refs/heads/spare"));
}
