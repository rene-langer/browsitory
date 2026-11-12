mod common;

use common::{commit_all, commit_tree, init_repo, write_file};

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

// #839: blame on top of the file's history walk. Each scenario lists the expected owner of
// every line and, when the git CLI is available, compares with `git blame --porcelain`.

const REGULAR: u32 = 0o100644;

fn set_head(repo: &git2::Repository, tip: git2::Oid) {
    repo.reference("refs/heads/main", tip, true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
}

/// The owner of each line, as (commit id, content).
fn owners(lines: &[git_core::blame::BlameLine]) -> Vec<(String, String)> {
    lines
        .iter()
        .map(|l| (l.commit_id.clone(), l.content.clone()))
        .collect()
}

fn expect(rows: &[(git2::Oid, &str)]) -> Vec<(String, String)> {
    rows.iter()
        .map(|(id, content)| (id.to_string(), (*content).to_string()))
        .collect()
}

/// When the git CLI is available, asserts that `git blame --porcelain` gives every line the
/// same commit as `lines`.
fn assert_matches_git(
    repo: &git2::Repository,
    revision: &str,
    path: &str,
    lines: &[git_core::blame::BlameLine],
) {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping the git blame comparison");
        return;
    }
    let output = test_support::git_command(repo.workdir().unwrap())
        .args(["blame", "--porcelain", revision, "--", path])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Each line's header is "<commit> <original line> <final line>[ <group size>]".
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut git = Vec::new();
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split(' ').collect();
        let is_header = fields.len() >= 3
            && fields[0].len() == 40
            && fields[0].bytes().all(|b| b.is_ascii_hexdigit())
            && fields[1].parse::<usize>().is_ok()
            && fields[2].parse::<usize>().is_ok();
        if is_header {
            git.push(fields[0].to_string());
        }
    }
    let ours: Vec<String> = lines.iter().map(|l| l.commit_id.clone()).collect();
    assert_eq!(ours, git, "blame differs from git blame");
}

#[test]
fn each_line_belongs_to_the_commit_that_last_changed_it() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("f.txt", "a\nb\nc\nd\ne\n", REGULAR)], &[], "c1");
    let c2 = commit_tree(
        &repo,
        &[("f.txt", "a\nb\nX\nc\nd\ne\n", REGULAR)],
        &[c1],
        "insert",
    );
    let c3 = commit_tree(
        &repo,
        &[("f.txt", "a\nb\nX\nc\nD\ne\n", REGULAR)],
        &[c2],
        "edit",
    );
    let c4 = commit_tree(
        &repo,
        &[("f.txt", "b\nX\nc\nD\ne\n", REGULAR)],
        &[c3],
        "delete",
    );
    set_head(&repo, c4);

    let lines = git_core::blame::blame_file(&repo, "HEAD", "f.txt").unwrap();

    assert_eq!(
        owners(&lines),
        expect(&[(c1, "b"), (c2, "X"), (c1, "c"), (c3, "D"), (c1, "e")])
    );
    assert_eq!(
        lines.iter().map(|l| l.line_number).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert_matches_git(&repo, "HEAD", "f.txt", &lines);
}

#[test]
fn follows_a_rename_with_an_edit() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(
        &repo,
        &[("old.txt", "a\nb\nc\nd\ne\n", REGULAR)],
        &[],
        "add",
    );
    let c2 = commit_tree(
        &repo,
        &[("new.txt", "a\nB\nc\nd\ne\n", REGULAR)],
        &[c1],
        "rename and edit",
    );
    let c3 = commit_tree(
        &repo,
        &[("new.txt", "a\nB\nC\nd\ne\n", REGULAR)],
        &[c2],
        "edit",
    );
    set_head(&repo, c3);

    let lines = git_core::blame::blame_file(&repo, "HEAD", "new.txt").unwrap();

    assert_eq!(
        owners(&lines),
        expect(&[(c1, "a"), (c2, "B"), (c3, "C"), (c1, "d"), (c1, "e")])
    );
    assert_matches_git(&repo, "HEAD", "new.txt", &lines);
}

#[test]
fn follows_a_file_whose_folder_was_renamed() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("old/f.txt", "a\nb\nc\n", REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("new/f.txt", "a\nb\nc\n", REGULAR)], &[c1], "move");
    let c3 = commit_tree(&repo, &[("new/f.txt", "a\nB\nc\n", REGULAR)], &[c2], "edit");
    set_head(&repo, c3);

    let lines = git_core::blame::blame_file(&repo, "HEAD", "new/f.txt").unwrap();

    assert_eq!(owners(&lines), expect(&[(c1, "a"), (c3, "B"), (c1, "c")]));
    assert_matches_git(&repo, "HEAD", "new/f.txt", &lines);
}

#[test]
fn a_merge_owns_only_its_resolution_and_each_side_keeps_its_lines() {
    let (_dir, repo) = init_repo();
    let base = commit_tree(&repo, &[("f.txt", "a\nb\nc\nd\ne\n", REGULAR)], &[], "base");
    let main = commit_tree(
        &repo,
        &[("f.txt", "A\nb\nc\nd\ne\n", REGULAR)],
        &[base],
        "main",
    );
    let side = commit_tree(
        &repo,
        &[("f.txt", "a\nb\nc\nd\nE\n", REGULAR)],
        &[base],
        "side",
    );
    let merge = commit_tree(
        &repo,
        &[("f.txt", "A\nb\nc\nM\nE\n", REGULAR)],
        &[main, side],
        "merge",
    );
    set_head(&repo, merge);

    let lines = git_core::blame::blame_file(&repo, "HEAD", "f.txt").unwrap();

    assert_eq!(
        owners(&lines),
        expect(&[
            (main, "A"),
            (base, "b"),
            (base, "c"),
            (merge, "M"),
            (side, "E")
        ])
    );
    assert_matches_git(&repo, "HEAD", "f.txt", &lines);
}

#[test]
fn a_merge_that_took_one_side_passes_every_line_to_that_side() {
    let (_dir, repo) = init_repo();
    let base = commit_tree(
        &repo,
        &[("f.txt", "a\nb\nc\n", REGULAR), ("k.txt", "1", REGULAR)],
        &[],
        "base",
    );
    let main = commit_tree(
        &repo,
        &[("f.txt", "a\nb\nc\n", REGULAR), ("k.txt", "2", REGULAR)],
        &[base],
        "main",
    );
    let side = commit_tree(
        &repo,
        &[("f.txt", "a\nB\nc\n", REGULAR), ("k.txt", "1", REGULAR)],
        &[base],
        "side",
    );
    let merge = commit_tree(
        &repo,
        &[("f.txt", "a\nB\nc\n", REGULAR), ("k.txt", "2", REGULAR)],
        &[main, side],
        "merge",
    );
    set_head(&repo, merge);

    let lines = git_core::blame::blame_file(&repo, "HEAD", "f.txt").unwrap();

    assert_eq!(
        owners(&lines),
        expect(&[(base, "a"), (side, "B"), (base, "c")])
    );
    assert_matches_git(&repo, "HEAD", "f.txt", &lines);
}

#[test]
fn a_file_re_added_after_a_delete_starts_at_the_re_add() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(
        &repo,
        &[("f.txt", "a\nb\n", REGULAR), ("k.txt", "k", REGULAR)],
        &[],
        "add",
    );
    let c2 = commit_tree(&repo, &[("k.txt", "k", REGULAR)], &[c1], "delete");
    let c3 = commit_tree(
        &repo,
        &[("f.txt", "a\nb\nc\n", REGULAR), ("k.txt", "k", REGULAR)],
        &[c2],
        "re-add",
    );
    set_head(&repo, c3);

    let lines = git_core::blame::blame_file(&repo, "HEAD", "f.txt").unwrap();

    assert_eq!(owners(&lines), expect(&[(c3, "a"), (c3, "b"), (c3, "c")]));
    assert_matches_git(&repo, "HEAD", "f.txt", &lines);
}

#[test]
fn crlf_lines_and_a_last_line_without_a_newline() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(
        &repo,
        &[("f.txt", "one\r\ntwo\r\nthree", REGULAR)],
        &[],
        "add",
    );
    let c2 = commit_tree(
        &repo,
        &[("f.txt", "one\r\nTWO\r\nthree", REGULAR)],
        &[c1],
        "edit",
    );
    set_head(&repo, c2);

    let lines = git_core::blame::blame_file(&repo, "HEAD", "f.txt").unwrap();

    assert_eq!(
        owners(&lines),
        expect(&[(c1, "one"), (c2, "TWO"), (c1, "three")])
    );
    assert_matches_git(&repo, "HEAD", "f.txt", &lines);
}

#[test]
fn an_empty_file_has_no_lines() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("f.txt", "", REGULAR)], &[], "add");
    set_head(&repo, c1);

    assert!(git_core::blame::blame_file(&repo, "HEAD", "f.txt")
        .unwrap()
        .is_empty());
}

/// libgit2's blame diffs the whole trees of every commit that changed the file, so it reads
/// every folder of both. Blame reads only the file's path: here an unrelated folder's objects
/// are gone from the object database and blame still answers.
#[test]
fn blame_reads_only_the_files_path_not_the_whole_tree() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(
        &repo,
        &[("f.txt", "a\nb\n", REGULAR), ("z/y.txt", "z\n", REGULAR)],
        &[],
        "add",
    );
    let c2 = commit_tree(
        &repo,
        &[("f.txt", "a\nB\n", REGULAR), ("z/y.txt", "z\n", REGULAR)],
        &[c1],
        "edit",
    );
    set_head(&repo, c2);
    let folder = repo
        .find_commit(c2)
        .unwrap()
        .tree()
        .unwrap()
        .get_name("z")
        .unwrap()
        .id();
    let file = repo.blob(b"z\n").unwrap();
    for id in [folder, file] {
        let hex = id.to_string();
        let path = repo.path().join("objects").join(&hex[..2]).join(&hex[2..]);
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
        std::fs::remove_file(path).unwrap();
    }
    let repo = git2::Repository::open(repo.path()).unwrap();

    let lines = git_core::blame::blame_file(&repo, "HEAD", "f.txt").unwrap();

    assert_eq!(owners(&lines), expect(&[(c1, "a"), (c2, "B")]));
}

/// A commit reached through a short and a long path is examined only after every child on
/// both has passed it its lines (the walk is topological).
#[test]
fn a_commit_reached_by_paths_of_different_lengths_gets_the_lines_of_both() {
    let (_dir, repo) = init_repo();
    let base = commit_tree(&repo, &[("f.txt", "a\nb\nc\n", REGULAR)], &[], "base");
    let edit = commit_tree(&repo, &[("f.txt", "a\nB\nc\n", REGULAR)], &[base], "edit");
    let other = commit_tree(&repo, &[("f.txt", "a\nB\nC\n", REGULAR)], &[edit], "other");
    // The merge takes `c` back from `base` directly; `a` reaches `base` through `edit`.
    let merge = commit_tree(
        &repo,
        &[("f.txt", "a\nB\nc\n", REGULAR)],
        &[other, base],
        "merge",
    );
    set_head(&repo, merge);

    let lines = git_core::blame::blame_file(&repo, "HEAD", "f.txt").unwrap();

    assert_eq!(
        owners(&lines),
        expect(&[(base, "a"), (edit, "B"), (base, "c")])
    );
    assert_matches_git(&repo, "HEAD", "f.txt", &lines);
}

/// Two renames merged: the parent holds the lines under two paths at once, and each keeps
/// its own path's history.
#[test]
fn lines_reaching_one_commit_under_two_paths_keep_their_own_paths() {
    let (_dir, repo) = init_repo();
    let older = commit_tree(
        &repo,
        &[
            ("f.txt", "a\nb\nc\nd\ne\n", REGULAR),
            ("g.txt", "1\n2\n3\nX\n5\n", REGULAR),
        ],
        &[],
        "older",
    );
    let both = commit_tree(
        &repo,
        &[
            ("f.txt", "A\nb\nc\nd\ne\n", REGULAR),
            ("g.txt", "1\n2\n3\n4\n5\n", REGULAR),
        ],
        &[older],
        "both",
    );
    let rename_f = commit_tree(
        &repo,
        &[
            ("h.txt", "A\nb\nc\nd\ne\n", REGULAR),
            ("g.txt", "1\n2\n3\n4\n5\n", REGULAR),
        ],
        &[both],
        "rename f",
    );
    let rename_g = commit_tree(
        &repo,
        &[
            ("f.txt", "A\nb\nc\nd\ne\n", REGULAR),
            ("h.txt", "1\n2\n3\n4\n5\n", REGULAR),
        ],
        &[both],
        "rename g",
    );
    let merge = commit_tree(
        &repo,
        &[("h.txt", "A\nb\nc\n4\n5\n", REGULAR)],
        &[rename_f, rename_g],
        "merge",
    );
    set_head(&repo, merge);

    let lines = git_core::blame::blame_file(&repo, "HEAD", "h.txt").unwrap();

    assert_eq!(
        owners(&lines),
        expect(&[
            (both, "A"),
            (older, "b"),
            (older, "c"),
            (both, "4"),
            (older, "5")
        ])
    );
    assert_matches_git(&repo, "HEAD", "h.txt", &lines);
}
