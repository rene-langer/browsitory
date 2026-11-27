//! `working-tree-encoding` (#1156): git keeps such a file as UTF-8 in the repository and
//! converts it to the named encoding in the work tree; libgit2 ignores the attribute, so it would
//! stage the raw UTF-16 bytes and write UTF-8 into the work tree. Like a file under an external
//! filter driver (#1063), such a path is refused before anything is written, the app's status
//! reads mark it, and `head_info` names the encodings. The git CLI sets the repositories up (it
//! does the conversion) and checks the results.
mod common;

use std::path::Path;

use common::{git_ok, init_repo, write_file};
use git2::{Oid, Repository};
use git_core::status::{StatusEntry, StatusKind};
use tempfile::TempDir;

const SCRIPT: &str = "s.ps1";
const PLAIN: &str = "plain.txt";
const UTF8_TAGGED: &str = "notes.u8";
const LATIN1: &str = "menu.lat";

/// `text` as UTF-16LE with a byte order mark, as Windows tools write a `.ps1`.
fn utf16(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xfe];
    for unit in text.encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    bytes
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// `s.ps1` (UTF-16LE in the work tree, UTF-8 in the repository), `notes.u8` (tagged UTF-8,
/// which git doesn't convert) and `plain.txt`, committed with git.
fn fixture() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(
        d,
        ".gitattributes",
        "*.ps1 text working-tree-encoding=UTF-16LE-BOM eol=lf\n*.u8 working-tree-encoding=utf-8\n\
         *.lat working-tree-encoding=ISO-8859-1\n",
    );
    std::fs::write(d.join(SCRIPT), utf16("Write-Host 'one'\n")).unwrap();
    write_file(d, UTF8_TAGGED, "n1\n");
    // ASCII, the same bytes in ISO-8859-1 and UTF-8: libgit2 never sees it changed by the
    // conversion alone (a racily clean entry is compared by content).
    write_file(d, LATIN1, "cafe\n");
    write_file(d, PLAIN, "p1\n");
    git_ok(d, &["add", "-A"]);
    git_ok(d, &["commit", "-q", "-m", "base"]);
    assert_eq!(
        git_out(d, &["cat-file", "-p", &format!("HEAD:{SCRIPT}")]),
        "Write-Host 'one'\n",
        "git stores the script as UTF-8"
    );
    drop(repo);
    let repo = Repository::open(d).unwrap();
    (dir, repo)
}

/// The script edited in the work tree, in its encoding.
fn edit_script(dir: &Path) {
    std::fs::write(
        dir.join(SCRIPT),
        utf16("Write-Host 'one'\nWrite-Host 'two'\n"),
    )
    .unwrap();
}

#[derive(Debug, PartialEq, Eq)]
struct State {
    head: Option<Oid>,
    index: Vec<(String, Oid)>,
    script: Vec<u8>,
}

fn state(dir: &Path, repo: &Repository) -> State {
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    State {
        head: repo.head().unwrap().target(),
        index: index
            .iter()
            .map(|entry| (String::from_utf8_lossy(&entry.path).into_owned(), entry.id))
            .collect(),
        script: std::fs::read(dir.join(SCRIPT)).unwrap(),
    }
}

/// The refusal names the script and its encoding.
fn assert_refused<T: std::fmt::Debug, E: std::fmt::Display + std::fmt::Debug>(
    result: Result<T, E>,
) {
    assert_refused_as(result, SCRIPT, "UTF-16LE-BOM");
}

fn assert_refused_as<T: std::fmt::Debug, E: std::fmt::Display + std::fmt::Debug>(
    result: Result<T, E>,
    path: &str,
    encoding: &str,
) {
    let error = result.expect_err("the operation should be refused");
    let message = error.to_string();
    assert!(
        message.contains(&format!("'{path}'")) && message.contains(encoding),
        "{message} ({error:?})"
    );
}

// --- To the repository: the work tree's bytes would be stored raw ---

#[test]
fn stage_file_refuses_a_file_with_a_working_tree_encoding() {
    let (dir, repo) = fixture();
    edit_script(dir.path());
    let before = state(dir.path(), &repo);

    assert_refused(git_core::stage::stage_file(&repo, SCRIPT));

    assert_eq!(state(dir.path(), &repo), before);
    assert_eq!(git_out(dir.path(), &["diff", "--cached", "--stat"]), "");
}

#[test]
fn stage_paths_stages_nothing_when_one_path_has_a_working_tree_encoding() {
    let (dir, repo) = fixture();
    edit_script(dir.path());
    write_file(dir.path(), PLAIN, "p2\n");
    let before = state(dir.path(), &repo);

    assert_refused(git_core::stage::stage_paths(
        &repo,
        &[PLAIN.to_string(), SCRIPT.to_string()],
    ));

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn stage_hunk_refuses_a_file_with_a_working_tree_encoding() {
    let (dir, repo) = fixture();
    // Text to libgit2 too (no NUL bytes), so it has hunks.
    std::fs::write(dir.path().join(LATIN1), b"cafe\nth\xe9\n").unwrap();
    let options = git_core::diff::DiffOptions {
        ignore_whitespace: false,
        context_lines: 3,
        algorithm: git_core::diff::DiffAlgorithm::Default,
        max_file_bytes: None,
    };
    let hunks = git_core::diff::working_diff(&repo, LATIN1, false, &options).unwrap();
    let before = state(dir.path(), &repo);

    assert_refused_as(
        git_core::stage::stage_hunk(
            &repo,
            LATIN1,
            hunks[0].old_start,
            hunks[0].new_start,
            common::hunk_ctx(3),
        ),
        LATIN1,
        "ISO-8859-1",
    );

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn a_utf8_working_tree_encoding_and_a_plain_file_still_stage() {
    let (dir, repo) = fixture();
    write_file(dir.path(), UTF8_TAGGED, "n2\n");
    write_file(dir.path(), PLAIN, "p2\n");

    git_core::stage::stage_paths(&repo, &[UTF8_TAGGED.to_string(), PLAIN.to_string()]).unwrap();

    assert_eq!(
        git_out(dir.path(), &["diff", "--cached", "--name-only"]),
        "notes.u8\nplain.txt\n"
    );
}

// --- To the work tree: the repository's UTF-8 would be written as it is ---

#[test]
fn discard_refuses_to_write_a_file_with_a_working_tree_encoding() {
    let (dir, repo) = fixture();
    edit_script(dir.path());
    let before = state(dir.path(), &repo);

    assert_refused(git_core::stage::discard_file(&repo, SCRIPT));

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn switch_refuses_to_write_a_file_with_a_working_tree_encoding() {
    let (dir, repo) = fixture();
    let d = dir.path();
    // The ASCII file, not the UTF-16 script, whose raw bytes libgit2 takes for a local change.
    git_ok(d, &["checkout", "-q", "-b", "other"]);
    std::fs::write(d.join(LATIN1), "other\n").unwrap();
    git_ok(d, &["commit", "-q", "-am", "other"]);
    git_ok(d, &["checkout", "-q", "-"]);
    let before = state(d, &repo);

    assert_refused_as(
        git_core::branch::switch_branch(&repo, "other"),
        LATIN1,
        "ISO-8859-1",
    );

    assert_eq!(state(d, &repo), before);
    assert_eq!(std::fs::read(d.join(LATIN1)).unwrap(), b"cafe\n");
}

#[test]
fn restore_version_refuses_to_write_a_file_with_a_working_tree_encoding() {
    let (dir, repo) = fixture();
    let base = repo.head().unwrap().target().unwrap();
    edit_script(dir.path());
    git_ok(dir.path(), &["commit", "-q", "-am", "two"]);
    let before = state(dir.path(), &repo);

    assert_refused(git_core::stage::restore_file_version(
        &repo,
        &base.to_string(),
        SCRIPT,
    ));

    assert_eq!(state(dir.path(), &repo), before);
}

// --- What the app lists ---

fn app_status(repo: &Repository) -> Vec<StatusEntry> {
    let refreshing = git_core::status::status_refreshing_index(repo).unwrap();
    let with_stamp = git_core::change_stamp::status_with_stamp(repo)
        .unwrap()
        .status;
    assert_eq!(refreshing, with_stamp);
    refreshing
}

#[test]
fn a_changed_file_with_a_working_tree_encoding_is_marked_with_it() {
    let (dir, repo) = fixture();
    edit_script(dir.path());
    write_file(dir.path(), PLAIN, "p2\n");
    write_file(dir.path(), UTF8_TAGGED, "n2\n");
    std::fs::write(dir.path().join("new.ps1"), utf16("new\n")).unwrap();

    let entries = app_status(&repo);

    let marked: Vec<(&str, StatusKind, Option<&str>)> = entries
        .iter()
        .map(|entry| {
            (
                entry.path.as_str(),
                entry.kind,
                entry.working_tree_encoding.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        marked,
        [
            ("new.ps1", StatusKind::New, Some("UTF-16LE-BOM")),
            (UTF8_TAGGED, StatusKind::Modified, None),
            (PLAIN, StatusKind::Modified, None),
            (SCRIPT, StatusKind::Modified, Some("UTF-16LE-BOM")),
        ]
    );
}

#[test]
fn a_deleted_file_with_a_working_tree_encoding_is_not_marked() {
    let (dir, repo) = fixture();
    std::fs::remove_file(dir.path().join(SCRIPT)).unwrap();

    let entries = app_status(&repo);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, StatusKind::Deleted);
    assert_eq!(entries[0].working_tree_encoding, None);
}

#[test]
fn head_info_names_the_working_tree_encodings_the_attributes_use() {
    let (dir, repo) = fixture();
    std::fs::create_dir_all(repo.path().join("info")).unwrap();
    std::fs::write(
        repo.path().join("info/attributes"),
        "[attr]sql working-tree-encoding=UTF-16\n*.sql sql\n",
    )
    .unwrap();
    write_file(
        dir.path(),
        "db/.gitattributes",
        "*.rc working-tree-encoding=UTF-16LE-BOM\n",
    );

    let info = git_core::repo::head_info(&repo).unwrap();

    assert_eq!(
        info.working_tree_encodings,
        ["ISO-8859-1", "UTF-16", "UTF-16LE-BOM"]
    );
}

#[test]
fn head_info_names_no_working_tree_encoding_when_only_utf8_is_named() {
    let (dir, repo) = init_repo();
    write_file(
        dir.path(),
        ".gitattributes",
        "*.txt working-tree-encoding=UTF8\n",
    );

    let info = git_core::repo::head_info(&repo).unwrap();

    assert!(info.working_tree_encodings.is_empty());
}
