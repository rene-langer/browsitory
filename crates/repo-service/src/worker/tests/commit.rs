use super::*;

fn co_author(value: &str) -> git_core::commit::Trailer {
    git_core::commit::Trailer {
        key: "Co-authored-by".into(),
        value: value.into(),
    }
}

fn head_commit_message(repo: &Repository) -> String {
    repo.head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .message()
        .unwrap()
        .to_string()
}

#[test]
fn get_file_sides_returns_both_sides_of_an_unstaged_edit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "img.png", "foo");
    commit_all(&repo, "first");
    write_file(dir.path(), "img.png", "foobar");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let sides = worker
        .handle()
        .get_file_sides("img.png".into(), git_core::tools::DiffSource::Unstaged)
        .unwrap();
    assert_eq!(
        sides.old,
        git_core::diff::FileSide::Loaded {
            bytes: b"foo".to_vec()
        }
    );
    assert_eq!(
        sides.new,
        git_core::diff::FileSide::Loaded {
            bytes: b"foobar".to_vec()
        }
    );
    assert!(worker
        .handle()
        .get_file_sides("../x.png".into(), git_core::tools::DiffSource::Unstaged)
        .unwrap_err()
        .message
        .contains("path must be relative to the repository"));
}

#[test]
fn commit_appends_trailers_and_refuses_an_invalid_one() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("a.txt".into()).unwrap();

    let bad = git_core::commit::Trailer {
        key: "Bad key".into(),
        value: "x".into(),
    };
    assert_eq!(
        handle
            .commit("Add a".into(), false, vec![bad], mpsc::channel().0)
            .unwrap_err(),
        crate::wire::ErrorDto::from("Invalid trailer \"Bad key: x\".")
    );
    assert!(repo.head().is_err(), "nothing was committed");

    let outcome = handle
        .commit(
            "Add a".into(),
            false,
            vec![co_author("Ada <ada@x.org>")],
            mpsc::channel().0,
        )
        .unwrap();
    assert!(matches!(
        outcome,
        git_core::commit::CommitOutcome::Committed { .. }
    ));
    assert_eq!(
        head_commit_message(&repo),
        "Add a\n\nCo-authored-by: Ada <ada@x.org>\n"
    );
}

#[test]
fn an_empty_message_with_trailers_is_refused_for_commit_and_amend() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("a.txt".into()).unwrap();

    assert_eq!(
        handle
            .commit(
                " \n\n".into(),
                false,
                vec![co_author("Ada <ada@x.org>")],
                mpsc::channel().0,
            )
            .unwrap_err(),
        crate::wire::ErrorDto::from("Aborting commit due to empty commit message.")
    );
    assert!(repo.head().is_err(), "nothing was committed");

    handle
        .commit("Add a".into(), false, vec![], mpsc::channel().0)
        .unwrap();
    let before = repo.head().unwrap().target();
    assert_eq!(
        handle
            .amend_commit(
                "".into(),
                false,
                None,
                vec![co_author("Ada <ada@x.org>")],
                mpsc::channel().0,
            )
            .unwrap_err(),
        crate::wire::ErrorDto::from("Aborting commit due to empty commit message.")
    );
    assert_eq!(
        repo.head().unwrap().target(),
        before,
        "HEAD was not amended"
    );
}

#[cfg(unix)]
#[test]
fn the_commit_msg_hook_sees_the_trailers() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    let seen = dir.path().join("seen.txt");
    install_hook(
        &repo,
        "commit-msg",
        &format!("cp \"$1\" '{}'", seen.display()),
    );
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("a.txt".into()).unwrap();
    handle
        .commit(
            "Add a".into(),
            false,
            vec![co_author("Ada <ada@x.org>")],
            mpsc::channel().0,
        )
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&seen).unwrap(),
        "Add a\n\nCo-authored-by: Ada <ada@x.org>\n"
    );
}

#[test]
fn amend_commit_overrides_the_author_and_appends_trailers() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "first");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .amend_commit(
            "first".into(),
            false,
            Some(git_core::commit::SignatureInput {
                name: "Ada".into(),
                email: "ada@x.org".into(),
            }),
            vec![co_author("Bob <bob@x.org>")],
            mpsc::channel().0,
        )
        .unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.author().name().ok(), Some("Ada"));
    assert_eq!(head.author().email().ok(), Some("ada@x.org"));
    assert_eq!(head.committer().name().ok(), Some("Test User"));
    assert_eq!(
        head.message().ok(),
        Some("first\n\nCo-authored-by: Bob <bob@x.org>\n")
    );
    drop(head);

    let before = repo.head().unwrap().target();
    assert_eq!(
        handle
            .amend_commit(
                "first".into(),
                false,
                Some(git_core::commit::SignatureInput {
                    name: "Ada".into(),
                    email: "".into(),
                }),
                Vec::new(),
                mpsc::channel().0,
            )
            .unwrap_err(),
        crate::wire::ErrorDto::from("Invalid author. Use a name and an email address.")
    );
    assert_eq!(repo.head().unwrap().target(), before);
}

#[test]
fn list_recent_authors_excludes_the_current_user() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "mine");
    write_file(dir.path(), "b.txt", "b");
    {
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("b.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let ada = git2::Signature::now("Ada", "ada@x.org").unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &ada, &ada, "hers", &tree, &[&parent])
            .unwrap();
    }
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    assert_eq!(
        worker.handle().list_recent_authors().unwrap(),
        vec![git_core::commit::SignatureInput {
            name: "Ada".into(),
            email: "ada@x.org".into()
        }]
    );
}
