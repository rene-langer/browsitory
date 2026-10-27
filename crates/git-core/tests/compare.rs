mod common;

use common::{commit_tree, init_repo};
use git2::Oid;
use git_core::diff::{
    range_diff, range_diff_files, ChangedFile, DiffHunk, DiffLineOrigin, DiffOptions,
};

const REGULAR: u32 = 0o100644;
const FIVE_LINES: &str = "l1\nl2\nl3\nl4\nl5\n";

fn changed(path: &str) -> ChangedFile {
    ChangedFile {
        path: path.into(),
        old_path: None,
    }
}

/// (origin, content without the line ending) of every added or removed line.
fn edits(hunks: &[DiffHunk]) -> Vec<(DiffLineOrigin, String)> {
    hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.origin != DiffLineOrigin::Context)
        .map(|l| (l.origin, l.content.trim_end().to_string()))
        .collect()
}

fn sorted(mut files: Vec<ChangedFile>) -> Vec<ChangedFile> {
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

/// c1: a=one, b ; c2: a=two, b, c ; c3: a=three, c, d (b deleted).
fn three_commits(repo: &git2::Repository) -> (Oid, Oid, Oid) {
    let c1 = commit_tree(
        repo,
        &[("a.txt", "one\n", REGULAR), ("b.txt", "b\n", REGULAR)],
        &[],
        "c1",
    );
    let c2 = commit_tree(
        repo,
        &[
            ("a.txt", "two\n", REGULAR),
            ("b.txt", "b\n", REGULAR),
            ("c.txt", "c\n", REGULAR),
        ],
        &[c1],
        "c2",
    );
    let c3 = commit_tree(
        repo,
        &[
            ("a.txt", "three\n", REGULAR),
            ("c.txt", "c\n", REGULAR),
            ("d.txt", "d\n", REGULAR),
        ],
        &[c2],
        "c3",
    );
    (c1, c2, c3)
}

#[test]
fn range_files_of_non_adjacent_commits_list_every_net_change() {
    let (_dir, repo) = init_repo();
    let (c1, _c2, c3) = three_commits(&repo);

    let files = range_diff_files(&repo, &c1.to_string(), &c3.to_string()).unwrap();

    assert_eq!(
        sorted(files),
        vec![
            changed("a.txt"),
            changed("b.txt"),
            changed("c.txt"),
            changed("d.txt")
        ]
    );
}

#[test]
fn range_diff_shows_the_net_change_not_each_step() {
    let (_dir, repo) = init_repo();
    let (c1, _c2, c3) = three_commits(&repo);

    let hunks = range_diff(
        &repo,
        &c1.to_string(),
        &c3.to_string(),
        "a.txt",
        None,
        &DiffOptions::default(),
    )
    .unwrap();

    assert_eq!(
        edits(&hunks),
        vec![
            (DiffLineOrigin::Remove, "one".into()),
            (DiffLineOrigin::Add, "three".into())
        ]
    );
}

#[test]
fn reversed_range_swaps_adds_and_removes() {
    let (_dir, repo) = init_repo();
    let (c1, _c2, c3) = three_commits(&repo);
    let opts = DiffOptions::default();

    let a = range_diff(
        &repo,
        &c3.to_string(),
        &c1.to_string(),
        "a.txt",
        None,
        &opts,
    )
    .unwrap();
    assert_eq!(
        edits(&a),
        vec![
            (DiffLineOrigin::Remove, "three".into()),
            (DiffLineOrigin::Add, "one".into())
        ]
    );
    let d = range_diff(
        &repo,
        &c3.to_string(),
        &c1.to_string(),
        "d.txt",
        None,
        &opts,
    )
    .unwrap();
    assert_eq!(edits(&d), vec![(DiffLineOrigin::Remove, "d".into())]);
}

#[test]
fn a_range_from_a_commit_to_itself_is_empty() {
    let (_dir, repo) = init_repo();
    let (_c1, c2, _c3) = three_commits(&repo);

    assert!(range_diff_files(&repo, &c2.to_string(), &c2.to_string())
        .unwrap()
        .is_empty());
}

#[test]
fn a_file_deleted_in_the_range_diffs_as_all_removed() {
    let (_dir, repo) = init_repo();
    let (c1, _c2, c3) = three_commits(&repo);

    let hunks = range_diff(
        &repo,
        &c1.to_string(),
        &c3.to_string(),
        "b.txt",
        None,
        &DiffOptions::default(),
    )
    .unwrap();

    assert_eq!(edits(&hunks), vec![(DiffLineOrigin::Remove, "b".into())]);
}

#[test]
fn a_rename_across_the_range_is_one_entry_with_only_the_edit() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("new.txt", FIVE_LINES, REGULAR)], &[c1], "rename");
    let c3 = commit_tree(
        &repo,
        &[
            ("new.txt", "l1\nl2\nl3\nl4\nCHANGED\n", REGULAR),
            ("other.txt", "x\n", REGULAR),
        ],
        &[c2],
        "edit",
    );

    let files = range_diff_files(&repo, &c1.to_string(), &c3.to_string()).unwrap();
    assert_eq!(
        sorted(files),
        vec![
            ChangedFile {
                path: "new.txt".into(),
                old_path: Some("old.txt".into())
            },
            changed("other.txt"),
        ]
    );

    let hunks = range_diff(
        &repo,
        &c1.to_string(),
        &c3.to_string(),
        "new.txt",
        Some("old.txt"),
        &DiffOptions::default(),
    )
    .unwrap();
    assert_eq!(
        edits(&hunks),
        vec![
            (DiffLineOrigin::Remove, "l5".into()),
            (DiffLineOrigin::Add, "CHANGED".into())
        ]
    );
}

#[test]
fn an_unknown_commit_is_an_error() {
    let (_dir, repo) = init_repo();
    let (c1, _c2, _c3) = three_commits(&repo);

    assert!(range_diff_files(&repo, &"0".repeat(40), &c1.to_string()).is_err());
}
