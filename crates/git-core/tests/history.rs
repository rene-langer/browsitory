mod common;

use common::{commit_tree, init_repo};
use git2::{Oid, Repository};
use git_core::diff::{commit_rename_diff, DiffLineOrigin, DiffOptions};
use git_core::history::{file_history, FileHistoryEntry, HistoryError};

const REGULAR: u32 = 0o100644;
const EXECUTABLE: u32 = 0o100755;

fn set_head(repo: &Repository, tip: Oid) {
    repo.reference("refs/heads/main", tip, true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
}

fn summaries(entries: &[FileHistoryEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.commit.summary.as_str()).collect()
}

const FIVE_LINES: &str = "l1\nl2\nl3\nl4\nl5\n";
const FIVE_LINES_EDITED: &str = "l1\nl2\nl3\nl4\nCHANGED\n";

#[test]
fn lists_only_commits_that_change_the_file_newest_first() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(
        &repo,
        &[("a.txt", "1", REGULAR), ("b.txt", "1", REGULAR)],
        &[],
        "c1",
    );
    let c2 = commit_tree(
        &repo,
        &[("a.txt", "2", REGULAR), ("b.txt", "1", REGULAR)],
        &[c1],
        "c2",
    );
    let c3 = commit_tree(
        &repo,
        &[("a.txt", "2", REGULAR), ("b.txt", "2", REGULAR)],
        &[c2],
        "c3",
    );
    let c4 = commit_tree(
        &repo,
        &[("a.txt", "3", REGULAR), ("b.txt", "2", REGULAR)],
        &[c3],
        "c4",
    );
    set_head(&repo, c4);

    let history = file_history(&repo, "a.txt", 100).unwrap();

    assert_eq!(summaries(&history), vec!["c4", "c2", "c1"]);
    assert!(history
        .iter()
        .all(|e| e.path == "a.txt" && e.old_path.is_none()));
    assert_eq!(history[0].commit.branch_refs, vec!["main".to_string()]);
}

#[test]
fn follows_a_pure_rename() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("new.txt", FIVE_LINES, REGULAR)], &[c1], "rename");
    let c3 = commit_tree(
        &repo,
        &[("new.txt", FIVE_LINES_EDITED, REGULAR)],
        &[c2],
        "edit",
    );
    set_head(&repo, c3);

    let history = file_history(&repo, "new.txt", 100).unwrap();

    assert_eq!(summaries(&history), vec!["edit", "rename", "add"]);
    assert_eq!(history[0].path, "new.txt");
    assert_eq!(history[0].old_path, None);
    assert_eq!(history[1].path, "new.txt");
    assert_eq!(history[1].old_path.as_deref(), Some("old.txt"));
    assert_eq!(history[2].path, "old.txt");
    assert_eq!(history[2].old_path, None);
}

#[test]
fn follows_a_rename_with_an_edit_in_the_same_commit() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "add");
    let c2 = commit_tree(
        &repo,
        &[("new.txt", FIVE_LINES_EDITED, REGULAR)],
        &[c1],
        "rename and edit",
    );
    set_head(&repo, c2);

    let history = file_history(&repo, "new.txt", 100).unwrap();

    assert_eq!(summaries(&history), vec!["rename and edit", "add"]);
    assert_eq!(history[0].old_path.as_deref(), Some("old.txt"));
    assert_eq!(history[1].path, "old.txt");
}

#[test]
fn follows_each_lineage_with_its_own_path_across_a_merge() {
    let (_dir, repo) = init_repo();
    let base = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "base");
    let side = commit_tree(
        &repo,
        &[("new.txt", FIVE_LINES, REGULAR)],
        &[base],
        "rename on side",
    );
    let main = commit_tree(
        &repo,
        &[("old.txt", FIVE_LINES, REGULAR), ("z.txt", "z", REGULAR)],
        &[base],
        "unrelated on main",
    );
    let merge = commit_tree(
        &repo,
        &[("new.txt", FIVE_LINES, REGULAR), ("z.txt", "z", REGULAR)],
        &[main, side],
        "merge",
    );
    set_head(&repo, merge);

    let history = file_history(&repo, "new.txt", 100).unwrap();

    // The merge is TREESAME to `side` for new.txt, so it is hidden; `main` is TREESAME to `base`.
    assert_eq!(summaries(&history), vec!["rename on side", "base"]);
    assert_eq!(history[0].old_path.as_deref(), Some("old.txt"));
    assert_eq!(history[1].path, "old.txt");
}

#[test]
fn hides_a_merge_that_matches_one_parent() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("f.txt", "1", REGULAR)], &[], "c1");
    let side = commit_tree(&repo, &[("f.txt", "2", REGULAR)], &[c1], "side edit");
    let main = commit_tree(
        &repo,
        &[("f.txt", "1", REGULAR), ("g.txt", "1", REGULAR)],
        &[c1],
        "main other",
    );
    let merge = commit_tree(
        &repo,
        &[("f.txt", "2", REGULAR), ("g.txt", "1", REGULAR)],
        &[main, side],
        "merge",
    );
    set_head(&repo, merge);

    let history = file_history(&repo, "f.txt", 100).unwrap();

    assert_eq!(summaries(&history), vec!["side edit", "c1"]);
}

#[test]
fn shows_a_merge_whose_resolution_differs_from_every_parent() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("f.txt", "1", REGULAR)], &[], "c1");
    let side = commit_tree(&repo, &[("f.txt", "2", REGULAR)], &[c1], "side edit");
    let main = commit_tree(&repo, &[("f.txt", "3", REGULAR)], &[c1], "main edit");
    let merge = commit_tree(
        &repo,
        &[("f.txt", "4", REGULAR)],
        &[main, side],
        "resolved merge",
    );
    set_head(&repo, merge);

    let history = file_history(&repo, "f.txt", 100).unwrap();

    assert_eq!(history[0].commit.summary, "resolved merge");
    let mut rest: Vec<&str> = summaries(&history)[1..].to_vec();
    rest.sort();
    assert_eq!(rest, vec!["c1", "main edit", "side edit"]);
}

#[test]
fn stops_at_a_re_add_after_a_delete() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(
        &repo,
        &[("f.txt", "first life", REGULAR), ("k.txt", "k", REGULAR)],
        &[],
        "c1",
    );
    let c2 = commit_tree(&repo, &[("k.txt", "k", REGULAR)], &[c1], "delete");
    let c3 = commit_tree(
        &repo,
        &[
            ("f.txt", "second life\nunrelated\n", REGULAR),
            ("k.txt", "k", REGULAR),
        ],
        &[c2],
        "re-add",
    );
    set_head(&repo, c3);

    let history = file_history(&repo, "f.txt", 100).unwrap();

    assert_eq!(summaries(&history), vec!["re-add"]);
}

#[test]
fn lists_a_mode_only_change() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("run.sh", "echo hi\n", REGULAR)], &[], "add");
    let c2 = commit_tree(
        &repo,
        &[("run.sh", "echo hi\n", EXECUTABLE)],
        &[c1],
        "chmod",
    );
    set_head(&repo, c2);

    let history = file_history(&repo, "run.sh", 100).unwrap();

    assert_eq!(summaries(&history), vec!["chmod", "add"]);
}

#[test]
fn matches_glob_characters_and_spaces_literally() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(
        &repo,
        &[
            ("a[1]*.txt", "1", REGULAR),
            ("a1x.txt", "1", REGULAR),
            ("my file.txt", "1", REGULAR),
        ],
        &[],
        "c1",
    );
    let c2 = commit_tree(
        &repo,
        &[
            ("a[1]*.txt", "1", REGULAR),
            ("a1x.txt", "2", REGULAR),
            ("my file.txt", "2", REGULAR),
        ],
        &[c1],
        "c2",
    );
    set_head(&repo, c2);

    assert_eq!(
        summaries(&file_history(&repo, "a[1]*.txt", 100).unwrap()),
        vec!["c1"]
    );
    assert_eq!(
        summaries(&file_history(&repo, "my file.txt", 100).unwrap()),
        vec!["c2", "c1"]
    );
}

#[test]
fn a_directory_path_has_no_history() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("src/a.txt", "1", REGULAR)], &[], "c1");
    let c2 = commit_tree(&repo, &[("src/a.txt", "2", REGULAR)], &[c1], "c2");
    set_head(&repo, c2);

    assert!(file_history(&repo, "src", 100).unwrap().is_empty());
}

#[test]
fn respects_the_limit() {
    let (_dir, repo) = init_repo();
    let mut tip = commit_tree(&repo, &[("f.txt", "0", REGULAR)], &[], "v0");
    for n in 1..5 {
        tip = commit_tree(
            &repo,
            &[("f.txt", &n.to_string(), REGULAR)],
            &[tip],
            &format!("v{n}"),
        );
    }
    set_head(&repo, tip);

    assert_eq!(
        summaries(&file_history(&repo, "f.txt", 2).unwrap()),
        vec!["v4", "v3"]
    );
}

#[test]
fn unborn_head_and_missing_path_give_empty_history() {
    let (_dir, repo) = init_repo();
    assert!(file_history(&repo, "f.txt", 100).unwrap().is_empty());

    let c1 = commit_tree(&repo, &[("f.txt", "1", REGULAR)], &[], "c1");
    set_head(&repo, c1);
    assert!(file_history(&repo, "missing.txt", 100).unwrap().is_empty());
}

#[test]
fn rejects_paths_outside_the_repository() {
    let (_dir, repo) = init_repo();
    for bad in ["", "../x.txt", "/abs.txt", "a/../../x.txt"] {
        assert!(
            matches!(
                file_history(&repo, bad, 100),
                Err(HistoryError::InvalidPath)
            ),
            "{bad:?} should be rejected"
        );
    }
}

#[test]
fn rename_diff_shows_only_the_edited_lines() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "add");
    let c2 = commit_tree(
        &repo,
        &[("new.txt", FIVE_LINES_EDITED, REGULAR)],
        &[c1],
        "rename and edit",
    );

    let hunks = commit_rename_diff(
        &repo,
        &c2.to_string(),
        "old.txt",
        "new.txt",
        &DiffOptions::default(),
    )
    .unwrap();

    let changed: Vec<(DiffLineOrigin, &str)> = hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.origin != DiffLineOrigin::Context)
        .map(|l| (l.origin, l.content.trim_end()))
        .collect();
    assert_eq!(
        changed,
        vec![
            (DiffLineOrigin::Remove, "l5"),
            (DiffLineOrigin::Add, "CHANGED")
        ]
    );
}

#[test]
fn pure_rename_diff_has_no_hunks() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("new.txt", FIVE_LINES, REGULAR)], &[c1], "rename");

    let hunks = commit_rename_diff(
        &repo,
        &c2.to_string(),
        "old.txt",
        "new.txt",
        &DiffOptions::default(),
    )
    .unwrap();

    assert!(hunks.is_empty());
}

#[test]
fn rename_diff_matches_glob_characters_literally() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(
        &repo,
        &[
            ("o[1]*.txt", FIVE_LINES, REGULAR),
            ("o1x.txt", "noise\n", REGULAR),
        ],
        &[],
        "add",
    );
    let c2 = commit_tree(
        &repo,
        &[("n[1]*.txt", FIVE_LINES_EDITED, REGULAR)],
        &[c1],
        "rename",
    );

    let hunks = commit_rename_diff(
        &repo,
        &c2.to_string(),
        "o[1]*.txt",
        "n[1]*.txt",
        &DiffOptions::default(),
    )
    .unwrap();

    let all: Vec<&str> = hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .map(|l| l.content.trim_end())
        .collect();
    assert!(!all.contains(&"noise"));
    assert!(all.contains(&"CHANGED"));
}
