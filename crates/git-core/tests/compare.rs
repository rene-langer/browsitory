mod common;

use common::{commit_all, commit_tree, init_repo, write_file};
use git2::Oid;
use git_core::diff::{
    branch_compare_range, compare_working_tree, compare_working_tree_files, range_diff,
    range_diff_files, ChangedFile, CompareRange, DiffError, DiffHunk, DiffLineOrigin, DiffOptions,
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
fn head_id(repo: &git2::Repository) -> String {
    repo.head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string()
}

#[test]
fn working_tree_lists_staged_unstaged_untracked_and_deleted_files() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "gone.txt", "gone\n");
    commit_all(&repo, "base");
    let base = head_id(&repo);
    write_file(dir.path(), "a.txt", "a changed\n");
    write_file(dir.path(), "s.txt", "staged\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("s.txt")).unwrap();
    index.write().unwrap();
    write_file(dir.path(), "u.txt", "untracked\n");
    std::fs::remove_file(dir.path().join("gone.txt")).unwrap();

    let files = compare_working_tree_files(&repo, &base).unwrap();
    assert_eq!(
        sorted(files),
        vec![
            changed("a.txt"),
            changed("gone.txt"),
            changed("s.txt"),
            changed("u.txt")
        ]
    );

    let untracked =
        compare_working_tree(&repo, &base, "u.txt", None, &DiffOptions::default()).unwrap();
    assert_eq!(
        edits(&untracked),
        vec![(DiffLineOrigin::Add, "untracked".into())]
    );
}

#[test]
fn working_tree_against_an_older_commit_includes_later_commits() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "first");
    let first = head_id(&repo);
    write_file(dir.path(), "a.txt", "two\n");
    commit_all(&repo, "second");
    write_file(dir.path(), "a.txt", "three\n");

    let hunks =
        compare_working_tree(&repo, &first, "a.txt", None, &DiffOptions::default()).unwrap();

    assert_eq!(
        edits(&hunks),
        vec![
            (DiffLineOrigin::Remove, "one".into()),
            (DiffLineOrigin::Add, "three".into())
        ]
    );
}

/// base ← main (adds main.txt), base ← feature (adds f.txt); HEAD on main.
fn diverged(repo: &git2::Repository) -> (Oid, Oid, Oid) {
    let base = commit_tree(repo, &[("shared.txt", "s\n", REGULAR)], &[], "base");
    let main = commit_tree(
        repo,
        &[("shared.txt", "s\n", REGULAR), ("main.txt", "m\n", REGULAR)],
        &[base],
        "main",
    );
    let feature = commit_tree(
        repo,
        &[("shared.txt", "s\n", REGULAR), ("f.txt", "f\n", REGULAR)],
        &[base],
        "feature",
    );
    repo.reference("refs/heads/main", main, true, "test")
        .unwrap();
    repo.reference("refs/heads/feature", feature, true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    (base, main, feature)
}

#[test]
fn branch_compare_uses_the_merge_base_and_ignores_head_only_commits() {
    let (_dir, repo) = init_repo();
    let (base, _main, feature) = diverged(&repo);

    let range = branch_compare_range(&repo, "refs/heads/feature").unwrap();
    assert_eq!(
        range,
        CompareRange {
            from_id: base.to_string(),
            to_id: feature.to_string()
        }
    );

    let files = range_diff_files(&repo, &range.from_id, &range.to_id).unwrap();
    assert_eq!(files, vec![changed("f.txt")]);
}

#[test]
fn a_remote_tracking_branch_compares_the_same_way() {
    let (_dir, repo) = init_repo();
    let (base, _main, feature) = diverged(&repo);
    repo.reference("refs/remotes/origin/feature", feature, true, "test")
        .unwrap();

    let range = branch_compare_range(&repo, "refs/remotes/origin/feature").unwrap();

    assert_eq!(
        range,
        CompareRange {
            from_id: base.to_string(),
            to_id: feature.to_string()
        }
    );
}

#[test]
fn a_detached_head_compares_with_head() {
    let (_dir, repo) = init_repo();
    let (base, main, feature) = diverged(&repo);
    repo.set_head_detached(main).unwrap();

    let range = branch_compare_range(&repo, "refs/heads/feature").unwrap();

    assert_eq!(
        range,
        CompareRange {
            from_id: base.to_string(),
            to_id: feature.to_string()
        }
    );
}

#[test]
fn unrelated_history_has_no_merge_base() {
    let (_dir, repo) = init_repo();
    diverged(&repo);
    let orphan = commit_tree(&repo, &[("x.txt", "x\n", REGULAR)], &[], "orphan");
    repo.reference("refs/heads/orphan", orphan, true, "test")
        .unwrap();

    let error = branch_compare_range(&repo, "refs/heads/orphan").unwrap_err();

    assert!(matches!(error, DiffError::NoMergeBase(_)));
    assert_eq!(
        error.to_string(),
        "orphan and HEAD have no common ancestor."
    );
}

#[test]
fn an_unknown_branch_is_reported_by_its_short_name() {
    let (_dir, repo) = init_repo();
    diverged(&repo);

    let error = branch_compare_range(&repo, "refs/heads/nope").unwrap_err();

    assert_eq!(error.to_string(), "No branch named \"nope\".");
}

#[test]
fn an_unborn_head_has_nothing_to_compare_with() {
    let (_dir, repo) = init_repo();
    let only = commit_tree(&repo, &[("x.txt", "x\n", REGULAR)], &[], "only");
    repo.reference("refs/heads/compare-unborn-target", only, true, "test")
        .unwrap();
    // Point HEAD at an unborn branch explicitly, whatever `init.defaultBranch` says.
    repo.set_head("refs/heads/compare-unborn-head").unwrap();

    let error = branch_compare_range(&repo, "refs/heads/compare-unborn-target").unwrap_err();

    assert!(matches!(error, DiffError::UnbornHead));
    assert_eq!(
        error.to_string(),
        "There is no commit checked out to compare with."
    );
}

/// Commit A: `d` is a file; commit B: `d` is a folder holding `inner.txt` (#116).
fn file_became_folder(repo: &git2::Repository) -> (Oid, Oid) {
    let a = commit_tree(repo, &[("d", "file d\n", REGULAR)], &[], "a");
    let b = commit_tree(repo, &[("d/inner.txt", "inner\n", REGULAR)], &[a], "b");
    (a, b)
}

#[test]
fn range_diff_of_a_file_that_became_a_folder_shows_only_that_file() {
    let (_dir, repo) = init_repo();
    let (a, b) = file_became_folder(&repo);
    let (a, b) = (a.to_string(), b.to_string());

    assert_eq!(
        sorted(range_diff_files(&repo, &a, &b).unwrap()),
        vec![changed("d"), changed("d/inner.txt")]
    );
    let opts = DiffOptions::default();
    assert_eq!(
        edits(&range_diff(&repo, &a, &b, "d", None, &opts).unwrap()),
        vec![(DiffLineOrigin::Remove, "file d".to_string())]
    );
    assert_eq!(
        edits(&range_diff(&repo, &a, &b, "d/inner.txt", None, &opts).unwrap()),
        vec![(DiffLineOrigin::Add, "inner".to_string())]
    );
}

#[test]
fn working_tree_compare_of_a_file_that_became_a_folder_shows_only_that_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "d", "file d\n");
    commit_all(&repo, "a");
    let a = repo.head().unwrap().target().unwrap().to_string();
    std::fs::remove_file(dir.path().join("d")).unwrap();
    write_file(dir.path(), "d/inner.txt", "inner\n");

    let opts = DiffOptions::default();
    assert_eq!(
        edits(&compare_working_tree(&repo, &a, "d", None, &opts).unwrap()),
        vec![(DiffLineOrigin::Remove, "file d".to_string())]
    );
}
