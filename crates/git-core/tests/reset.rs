mod common;

use git_core::reset::{reset_to, ResetError, ResetMode};

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
