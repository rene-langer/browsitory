//! `change_stamp` (#650): changes the app shows change the stamp; git's own bookkeeping and
//! ignored files don't.

mod common;

use std::path::Path;

use common::{commit_all, init_repo, write_file};
use git2::{Repository, Signature};
use git_core::change_stamp::{change_stamp, ChangeStamp};

fn stamp(repo: &Repository) -> ChangeStamp {
    change_stamp(repo).expect("stamp")
}

/// A repository with one commit of `a.txt` and an ignored `build/` folder.
fn committed_repo() -> (tempfile::TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    write_file(dir.path(), ".gitignore", "build/\n");
    commit_all(&repo, "first");
    (dir, repo)
}

fn stage(repo: &Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

#[test]
fn an_unchanged_repository_keeps_its_stamp() {
    let (_dir, repo) = committed_repo();
    assert_eq!(stamp(&repo), stamp(&repo));
}

#[test]
fn an_empty_repository_has_a_stamp() {
    let (dir, repo) = init_repo();
    let before = stamp(&repo);
    write_file(dir.path(), "new.txt", "x");
    assert_ne!(stamp(&repo), before);
}

#[test]
fn editing_a_tracked_file_changes_it_and_editing_it_again_too() {
    let (dir, repo) = committed_repo();
    let clean = stamp(&repo);
    write_file(dir.path(), "a.txt", "one\ntwo\n");
    let modified = stamp(&repo);
    assert_ne!(modified, clean);
    // Already modified: the status flags stay the same, the file's size and time don't.
    write_file(dir.path(), "a.txt", "one\ntwo\nthree\n");
    assert_ne!(stamp(&repo), modified);
}

#[test]
fn a_new_and_a_deleted_file_change_it() {
    let (dir, repo) = committed_repo();
    let clean = stamp(&repo);
    write_file(dir.path(), "sub/new.txt", "x");
    let added = stamp(&repo);
    assert_ne!(added, clean);
    std::fs::remove_file(dir.path().join("a.txt")).unwrap();
    assert_ne!(stamp(&repo), added);
}

#[test]
fn ignored_files_dont_change_it() {
    let (dir, repo) = committed_repo();
    let clean = stamp(&repo);
    write_file(dir.path(), "build/out.o", "binary");
    assert_eq!(stamp(&repo), clean);
    write_file(dir.path(), "build/out.o", "other binary");
    assert_eq!(stamp(&repo), clean);
}

#[test]
fn staging_changes_it_and_staging_again_too() {
    let (dir, repo) = committed_repo();
    write_file(dir.path(), "a.txt", "one\ntwo\n");
    let unstaged = stamp(&repo);
    stage(&repo, "a.txt");
    let staged = stamp(&repo);
    assert_ne!(staged, unstaged);
    // Staged and then changed and staged once more: only the index entry's id differs from a
    // moment ago, once the work tree file is back to its earlier size.
    write_file(dir.path(), "a.txt", "one\nTWO\n");
    let edited = stamp(&repo);
    stage(&repo, "a.txt");
    assert_ne!(stamp(&repo), edited);
}

#[test]
fn rewriting_a_staged_file_with_the_same_content_keeps_it() {
    let (dir, repo) = committed_repo();
    write_file(dir.path(), "a.txt", "one\ntwo\n");
    stage(&repo, "a.txt");
    let staged = stamp(&repo);
    // A new modification time, but nothing to show: the file still matches the index.
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_file(dir.path(), "a.txt", "one\ntwo\n");
    assert_eq!(stamp(&repo), staged);
}

#[test]
fn rewriting_the_index_without_changes_keeps_it() {
    let (_dir, repo) = committed_repo();
    let clean = stamp(&repo);
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    index.write().unwrap();
    assert_eq!(stamp(&repo), clean);
}

#[test]
fn committing_changes_it() {
    let (dir, repo) = committed_repo();
    write_file(dir.path(), "a.txt", "one\ntwo\n");
    stage(&repo, "a.txt");
    let staged = stamp(&repo);
    commit_all(&repo, "second");
    assert_ne!(stamp(&repo), staged);
}

#[test]
fn branches_tags_and_head_change_it() {
    let (_dir, repo) = committed_repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let clean = stamp(&repo);
    repo.branch("topic", &head, false).unwrap();
    let branched = stamp(&repo);
    assert_ne!(branched, clean);
    repo.tag_lightweight("v1", head.as_object(), false).unwrap();
    let tagged = stamp(&repo);
    assert_ne!(tagged, branched);
    repo.set_head("refs/heads/topic").unwrap();
    let switched = stamp(&repo);
    assert_ne!(switched, tagged);
    repo.set_head_detached(head.id()).unwrap();
    assert_ne!(stamp(&repo), switched);
}

#[test]
fn a_symbolic_reference_target_changes_it() {
    let (_dir, repo) = committed_repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("other", &head, false).unwrap();
    repo.reference_symbolic("refs/remotes/origin/HEAD", "refs/heads/master", true, "")
        .unwrap();
    let before = stamp(&repo);
    repo.reference_symbolic("refs/remotes/origin/HEAD", "refs/heads/other", true, "")
        .unwrap();
    assert_ne!(stamp(&repo), before);
}

#[test]
fn a_moved_remote_branch_changes_it() {
    let (dir, repo) = committed_repo();
    let first = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.reference("refs/remotes/origin/main", first, true, "")
        .unwrap();
    write_file(dir.path(), "a.txt", "one\ntwo\n");
    commit_all(&repo, "second");
    let second = repo.head().unwrap().peel_to_commit().unwrap().id();
    let before = stamp(&repo);
    repo.reference("refs/remotes/origin/main", second, true, "")
        .unwrap();
    assert_ne!(stamp(&repo), before);
}

#[test]
fn git_bookkeeping_doesnt_change_it() {
    let (_dir, repo) = committed_repo();
    let clean = stamp(&repo);
    let git_dir = repo.path();
    std::fs::write(
        git_dir.join("FETCH_HEAD"),
        "0000000000000000000000000000000000000000\n",
    )
    .unwrap();
    std::fs::write(
        git_dir.join("ORIG_HEAD"),
        "0000000000000000000000000000000000000000\n",
    )
    .unwrap();
    repo.blob(b"a loose object nobody points at").unwrap();
    assert_eq!(stamp(&repo), clean);
}

#[test]
fn an_operation_in_progress_changes_it() {
    let (_dir, repo) = committed_repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    let clean = stamp(&repo);
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{head}\n")).unwrap();
    let merging = stamp(&repo);
    assert_ne!(merging, clean);
    std::fs::create_dir_all(repo.path().join("sequencer")).unwrap();
    std::fs::write(
        repo.path().join("sequencer/todo"),
        format!("pick {head} x\n"),
    )
    .unwrap();
    assert_ne!(stamp(&repo), merging);
}

#[test]
fn the_configuration_changes_it() {
    let (_dir, repo) = committed_repo();
    let clean = stamp(&repo);
    repo.remote("origin", "https://example.invalid/repo.git")
        .unwrap();
    assert_ne!(stamp(&repo), clean);
}

#[test]
fn dropping_an_older_stash_changes_it() {
    let (dir, mut repo) = committed_repo();
    let signature = Signature::now("Test User", "test@example.com").unwrap();
    write_file(dir.path(), "a.txt", "one\nfirst stash\n");
    repo.stash_save(&signature, "first", None).unwrap();
    write_file(dir.path(), "a.txt", "one\nsecond stash\n");
    repo.stash_save(&signature, "second", None).unwrap();
    let two = stamp(&repo);
    // `refs/stash` still points at the newest stash; only its reflog lost an entry.
    repo.stash_drop(1).unwrap();
    assert_ne!(stamp(&repo), two);
}

#[test]
fn a_linked_worktree_switching_branches_changes_the_main_stamp() {
    let (dir, repo) = committed_repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("wt", &head, false).unwrap();
    repo.branch("wt2", &head, false).unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    let path = outside.path().join("linked");
    let reference = repo.find_reference("refs/heads/wt").unwrap();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    repo.worktree("linked", &path, Some(&options)).unwrap();
    let added = stamp(&repo);
    let linked = Repository::open(&path).unwrap();
    let linked_before = stamp(&linked);
    linked.set_head("refs/heads/wt2").unwrap();
    assert_ne!(stamp(&repo), added);
    assert_ne!(stamp(&linked), linked_before);
    let _ = dir;
}

#[test]
fn a_locked_worktree_changes_it() {
    let (_dir, repo) = committed_repo();
    let outside = tempfile::TempDir::new().unwrap();
    let worktree = repo
        .worktree("linked", &outside.path().join("linked"), None)
        .unwrap();
    let before = stamp(&repo);
    worktree.lock(None).unwrap();
    assert_ne!(stamp(&repo), before);
}

#[test]
fn a_bare_repository_has_a_stamp_that_follows_its_references() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = Repository::init_bare(dir.path()).unwrap();
    let before = stamp(&repo);
    let signature = Signature::now("Test User", "test@example.com").unwrap();
    let tree = repo
        .find_tree(repo.treebuilder(None).unwrap().write().unwrap())
        .unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "first", &tree, &[])
        .unwrap();
    assert_ne!(stamp(&repo), before);
}

// #685: a refresh keeps the history it has while the `refs` part is unchanged.
#[test]
fn editing_and_staging_change_only_the_work_tree_part() {
    let (dir, repo) = committed_repo();
    let clean = stamp(&repo);
    write_file(dir.path(), "a.txt", "one\ntwo\n");
    let edited = stamp(&repo);
    assert_eq!(edited.refs, clean.refs);
    assert_ne!(edited.work_tree, clean.work_tree);
    stage(&repo, "a.txt");
    let staged = stamp(&repo);
    assert_eq!(staged.refs, clean.refs);
    assert_ne!(staged.work_tree, edited.work_tree);
}

#[test]
fn a_new_branch_changes_only_the_refs_part() {
    let (_dir, repo) = committed_repo();
    let clean = stamp(&repo);
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let branched = stamp(&repo);
    assert_ne!(branched.refs, clean.refs);
    assert_eq!(branched.work_tree, clean.work_tree);
}

#[test]
fn committing_changes_both_parts() {
    let (dir, repo) = committed_repo();
    write_file(dir.path(), "a.txt", "one\ntwo\n");
    stage(&repo, "a.txt");
    let staged = stamp(&repo);
    commit_all(&repo, "second");
    let committed = stamp(&repo);
    assert_ne!(committed.refs, staged.refs);
    assert_ne!(committed.work_tree, staged.work_tree);
}
