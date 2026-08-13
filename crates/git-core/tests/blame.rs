mod common;

use common::{commit_all, init_repo, write_file};

#[test]
fn blame_file_attributes_all_lines_to_the_single_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "line one\nline two\nline three\n");
    commit_all(&repo, "initial commit");

    let lines = git_core::blame::blame_file(&repo, "HEAD", "file.txt").unwrap();

    assert_eq!(lines.len(), 3);
    let first_commit_id = lines[0].commit_id.clone();
    assert!(lines.iter().all(|l| l.commit_id == first_commit_id));
    assert_eq!(lines[0].content, "line one");
    assert_eq!(lines[0].line_number, 1);
    assert_eq!(lines[0].author_name, "Test User");
    assert_eq!(lines[0].short_id.len(), 7);
}

#[test]
fn blame_file_reattributes_only_the_changed_lines_after_an_edit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "line one\nline two\nline three\n");
    commit_all(&repo, "first commit");
    let first_commit_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    write_file(dir.path(), "file.txt", "line one\nCHANGED\nline three\n");
    commit_all(&repo, "second commit");
    let second_commit_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    let lines = git_core::blame::blame_file(&repo, "HEAD", "file.txt").unwrap();

    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].commit_id, first_commit_id);
    assert_eq!(lines[1].commit_id, second_commit_id);
    assert_eq!(lines[1].content, "CHANGED");
    assert_eq!(lines[2].commit_id, first_commit_id);
}

#[test]
fn blame_file_at_a_specific_historic_commit_only_sees_lines_up_to_that_point() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "line one\n");
    commit_all(&repo, "first commit");
    let first_commit_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    write_file(dir.path(), "file.txt", "line one\nline two\n");
    commit_all(&repo, "second commit");

    let lines = git_core::blame::blame_file(&repo, &first_commit_id, "file.txt").unwrap();

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "line one");
}

#[test]
fn blame_file_on_a_missing_path_returns_an_error() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "content\n");
    commit_all(&repo, "initial commit");

    let result = git_core::blame::blame_file(&repo, "HEAD", "nonexistent.txt");

    assert!(result.is_err());
}
