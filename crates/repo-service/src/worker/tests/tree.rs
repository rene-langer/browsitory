use super::*;

fn head_id(repo: &Repository) -> String {
    repo.head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string()
}

#[test]
fn browses_a_commits_tree_and_reads_a_file_through_the_worker() {
    let (dir, repo) = init_repo();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    write_file(dir.path(), "src/main.rs", "fn main() {}\n");
    write_file(dir.path(), "README.md", "hi\n");
    commit_all(&repo, "initial");
    let id = head_id(&repo);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let root = handle
        .get_commit_tree(id.clone(), String::new(), None)
        .unwrap();
    let paths: Vec<&str> = root.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, vec!["src", "README.md"]);

    let found = handle
        .get_commit_tree(id.clone(), String::new(), Some("main".into()))
        .unwrap();
    assert_eq!(found.entries.len(), 1);
    assert_eq!(found.entries[0].path, "src/main.rs");

    // A blank filter lists the folder.
    let blank = handle
        .get_commit_tree(id.clone(), "src".into(), Some("  ".into()))
        .unwrap();
    assert_eq!(blank.entries[0].path, "src/main.rs");

    assert_eq!(
        handle.get_file_at_commit(id, "README.md".into()).unwrap(),
        git_core::tree::FileContent::Text {
            text: "hi\n".into(),
            size: 3
        }
    );
    // Reads record nothing.
    assert_eq!(
        handle.get_undo_state().unwrap(),
        UndoState {
            undo: None,
            redo: None
        }
    );
}

#[test]
fn restoring_a_file_version_is_one_undo_step() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "old\n");
    commit_all(&repo, "first");
    let first = head_id(&repo);
    write_file(dir.path(), "a.txt", "new\n");
    commit_all(&repo, "second");
    write_file(dir.path(), "a.txt", "unsaved work\n");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    handle
        .restore_file_version(first.clone(), "a.txt".into())
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "old\n"
    );
    let label = format!("Restore a.txt from {}", &first[..7]);
    assert_eq!(
        handle.get_undo_state().unwrap(),
        UndoState {
            undo: Some(label.clone()),
            redo: None
        }
    );

    assert_eq!(handle.undo().unwrap(), UndoOutcome::Done { label });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "unsaved work\n"
    );
}

#[test]
fn saves_a_file_version_outside_the_repository() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "content\n");
    commit_all(&repo, "first");
    let id = head_id(&repo);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let outside = TempDir::new().unwrap();
    let destination = outside.path().join("copy.txt");

    handle
        .save_file_version(id, "a.txt".into(), destination.clone())
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), b"content\n");
    assert_eq!(
        handle.get_undo_state().unwrap(),
        UndoState {
            undo: None,
            redo: None
        }
    );
}
