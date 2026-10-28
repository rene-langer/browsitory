mod common;

use git_core::commit::{amend_head, commit, CommitError};

fn repo_with_two_commits() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    common::write_file(dir.path(), "b.txt", "b\n");
    common::commit_all(&repo, "second");
    (dir, repo)
}

#[test]
fn amend_folds_staged_changes_and_new_message_into_head() {
    let (dir, repo) = repo_with_two_commits();
    let old_head = repo.head().unwrap().peel_to_commit().unwrap();
    let parent_id = old_head.parent_id(0).unwrap();
    common::write_file(dir.path(), "c.txt", "c\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("c.txt")).unwrap();
    index.write().unwrap();

    let new_id = amend_head(
        &repo,
        "second, amended",
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.id().to_string(), new_id);
    assert_ne!(head.id(), old_head.id());
    assert_eq!(head.message().unwrap(), "second, amended");
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![parent_id]);
    assert_eq!(head.author().name(), old_head.author().name());
    assert!(head.tree().unwrap().get_name("c.txt").is_some());
    assert!(
        repo.head().unwrap().is_branch(),
        "branch ref moved, HEAD still attached"
    );
}

#[test]
fn message_only_amend_keeps_tree() {
    let (_dir, repo) = repo_with_two_commits();
    let old_tree = repo.head().unwrap().peel_to_commit().unwrap().tree_id();
    amend_head(
        &repo,
        "reworded",
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.tree_id(), old_tree);
    assert_eq!(head.message().unwrap(), "reworded");
}

#[test]
fn amend_works_on_detached_head() {
    let (_dir, repo) = repo_with_two_commits();
    let oid = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(oid).unwrap();
    let new_id = amend_head(
        &repo,
        "detached amend",
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    assert_eq!(repo.head().unwrap().target().unwrap().to_string(), new_id);
    assert!(repo.head_detached().unwrap());
}

#[test]
fn amend_refuses_unborn_head() {
    let (_dir, repo) = common::init_repo();
    assert!(matches!(
        amend_head(&repo, "x", None, git_core::hooks::HookOptions::default()),
        Err(CommitError::NothingToAmend)
    ));
}

#[test]
fn amend_refuses_during_a_merge() {
    let (_dir, repo) = repo_with_two_commits();
    let oid = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{oid}\n")).unwrap();
    assert!(matches!(
        amend_head(&repo, "x", None, git_core::hooks::HookOptions::default()),
        Err(CommitError::OperationInProgress)
    ));
}

#[test]
fn commit_finishes_a_paused_cherry_pick_with_a_single_parent_and_clean_state() {
    let (dir, mut repo) = repo_with_two_commits();
    let head = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("CHERRY_PICK_HEAD"), format!("{head}\n")).unwrap();
    common::write_file(dir.path(), "d.txt", "d\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("d.txt")).unwrap();
    index.write().unwrap();

    commit(&mut repo, "picked", git_core::hooks::HookOptions::default()).unwrap();

    let new_head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(new_head.parent_count(), 1);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn amend_with_an_author_override_keeps_the_committer_and_the_original_author_time() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    // An author time well in the past with a non-UTC offset, so "kept" is observable.
    {
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let when = git2::Time::new(1_000_000_000, 120);
        let sig = git2::Signature::new("Old Name", "old@x.org", &when).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "first", &tree, &[])
            .unwrap();
    }

    let author = git_core::commit::SignatureInput {
        name: "  Ada Lovelace ".into(),
        email: " ada@x.org ".into(),
    };
    amend_head(
        &repo,
        "first, reworded",
        Some(&author),
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.author().name().ok(), Some("Ada Lovelace"));
    assert_eq!(head.author().email().ok(), Some("ada@x.org"));
    assert_eq!(head.author().when().seconds(), 1_000_000_000);
    assert_eq!(head.author().when().offset_minutes(), 120);
    assert_eq!(head.committer().name().ok(), Some("Test User"));
    assert_eq!(head.committer().email().ok(), Some("test@example.com"));
    assert_eq!(head.message().ok(), Some("first, reworded"));
}

#[test]
fn amend_without_an_override_keeps_the_author() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let before = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .author()
        .to_owned();
    amend_head(
        &repo,
        "again",
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.author().name().ok(), before.name().ok());
    assert_eq!(head.author().email().ok(), before.email().ok());
}

#[test]
fn amend_refuses_an_invalid_author_and_leaves_head_alone() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let head = repo.head().unwrap().target().unwrap();
    for (name, email) in [
        ("", "a@x.org"),
        ("  ", "a@x.org"),
        ("Ada", ""),
        ("A<da", "a@x.org"),
        ("Ada>", "a@x.org"),
        ("Ada", "a@x\n.org"),
        ("Ada\nEvil", "a@x.org"),
        ("Ada", "a <b>@x.org"),
        ("Ada", "a b@x.org"),
        ("Ada\0", "a@x.org"),
    ] {
        let author = git_core::commit::SignatureInput {
            name: name.into(),
            email: email.into(),
        };
        assert!(
            matches!(
                amend_head(
                    &repo,
                    "first",
                    Some(&author),
                    git_core::hooks::HookOptions::default()
                ),
                Err(CommitError::InvalidAuthor)
            ),
            "{name:?} {email:?}"
        );
    }
    assert_eq!(repo.head().unwrap().target().unwrap(), head);
    assert_eq!(
        CommitError::InvalidAuthor.to_string(),
        "Invalid author. Use a name and an email address."
    );
}
