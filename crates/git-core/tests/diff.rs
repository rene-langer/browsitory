use git_core::diff::DiffOptions;
mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{commit_all, commit_tree, init_repo, write_file};
use git_core::commit_stats::{commit_stats, commit_stats_batch, CommitStats, StatsBudget};
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
        algorithm: git_core::diff::DiffAlgorithm::Default,
    };
    let five = DiffOptions {
        ignore_whitespace: false,
        context_lines: 5,
        algorithm: git_core::diff::DiffAlgorithm::Default,
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
        algorithm: git_core::diff::DiffAlgorithm::Default,
    };
    let hunk = &git_core::diff::working_diff(&repo, "f.txt", false, &huge).unwrap()[0];
    // 20 context lines on each side + the removed line.
    assert_eq!(hunk.old_lines, 41);
}

fn sixty_lines_with(changed: &[u32]) -> String {
    (1..=60)
        .map(|n| {
            if changed.contains(&n) {
                format!("changed {n}\n")
            } else {
                format!("l{n}\n")
            }
        })
        .collect()
}

#[test]
fn whole_file_context_shows_every_line_in_one_hunk() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", &sixty_lines_with(&[]));
    common::commit_all(&repo, "base");
    // Two changes 40 lines apart: twenty lines of context would still split them.
    common::write_file(dir.path(), "f.txt", &sixty_lines_with(&[5, 45]));

    let whole = DiffOptions {
        ignore_whitespace: false,
        context_lines: git_core::diff::WHOLE_FILE_CONTEXT_LINES,
        algorithm: git_core::diff::DiffAlgorithm::Default,
    };
    let hunks = git_core::diff::working_diff(&repo, "f.txt", false, &whole).unwrap();
    assert_eq!(hunks.len(), 1);
    let hunk = &hunks[0];
    assert_eq!((hunk.old_start, hunk.old_lines), (1, 60));
    assert_eq!((hunk.new_start, hunk.new_lines), (1, 60));
    assert_eq!(hunk.lines.len(), 62);
    assert_eq!(hunk.lines[0].content, "l1\n");
    assert_eq!(hunk.lines.last().unwrap().content, "l60\n");
}

#[test]
fn whole_file_context_applies_to_commit_diffs() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", &sixty_lines_with(&[]));
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", &sixty_lines_with(&[30]));
    common::commit_all(&repo, "change");
    let head = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    let whole = DiffOptions {
        ignore_whitespace: false,
        context_lines: git_core::diff::WHOLE_FILE_CONTEXT_LINES,
        algorithm: git_core::diff::DiffAlgorithm::Default,
    };
    let hunks = git_core::diff::commit_diff(&repo, &head, "f.txt", &whole).unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].old_lines, 60);
    assert_eq!(hunks[0].lines.len(), 61);
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
        algorithm: git_core::diff::DiffAlgorithm::Default,
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
        algorithm: git_core::diff::DiffAlgorithm::Default,
    };
    assert!(
        git_core::diff::commit_diff(&repo, &head, "f.txt", &ignoring)
            .unwrap()
            .is_empty()
    );
}

// ---------------------------------------------------------------------------------------------
// file_sides (image preview)
// ---------------------------------------------------------------------------------------------

use git_core::diff::{file_sides, FileSide, MAX_PREVIEW_BYTES};
use git_core::tools::DiffSource;

fn loaded(bytes: &[u8]) -> FileSide {
    FileSide::Loaded {
        bytes: bytes.to_vec(),
    }
}

fn head_id(repo: &git2::Repository) -> String {
    repo.head().unwrap().target().unwrap().to_string()
}

fn sides(repo: &git2::Repository, source: DiffSource, path: &str) -> (FileSide, FileSide) {
    let sides = file_sides(repo, &source, path, MAX_PREVIEW_BYTES).unwrap();
    (sides.old, sides.new)
}

#[test]
fn file_sides_for_unstaged_staged_and_commit_sources() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "img.png", "v1");
    commit_all(&repo, "v1");
    let v1 = head_id(&repo);
    write_file(dir.path(), "img.png", "v2");
    commit_all(&repo, "v2");
    let v2 = head_id(&repo);
    write_file(dir.path(), "img.png", "v3");
    git_core::stage::stage_file(&repo, "img.png").unwrap();
    write_file(dir.path(), "img.png", "v4");

    assert_eq!(
        sides(&repo, DiffSource::Unstaged, "img.png"),
        (loaded(b"v3"), loaded(b"v4"))
    );
    assert_eq!(
        sides(&repo, DiffSource::Staged { old_path: None }, "img.png"),
        (loaded(b"v2"), loaded(b"v3"))
    );
    let commit = |id: &str| DiffSource::Commit {
        commit_id: id.to_string(),
        old_path: None,
    };
    assert_eq!(
        sides(&repo, commit(&v2), "img.png"),
        (loaded(b"v1"), loaded(b"v2"))
    );
    assert_eq!(
        sides(&repo, commit(&v1), "img.png"),
        (FileSide::Missing, loaded(b"v1"))
    );
}

#[test]
fn file_sides_for_range_sources_including_the_working_tree() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "img.png", "v1");
    commit_all(&repo, "v1");
    let v1 = head_id(&repo);
    write_file(dir.path(), "img.png", "v2");
    commit_all(&repo, "v2");
    let v2 = head_id(&repo);
    write_file(dir.path(), "img.png", "work");

    let range = |to: Option<&str>| DiffSource::Range {
        from_id: v1.clone(),
        to_id: to.map(str::to_string),
        old_path: None,
    };
    assert_eq!(
        sides(&repo, range(Some(&v2)), "img.png"),
        (loaded(b"v1"), loaded(b"v2"))
    );
    assert_eq!(
        sides(&repo, range(None), "img.png"),
        (loaded(b"v1"), loaded(b"work"))
    );
}

#[test]
fn file_sides_reports_added_deleted_and_renamed_files() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "old.png", "pixels");
    commit_all(&repo, "add");
    write_file(dir.path(), "new.png", "fresh");
    assert_eq!(
        sides(&repo, DiffSource::Unstaged, "new.png"),
        (FileSide::Missing, loaded(b"fresh"))
    );
    std::fs::remove_file(dir.path().join("old.png")).unwrap();
    assert_eq!(
        sides(&repo, DiffSource::Unstaged, "old.png"),
        (loaded(b"pixels"), FileSide::Missing)
    );

    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.png", "same");
    commit_all(&repo, "a");
    let before = head_id(&repo);
    std::fs::rename(dir.path().join("a.png"), dir.path().join("b.png")).unwrap();
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("a.png")).unwrap();
    index.add_path(Path::new("b.png")).unwrap();
    index.write().unwrap();
    commit_all(&repo, "rename");
    let id = head_id(&repo);
    assert_eq!(
        sides(
            &repo,
            DiffSource::Commit {
                commit_id: id.clone(),
                old_path: Some("a.png".into())
            },
            "b.png"
        ),
        (loaded(b"same"), loaded(b"same"))
    );
    assert_eq!(
        sides(
            &repo,
            DiffSource::Range {
                from_id: before,
                to_id: Some(id),
                old_path: Some("a.png".into())
            },
            "b.png"
        ),
        (loaded(b"same"), loaded(b"same"))
    );
}

#[test]
fn a_side_over_the_cap_is_too_large_with_its_size() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "big.png", "0123456789");
    commit_all(&repo, "big");
    write_file(dir.path(), "big.png", "0123456789abcdef");
    let unstaged = file_sides(&repo, &DiffSource::Unstaged, "big.png", 12).unwrap();
    assert_eq!(unstaged.old, loaded(b"0123456789"));
    assert_eq!(unstaged.new, FileSide::TooLarge { size: 16 });

    // A blob over the cap is judged by its header, before it is read.
    git_core::stage::stage_file(&repo, "big.png").unwrap();
    let staged = file_sides(&repo, &DiffSource::Staged { old_path: None }, "big.png", 12).unwrap();
    assert_eq!(staged.new, FileSide::TooLarge { size: 16 });
    // Exactly at the cap still loads.
    let at_cap = file_sides(&repo, &DiffSource::Staged { old_path: None }, "big.png", 10).unwrap();
    assert_eq!(at_cap.old, loaded(b"0123456789"));
}

#[test]
fn file_sides_rejects_paths_outside_the_repository() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.png", "x");
    commit_all(&repo, "a");
    let id = head_id(&repo);
    for bad in [
        "../secret.png",
        "/etc/passwd",
        "a/../../b.png",
        "",
        "./a.png",
    ] {
        assert!(
            matches!(
                file_sides(&repo, &DiffSource::Unstaged, bad, MAX_PREVIEW_BYTES),
                Err(git_core::diff::DiffError::InvalidPath(_))
            ),
            "{bad:?}"
        );
    }
    let renamed_from_outside = DiffSource::Commit {
        commit_id: id,
        old_path: Some("../a.png".into()),
    };
    assert!(matches!(
        file_sides(&repo, &renamed_from_outside, "a.png", MAX_PREVIEW_BYTES),
        Err(git_core::diff::DiffError::InvalidPath(_))
    ));
}

#[test]
fn a_directory_is_missing_not_an_error() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "sub/a.png", "x");
    commit_all(&repo, "a");
    assert_eq!(
        sides(&repo, DiffSource::Unstaged, "sub"),
        (FileSide::Missing, FileSide::Missing)
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_working_file_is_missing() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "keep.txt", "k");
    commit_all(&repo, "base");
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("secret.png"), "secret").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.png"),
        dir.path().join("link.png"),
    )
    .unwrap();
    assert_eq!(
        sides(&repo, DiffSource::Unstaged, "link.png").1,
        FileSide::Missing
    );
}

#[cfg(unix)]
#[test]
fn a_working_file_behind_a_symlinked_folder_is_missing() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "keep.txt", "k");
    commit_all(&repo, "base");
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("secret.png"), "secret").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked")).unwrap();
    assert_eq!(
        sides(&repo, DiffSource::Unstaged, "linked/secret.png").1,
        FileSide::Missing
    );
}

#[test]
fn sniffs_image_types_from_magic_bytes_not_names() {
    use git_core::diff::sniff_image_mime;
    assert_eq!(
        sniff_image_mime(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"),
        Some("image/png")
    );
    assert_eq!(
        sniff_image_mime(b"\xff\xd8\xff\xe0\0\x10JFIF"),
        Some("image/jpeg")
    );
    assert_eq!(sniff_image_mime(b"GIF89a\x01\0"), Some("image/gif"));
    assert_eq!(sniff_image_mime(b"GIF87a\x01\0"), Some("image/gif"));
    assert_eq!(
        sniff_image_mime(b"RIFF\x24\0\0\0WEBPVP8 "),
        Some("image/webp")
    );
    assert_eq!(
        sniff_image_mime(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
        Some("image/svg+xml")
    );
    assert_eq!(
        sniff_image_mime(
            b"\xef\xbb\xbf  <?xml version=\"1.0\"?>\n<!-- made by hand -->\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x\">\n<svg>"
        ),
        Some("image/svg+xml")
    );
    for not_an_image in [
        &b""[..],
        b"\x89PN",
        b"RIFF\x24\0\0\0WAVE",
        b"<svgx>",
        b"<html><svg></svg></html>",
        b"<!-- unterminated <svg>",
        b"hello",
        b"\xff\xd8",
    ] {
        assert_eq!(sniff_image_mime(not_an_image), None, "{not_an_image:?}");
    }
}

#[test]
fn the_index_side_sees_a_git_add_made_through_another_handle() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "img.png", "v1");
    commit_all(&repo, "v1");
    assert_eq!(
        sides(&repo, DiffSource::Staged { old_path: None }, "img.png").1,
        loaded(b"v1")
    );
    write_file(dir.path(), "img.png", "v2");
    let other = git2::Repository::open(dir.path()).unwrap();
    let mut index = other.index().unwrap();
    index.add_path(Path::new("img.png")).unwrap();
    index.write().unwrap();
    assert_eq!(
        sides(&repo, DiffSource::Staged { old_path: None }, "img.png").1,
        loaded(b"v2")
    );
}

#[test]
fn commit_diff_of_a_file_that_became_a_folder_shows_only_that_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "d", "file d\n");
    commit_all(&repo, "a");
    std::fs::remove_file(dir.path().join("d")).unwrap();
    write_file(dir.path(), "d/inner.txt", "inner\n");
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("d")).unwrap();
    index.write().unwrap();
    commit_all(&repo, "b");
    let b = repo.head().unwrap().target().unwrap().to_string();

    let hunks = git_core::diff::commit_diff(&repo, &b, "d", &DiffOptions::default()).unwrap();

    let lines: Vec<_> = hunks.iter().flat_map(|h| h.lines.iter()).collect();
    assert_eq!(lines.len(), 1, "{hunks:?}");
    assert_eq!(lines[0].origin, DiffLineOrigin::Remove);
    assert_eq!(lines[0].content, "file d\n");
}

#[test]
fn working_diff_unstaged_of_a_file_replaced_by_a_folder_shows_only_that_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "d", "file d\n");
    commit_all(&repo, "a");
    std::fs::remove_file(dir.path().join("d")).unwrap();
    write_file(dir.path(), "d/inner.txt", "inner\n");

    let hunks = git_core::diff::working_diff(&repo, "d", false, &DiffOptions::default()).unwrap();

    let lines: Vec<_> = hunks.iter().flat_map(|h| h.lines.iter()).collect();
    assert_eq!(lines.len(), 1, "{hunks:?}");
    assert_eq!(lines[0].origin, DiffLineOrigin::Remove);
}

fn staged_rename(dir: &Path, repo: &git2::Repository, old: &str, new: &str) {
    std::fs::rename(dir.join(old), dir.join(new)).unwrap();
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new(old)).unwrap();
    index.add_path(Path::new(new)).unwrap();
    index.write().unwrap();
}

#[test]
fn file_sides_of_a_staged_rename_compares_old_and_new() {
    use git_core::diff::{file_sides, FileSide};
    use git_core::tools::DiffSource;
    let (dir, repo) = init_repo();
    std::fs::write(dir.path().join("old.png"), b"\x89PNG image").unwrap();
    commit_all(&repo, "base");
    staged_rename(dir.path(), &repo, "old.png", "new.png");

    let with_old = file_sides(
        &repo,
        &DiffSource::Staged {
            old_path: Some("old.png".into()),
        },
        "new.png",
        1 << 20,
    )
    .unwrap();
    let loaded = FileSide::Loaded {
        bytes: b"\x89PNG image".to_vec(),
    };
    assert_eq!(with_old.old, loaded);
    assert_eq!(with_old.new, loaded);

    let without = file_sides(
        &repo,
        &DiffSource::Staged { old_path: None },
        "new.png",
        1 << 20,
    )
    .unwrap();
    assert_eq!(without.old, FileSide::Missing);
}

#[test]
fn staged_rename_diff_shows_only_the_edit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "old.txt", "one\ntwo\nthree\nfour\nfive\n");
    commit_all(&repo, "base");
    staged_rename(dir.path(), &repo, "old.txt", "new.txt");
    write_file(dir.path(), "new.txt", "one\ntwo\nTHREE\nfour\nfive\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();

    let hunks =
        git_core::diff::staged_rename_diff(&repo, "old.txt", "new.txt", &DiffOptions::default())
            .unwrap();

    let edits: Vec<_> = hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.origin != DiffLineOrigin::Context)
        .map(|l| (l.origin, l.content.clone()))
        .collect();
    assert_eq!(
        edits,
        vec![
            (DiffLineOrigin::Remove, "three\n".to_string()),
            (DiffLineOrigin::Add, "THREE\n".to_string())
        ]
    );
}

const BLOB: u32 = 0o100644;

fn stats_of(repo: &git2::Repository, commit: git2::Oid) -> CommitStats {
    commit_stats(repo, &commit.to_string(), true, 100, None)
        .unwrap()
        .expect("under the cap")
}

fn totals(stats: &CommitStats) -> (usize, usize, usize) {
    (stats.files_changed, stats.insertions, stats.deletions)
}

/// Each file as `path +insertions -deletions`, sorted.
fn file_counts(stats: &CommitStats) -> Vec<String> {
    let mut files = Vec::new();
    for f in &stats.files {
        let kind = if f.binary { " (binary)" } else { "" };
        let line = format!("{} +{} -{}{kind}", f.path, f.insertions, f.deletions);
        files.push(line);
    }
    files.sort();
    files
}

#[test]
fn commit_stats_count_added_removed_and_modified_lines() {
    let (_dir, repo) = init_repo();
    let base = commit_tree(
        &repo,
        &[
            ("a", "one\ntwo\nthree\n", BLOB),
            ("c", "gone\ngone\ngone\n", BLOB),
        ],
        &[],
        "base",
    );
    let next = commit_tree(
        &repo,
        &[
            ("a", "one\nTWO\nthree\nfour\n", BLOB),
            ("b", "new\nfile\n", BLOB),
        ],
        &[base],
        "edit",
    );

    let stats = stats_of(&repo, next);

    assert_eq!(totals(&stats), (3, 4, 4));
    assert_eq!(file_counts(&stats), ["a +2 -1", "b +2 -0", "c +0 -3"]);
}

#[test]
fn commit_stats_of_a_root_commit_count_every_line_as_added() {
    let (_dir, repo) = init_repo();
    let root = commit_tree(
        &repo,
        &[("a", "1\n2\n", BLOB), ("b", "3\n", BLOB)],
        &[],
        "root",
    );

    assert_eq!(totals(&stats_of(&repo, root)), (2, 3, 0));
}

/// Like the commit's file list (`commit_files`), a rename is a removed and an added file.
#[test]
fn commit_stats_count_a_rename_like_the_file_list() {
    let (_dir, repo) = init_repo();
    let contents = "1\n2\n3\n4\n5\n";
    let base = commit_tree(&repo, &[("old", contents, BLOB)], &[], "base");
    let moved = commit_tree(&repo, &[("new", contents, BLOB)], &[base], "move");

    let stats = stats_of(&repo, moved);

    assert_eq!(totals(&stats), (2, 5, 5));
    assert_eq!(file_counts(&stats), ["new +5 -0", "old +0 -5"]);
    let id = moved.to_string();
    let mut listed = git_core::diff::commit_files(&repo, &id).unwrap();
    listed.sort();
    assert_eq!(listed, ["new", "old"]);
}

#[test]
fn commit_stats_count_a_binary_file_but_no_lines() {
    let (_dir, repo) = init_repo();
    let base = commit_tree(&repo, &[("bin", "a\0b\n", BLOB)], &[], "base");
    let next = commit_tree(
        &repo,
        &[("bin", "a\0c\n", BLOB), ("t", "text\n", BLOB)],
        &[base],
        "binary edit",
    );

    let stats = stats_of(&repo, next);

    assert_eq!(totals(&stats), (2, 1, 0));
    assert_eq!(file_counts(&stats), ["bin +0 -0 (binary)", "t +1 -0"]);
}

#[test]
fn commit_stats_of_a_merge_compare_it_with_its_first_parent() {
    let (_dir, repo) = init_repo();
    let base = commit_tree(&repo, &[("base", "x\n", BLOB)], &[], "base");
    let ours = commit_tree(
        &repo,
        &[("base", "x\n", BLOB), ("a", "a\n", BLOB)],
        &[base],
        "ours",
    );
    let theirs = commit_tree(
        &repo,
        &[("base", "x\n", BLOB), ("b", "b\nb\nb\n", BLOB)],
        &[base],
        "theirs",
    );
    let merge = commit_tree(
        &repo,
        &[
            ("base", "x\n", BLOB),
            ("a", "a\n", BLOB),
            ("b", "b\nb\nb\n", BLOB),
        ],
        &[ours, theirs],
        "merge",
    );

    let stats = stats_of(&repo, merge);

    assert_eq!(totals(&stats), (1, 3, 0));
    assert_eq!(file_counts(&stats), ["b +3 -0"]);
}

#[test]
fn commit_stats_without_files_keep_only_the_totals() {
    let (_dir, repo) = init_repo();
    let root = commit_tree(&repo, &[("a", "one\n", BLOB)], &[], "root");

    let stats = commit_stats(&repo, &root.to_string(), false, 100, None)
        .unwrap()
        .unwrap();

    assert_eq!(totals(&stats), (1, 1, 0));
    assert!(stats.files.is_empty());
}

#[test]
fn commit_stats_over_the_cap_are_not_counted() {
    let (_dir, repo) = init_repo();
    let root = commit_tree(
        &repo,
        &[("a.txt", "one\n", BLOB), ("b.txt", "two\n", BLOB)],
        &[],
        "root",
    );
    let id = root.to_string();
    let past = Some(Instant::now());

    // More files than `max_files`, exactly `max_files`, and past the deadline.
    assert!(commit_stats(&repo, &id, false, 1, None).unwrap().is_none());
    assert!(commit_stats(&repo, &id, false, 2, None).unwrap().is_some());
    assert!(commit_stats(&repo, &id, false, 2, past).unwrap().is_none());
}

#[test]
fn commit_stats_batch_answers_in_order_and_stops_at_the_request_budget() {
    let (_dir, repo) = init_repo();
    let root = commit_tree(&repo, &[("a", "1\n", BLOB)], &[], "root");
    let next = commit_tree(&repo, &[("a", "1\n2\n", BLOB)], &[root], "next");
    // A commit that isn't there (gone after a force push) is answered with no stats.
    let ids = [next.to_string(), "0".repeat(40), root.to_string()];
    let roomy = StatsBudget {
        request: Duration::from_secs(60),
        ..StatsBudget::default()
    };
    let hurried = StatsBudget {
        request: Duration::ZERO,
        ..roomy
    };

    let all = commit_stats_batch(&repo, &ids, false, roomy);
    let first_only = commit_stats_batch(&repo, &ids, false, hurried);

    let mut answered = Vec::new();
    for entry in &all {
        let inserted = entry.stats.as_ref().map(|s| s.insertions);
        answered.push((entry.commit_id.as_str(), inserted));
    }
    assert_eq!(
        answered,
        [
            (ids[0].as_str(), Some(1)),
            (ids[1].as_str(), None),
            (ids[2].as_str(), Some(1))
        ]
    );
    assert_eq!(first_only.len(), 1);
    assert_eq!(first_only[0].commit_id, ids[0]);
}
