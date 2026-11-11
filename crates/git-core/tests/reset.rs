mod common;

use git_core::reset::{reset_to, ResetError, ResetMode};

/// The newest reflog entry of `name`: (old id, new id, message).
fn last_reflog(repo: &git2::Repository, name: &str) -> (git2::Oid, git2::Oid, String) {
    let reflog = repo.reflog(name).unwrap();
    let entry = reflog.get(0).expect("a reflog entry");
    (
        entry.id_old(),
        entry.id_new(),
        entry.message().unwrap().unwrap_or("").to_string(),
    )
}

/// HEAD on a branch that is a symbolic ref to another branch (`git symbolic-ref
/// refs/heads/alias refs/heads/<main>`): a mixed or hard reset moves the branch the chain ends
/// at and keeps the alias, as libgit2's `git_reset` and `git reset` do; both the branch's and
/// HEAD's reflogs get the entry.
#[test]
fn a_reset_through_a_symbolic_branch_moves_the_branch_it_names() {
    for mode in [ResetMode::Mixed, ResetMode::Hard] {
        let (_dir, repo, first) = three_commits();
        let main = repo.head().unwrap().name().unwrap().to_string();
        let tip = repo.head().unwrap().target().unwrap();
        repo.reference_symbolic("refs/heads/alias", &main, true, "alias")
            .unwrap();
        repo.set_head("refs/heads/alias").unwrap();

        reset_to(&repo, &first.to_string(), mode).unwrap();

        let alias = repo.find_reference("refs/heads/alias").unwrap();
        assert_eq!(
            alias.symbolic_target().unwrap(),
            Some(main.as_str()),
            "{mode:?}"
        );
        assert_eq!(repo.find_reference(&main).unwrap().target(), Some(first));
        assert_eq!(repo.head().unwrap().target(), Some(first));
        let expected = (tip, first, format!("reset: moving to {first}"));
        assert_eq!(last_reflog(&repo, &main), expected, "{mode:?}");
        assert_eq!(last_reflog(&repo, "HEAD"), expected, "{mode:?}");
    }
}

/// HEAD's reflog entry after a reset is git's: `reset: moving to <id>` from the old to the new
/// HEAD, on a branch and on a detached HEAD.
#[test]
fn a_reset_writes_gits_reflog_entry_for_head() {
    for (mode, flag) in [(ResetMode::Mixed, "--mixed"), (ResetMode::Hard, "--hard")] {
        for detached in [false, true] {
            let run = |use_git: bool| {
                let (dir, repo, first) = three_commits();
                let tip = repo.head().unwrap().target().unwrap();
                if detached {
                    repo.set_head_detached(tip).unwrap();
                }
                if use_git {
                    let output = test_support::git_command(dir.path())
                        .args(["reset", "-q", flag, &first.to_string()])
                        .output()
                        .unwrap();
                    assert!(output.status.success(), "{output:?}");
                } else {
                    reset_to(&repo, &first.to_string(), mode).unwrap();
                }
                let (old, new, message) = last_reflog(&repo, "HEAD");
                assert_eq!((old, new), (tip, first));
                assert_eq!(repo.head_detached().unwrap(), detached);
                // The twins are separate repositories whose commit ids differ (commit times,
                // and line endings on Windows): compare the messages with each one's own target
                // id named the same way, so only the format is compared.
                let target = first.to_string();
                assert!(message.contains(&target), "{message}");
                message.replace(&target, "<target>")
            };
            assert_eq!(run(false), run(true), "{mode:?} detached={detached}");
        }
    }
}

fn three_commits() -> (tempfile::TempDir, git2::Repository, git2::Oid) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "1\n");
    common::commit_all(&repo, "one");
    let first = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "2\n");
    common::commit_all(&repo, "two");
    common::write_file(dir.path(), "a.txt", "3\n");
    common::commit_all(&repo, "three");
    (dir, repo, first)
}

fn staged_paths(repo: &git2::Repository) -> usize {
    let head_tree = repo.head().unwrap().peel_to_tree().unwrap();
    repo.diff_tree_to_index(Some(&head_tree), None, None)
        .unwrap()
        .deltas()
        .len()
}

#[test]
fn soft_reset_moves_branch_and_keeps_index_and_workdir() {
    let (dir, repo, first) = three_commits();
    reset_to(&repo, &first.to_string(), ResetMode::Soft).unwrap();
    assert_eq!(repo.head().unwrap().target().unwrap(), first);
    assert!(repo.head().unwrap().is_branch());
    assert_eq!(staged_paths(&repo), 1);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "3\n"
    );
}

#[test]
fn mixed_reset_unstages_but_keeps_workdir() {
    let (dir, repo, first) = three_commits();
    reset_to(&repo, &first.to_string(), ResetMode::Mixed).unwrap();
    assert_eq!(staged_paths(&repo), 0);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "3\n"
    );
}

#[test]
fn hard_reset_discards_tracked_changes_but_keeps_untracked_files() {
    let (dir, repo, first) = three_commits();
    common::write_file(dir.path(), "untracked.txt", "keep me\n");
    reset_to(&repo, &first.to_string(), ResetMode::Hard).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "1\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("untracked.txt")).unwrap(),
        "keep me\n"
    );
}

#[test]
fn reset_refuses_during_a_paused_operation() {
    let (_dir, repo, first) = three_commits();
    let head = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{head}\n")).unwrap();
    assert!(matches!(
        reset_to(&repo, &first.to_string(), ResetMode::Hard),
        Err(ResetError::OperationInProgress)
    ));
}

#[test]
fn reset_clears_a_leftover_squash_message() {
    let (_dir, repo, first) = three_commits();
    std::fs::write(
        repo.path().join("SQUASH_MSG"),
        "Squashed commit of the following:\n",
    )
    .unwrap();

    reset_to(&repo, &first.to_string(), ResetMode::Mixed).unwrap();

    assert!(!repo.path().join("SQUASH_MSG").exists());
}

#[test]
#[cfg(unix)]
fn hard_reset_replaces_a_typechanged_symlink_without_writing_through_it() {
    let out = common::outside_dir();
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "tc", "plain\n");
    common::commit_all(&repo, "base");
    common::replace_with_link(dir.path(), "tc", &out.path().join("target.txt"));
    let head = repo.head().unwrap().target().unwrap().to_string();

    reset_to(&repo, &head, ResetMode::Hard).unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "tc"));
    assert_eq!(common::read(dir.path(), "tc"), "plain\n");
}

#[test]
#[cfg(unix)]
fn hard_reset_replaces_a_symlinked_directory_without_writing_through_it() {
    let out = common::outside_dir();
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "d/f", "inner\n");
    common::commit_all(&repo, "base");
    common::replace_with_link(dir.path(), "d", out.path());
    let head = repo.head().unwrap().target().unwrap().to_string();

    reset_to(&repo, &head, ResetMode::Hard).unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "d"));
    assert_eq!(common::read(dir.path(), "d/f"), "inner\n");
}

#[test]
#[cfg(unix)]
fn hard_reset_to_another_commit_does_not_write_through_a_link() {
    let out = common::outside_dir();
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "tc", "plain\n");
    common::commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap().to_string();
    common::write_file(dir.path(), "tc", "second\n");
    common::commit_all(&repo, "second");
    common::replace_with_link(dir.path(), "tc", &out.path().join("target.txt"));

    reset_to(&repo, &base, ResetMode::Hard).unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "tc"));
    assert_eq!(common::read(dir.path(), "tc"), "plain\n");
}

#[test]
#[cfg(unix)]
fn hard_reset_never_deletes_through_a_symlinked_directory() {
    let out = common::outside_dir();
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap().to_string();
    common::write_file(dir.path(), "d/f", "inner\n");
    common::commit_all(&repo, "adds d/f");
    common::replace_with_link(dir.path(), "d", out.path());

    reset_to(&repo, &base, ResetMode::Hard).unwrap();

    common::assert_outside_untouched(out.path());
}

/// Windows: a hard reset whose checkout stops at a file another program keeps open says so,
/// not libgit2's raw error, and leaves HEAD where it was; once the file is closed, resetting
/// again finishes it (#303).
#[cfg(windows)]
#[test]
fn hard_reset_with_a_file_open_in_another_program_says_so_and_finishes_when_tried_again() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "1\n");
    common::write_file(dir.path(), "z.txt", "1\n");
    common::commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap().id();
    common::write_file(dir.path(), "a.txt", "2\n");
    common::write_file(dir.path(), "z.txt", "2\n");
    common::commit_all(&repo, "next");
    let next = repo.head().unwrap().peel_to_commit().unwrap().id();
    let editor = common::hold_open_without_share_delete(&dir.path().join("z.txt"));

    let error = reset_to(&repo, &base.to_string(), ResetMode::Hard).unwrap_err();

    assert!(
        matches!(error, ResetError::HeldOpen(ref held) if held.path == "z.txt"),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "'z.txt' is open in another program. Some files were already reset. Close it, then try \
         again to finish."
    );
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), next);
    assert_eq!(common::read(dir.path(), "z.txt"), "2\n");
    drop(editor);
    reset_to(&repo, &base.to_string(), ResetMode::Hard).unwrap();
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), base);
    assert_eq!(common::read(dir.path(), "a.txt"), "1\n");
    assert_eq!(common::read(dir.path(), "z.txt"), "1\n");
    assert!(repo.statuses(None).unwrap().is_empty());
}

/// git writes the index first and moves the ref last (#796): a mixed or hard reset whose index
/// can't be written (another git command holds `index.lock`) fails with HEAD and the branch still
/// at the old commit, so nothing that sees HEAD at the target can find the old index under it.
#[test]
fn a_reset_that_cannot_write_the_index_leaves_head_where_it_was() {
    for mode in [ResetMode::Mixed, ResetMode::Hard] {
        let (_dir, repo, first) = three_commits();
        let main = repo.head().unwrap().name().unwrap().to_string();
        let tip = repo.head().unwrap().target().unwrap();
        let lock = repo.path().join("index.lock");
        std::fs::write(&lock, "").unwrap();

        let result = reset_to(&repo, &first.to_string(), mode);

        assert!(result.is_err(), "{mode:?}: {result:?}");
        assert_eq!(
            repo.find_reference(&main).unwrap().target(),
            Some(tip),
            "{mode:?}"
        );
        assert_eq!(repo.head().unwrap().target(), Some(tip), "{mode:?}");
        // The lock is the other command's; the reset leaves it alone.
        assert!(lock.exists(), "{mode:?}");
    }
}
