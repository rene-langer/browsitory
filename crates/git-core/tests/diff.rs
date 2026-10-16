use git_core::diff::DiffOptions;
mod common;

use std::path::Path;

use common::{commit_all, init_repo, write_file};
use git_core::diff::DiffLineOrigin;

#[test]
fn working_diff_unstaged_shows_added_and_context_lines() {
    let (dir, repo) = init_repo();
    write_file(
        dir.path(),
        "tracked.txt",
        "line one\nline two\nline three\n",
    );
    commit_all(&repo, "initial commit");
    write_file(
        dir.path(),
        "tracked.txt",
        "line one\nline two changed\nline three\n",
    );

    let hunks =
        git_core::diff::working_diff(&repo, "tracked.txt", false, &DiffOptions::default()).unwrap();

    assert_eq!(hunks.len(), 1);
    let lines = &hunks[0].lines;
    assert!(lines
        .iter()
        .any(|line| line.origin == DiffLineOrigin::Remove && line.content.trim() == "line two"));
    assert!(lines.iter().any(
        |line| line.origin == DiffLineOrigin::Add && line.content.trim() == "line two changed"
    ));
}

#[test]
fn working_diff_staged_shows_the_staged_content() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "new.txt", "hello\nworld\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();

    let staged_hunks =
        git_core::diff::working_diff(&repo, "new.txt", true, &DiffOptions::default()).unwrap();

    assert!(!staged_hunks.is_empty());
    let concatenated: String = staged_hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| {
            assert_eq!(line.origin, DiffLineOrigin::Add);
            line.content.clone()
        })
        .collect();
    assert_eq!(concatenated, "hello\nworld\n");

    let unstaged_hunks =
        git_core::diff::working_diff(&repo, "new.txt", false, &DiffOptions::default()).unwrap();
    assert!(unstaged_hunks.is_empty());
}

/// An untracked file is reported by `status()` (which sets `include_untracked(true)`) as an
/// unstaged `New` row, so clicking it must show its content — not an empty "No differences"
/// pane, which is what plain `git diff` semantics would produce.
#[test]
fn working_diff_unstaged_shows_the_content_of_an_untracked_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "untracked.txt", "brand\nnew\nfile\n");

    let hunks =
        git_core::diff::working_diff(&repo, "untracked.txt", false, &DiffOptions::default())
            .unwrap();

    assert!(!hunks.is_empty());
    let concatenated: String = hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| {
            assert_eq!(line.origin, DiffLineOrigin::Add);
            line.content.clone()
        })
        .collect();
    assert_eq!(concatenated, "brand\nnew\nfile\n");
}

#[test]
fn commit_diff_shows_the_change_introduced_by_that_commit() {
    let (dir, repo) = init_repo();
    write_file(
        dir.path(),
        "tracked.txt",
        "line one\nline two\nline three\n",
    );
    commit_all(&repo, "initial commit");
    write_file(
        dir.path(),
        "tracked.txt",
        "line one\nline two changed\nline three\n",
    );
    commit_all(&repo, "change middle line");

    let second_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    let hunks =
        git_core::diff::commit_diff(&repo, &second_id, "tracked.txt", &DiffOptions::default())
            .unwrap();

    assert_eq!(hunks.len(), 1);
    let lines = &hunks[0].lines;
    assert!(lines
        .iter()
        .any(|line| line.origin == DiffLineOrigin::Remove && line.content.trim() == "line two"));
    assert!(lines.iter().any(
        |line| line.origin == DiffLineOrigin::Add && line.content.trim() == "line two changed"
    ));
}

#[test]
fn commit_diff_on_the_first_commit_shows_every_line_as_added() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");

    let commit_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    let hunks =
        git_core::diff::commit_diff(&repo, &commit_id, "tracked.txt", &DiffOptions::default())
            .unwrap();

    assert!(!hunks.is_empty());
    for hunk in &hunks {
        for line in &hunk.lines {
            assert_eq!(line.origin, DiffLineOrigin::Add);
        }
    }
}

#[test]
fn commit_files_lists_every_changed_path() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "one.txt", "content one");
    write_file(dir.path(), "two.txt", "content two");
    commit_all(&repo, "add two files");

    let commit_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    let mut result = git_core::diff::commit_files(&repo, &commit_id).unwrap();
    result.sort();
    let mut expected = vec!["one.txt".to_string(), "two.txt".to_string()];
    expected.sort();

    assert_eq!(result, expected);
}

fn twenty_lines() -> String {
    (1..=20).map(|n| format!("line {n}\n")).collect()
}

#[test]
fn context_lines_option_merges_or_splits_hunks() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", &twenty_lines());
    common::commit_all(&repo, "base");
    let changed = twenty_lines()
        .replace("line 2\n", "line 2 changed\n")
        .replace("line 10\n", "line 10 changed\n");
    common::write_file(dir.path(), "f.txt", &changed);

    let three = DiffOptions {
        ignore_whitespace: false,
        context_lines: 3,
    };
    let five = DiffOptions {
        ignore_whitespace: false,
        context_lines: 5,
    };
    assert_eq!(
        git_core::diff::working_diff(&repo, "f.txt", false, &three)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        git_core::diff::working_diff(&repo, "f.txt", false, &five)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn context_lines_option_is_clamped_to_twenty() {
    let (dir, repo) = common::init_repo();
    common::write_file(
        dir.path(),
        "f.txt",
        &(1..=60).map(|n| format!("l{n}\n")).collect::<String>(),
    );
    common::commit_all(&repo, "base");
    common::write_file(
        dir.path(),
        "f.txt",
        &(1..=60)
            .map(|n| {
                if n == 30 {
                    "x\n".to_string()
                } else {
                    format!("l{n}\n")
                }
            })
            .collect::<String>(),
    );

    let huge = DiffOptions {
        ignore_whitespace: false,
        context_lines: 1000,
    };
    let hunk = &git_core::diff::working_diff(&repo, "f.txt", false, &huge).unwrap()[0];
    // 20 context lines on each side + the removed line.
    assert_eq!(hunk.old_lines, 41);
}

#[test]
fn ignore_whitespace_hides_whitespace_only_changes() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a b\nc\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a   b\nc\n");

    let raw = DiffOptions::default();
    let ignoring = DiffOptions {
        ignore_whitespace: true,
        context_lines: 3,
    };
    assert_eq!(
        git_core::diff::working_diff(&repo, "f.txt", false, &raw)
            .unwrap()
            .len(),
        1
    );
    assert!(
        git_core::diff::working_diff(&repo, "f.txt", false, &ignoring)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn commit_diff_honours_ignore_whitespace() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a b\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a  b\n");
    common::commit_all(&repo, "spaces");
    let head = repo.head().unwrap().target().unwrap().to_string();

    let ignoring = DiffOptions {
        ignore_whitespace: true,
        context_lines: 3,
    };
    assert!(
        git_core::diff::commit_diff(&repo, &head, "f.txt", &ignoring)
            .unwrap()
            .is_empty()
    );
}
