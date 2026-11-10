//! A patch file's symbolic-link entries: nothing is written through a link the same patch
//! creates, no link is written at a name that opens the git folder or into it, and git's own
//! control files are never written as links.
mod common;

use common::{commit_all, init_repo, write_file};
use git_core::patch::{apply_patch, PatchError};
use std::path::Path;

/// A patch entry creating the link `path` to `target`.
fn new_link(path: &str, target: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\nnew file mode 120000\nindex 0000000..1111111\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1 @@\n+{target}\n\\ No newline at end of file\n"
    )
}

/// A patch entry creating the file `path` holding `line`.
fn new_file(path: &str, line: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\nnew file mode 100755\nindex 0000000..3333333\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1 @@\n+{line}\n"
    )
}

fn repo_with_a_commit() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "README", "readme\n");
    commit_all(&repo, "base");
    (dir, repo)
}

/// Nothing but the committed file and `.git` is in the work tree.
fn assert_untouched(dir: &Path) {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, [".git", "README"]);
}

fn hooks(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir.join(".git/hooks"))
        .map(|entries| {
            entries
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .filter(|n| !n.ends_with(".sample"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_link_entry_below_a_link_the_patch_creates_is_refused() {
    let (dir, repo) = repo_with_a_commit();
    let patch = new_link("a", ".git/hooks")
        + &new_link("a/pre-commit", "../../evil.sh")
        + &new_file("evil.sh", "#!/bin/sh");
    let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();
    assert!(
        matches!(&err, PatchError::BeyondSymlink(p) if p == "a/pre-commit"),
        "{err:?}"
    );
    assert_untouched(dir.path());
    assert!(hooks(dir.path()).is_empty(), "{:?}", hooks(dir.path()));
}

#[test]
fn a_file_entry_below_a_link_the_patch_creates_is_refused() {
    let (dir, repo) = repo_with_a_commit();
    let outside = tempfile::TempDir::new().unwrap();
    let patch = new_link("a", outside.path().to_str().unwrap()) + &new_file("a/planted", "x");
    let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();
    assert!(
        matches!(&err, PatchError::BeyondSymlink(p) if p == "a/planted"),
        "{err:?}"
    );
    assert_untouched(dir.path());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn a_link_below_an_absolute_link_the_patch_creates_is_refused() {
    let (dir, repo) = repo_with_a_commit();
    let outside = tempfile::TempDir::new().unwrap();
    let patch = new_link("a", outside.path().to_str().unwrap()) + &new_link("a/planted", "x");
    let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();
    assert!(matches!(err, PatchError::BeyondSymlink(_)), "{err:?}");
    assert_untouched(dir.path());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn the_entry_order_does_not_matter() {
    let (dir, repo) = repo_with_a_commit();
    let patch = new_link("a/pre-commit", "../../evil.sh") + &new_link("a", ".git/hooks");
    let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();
    assert!(matches!(err, PatchError::BeyondSymlink(_)), "{err:?}");
    assert_untouched(dir.path());
}

#[test]
fn a_link_differing_only_in_case_counts_where_case_folds() {
    let (dir, repo) = repo_with_a_commit();
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    let patch = new_link("A", ".git/hooks") + &new_link("a/pre-commit", "../../evil.sh");
    let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();
    assert!(matches!(err, PatchError::BeyondSymlink(_)), "{err:?}");
    assert_untouched(dir.path());
}

#[test]
fn a_link_at_another_spelling_of_the_git_folder_is_refused() {
    for path in [
        "git~1/hooks/pre-commit",
        "GIT~1/hooks/pre-commit",
        ".git./hooks/pre-commit",
        ".GIT /hooks/pre-commit",
        ".git::$INDEX_ALLOCATION/hooks/pre-commit",
        "\u{200c}.git/hooks/pre-commit",
        "sub\\.git\\hooks\\pre-commit",
    ] {
        let (dir, repo) = repo_with_a_commit();
        let err = apply_patch(&repo, new_link(path, "echo pwned").as_bytes()).unwrap_err();
        assert!(matches!(err, PatchError::InvalidPath(_)), "{path}: {err:?}");
        assert_untouched(dir.path());
    }
}

#[test]
fn a_link_into_the_git_folder_is_refused() {
    let (dir, repo) = repo_with_a_commit();
    let name = dir
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let absolute = repo.path().join("hooks");
    for target in [
        ".git/hooks".to_string(),
        "./.git".to_string(),
        ".GIT/hooks".to_string(),
        format!("../{name}/.git/hooks"),
        "sub/../.git".to_string(),
        absolute.to_str().unwrap().to_string(),
    ] {
        let err = apply_patch(&repo, new_link("l", &target).as_bytes()).unwrap_err();
        assert!(
            matches!(&err, PatchError::LinkIntoGitDir { path, .. } if path == "l"),
            "{target}: {err:?}"
        );
        assert_untouched(dir.path());
    }
    // From a subfolder, `..` climbs to the work tree.
    let err = apply_patch(&repo, new_link("d/l", "../.git/hooks").as_bytes()).unwrap_err();
    assert!(matches!(err, PatchError::LinkIntoGitDir { .. }), "{err:?}");
    assert_untouched(dir.path());
}

#[test]
fn git_control_files_are_not_written_as_links() {
    for path in [
        ".gitmodules",
        "sub/.gitattributes",
        ".GITIGNORE",
        ".mailmap.",
    ] {
        let (dir, repo) = repo_with_a_commit();
        let err = apply_patch(&repo, new_link(path, "elsewhere").as_bytes()).unwrap_err();
        assert!(matches!(err, PatchError::InvalidPath(_)), "{path}: {err:?}");
        assert_untouched(dir.path());
    }
}

#[cfg(unix)]
#[test]
fn ordinary_links_next_to_files_still_apply() {
    let (dir, repo) = repo_with_a_commit();
    let patch = new_link("docs/latest", "v2/index.md")
        + &new_link("up", "../elsewhere")
        + &new_link(".github-link", "README")
        + &new_file("docs/v2/index.md", "hello")
        + &new_file("docs/latest-notes", "notes");
    apply_patch(&repo, patch.as_bytes()).unwrap();
    assert_eq!(
        std::fs::read_link(dir.path().join("docs/latest")).unwrap(),
        Path::new("v2/index.md")
    );
    assert_eq!(
        std::fs::read_link(dir.path().join("up")).unwrap(),
        Path::new("../elsewhere")
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("docs/latest-notes")).unwrap(),
        "notes\n"
    );
}
