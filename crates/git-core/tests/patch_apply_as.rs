//! #658: a patch file previewed, applied and staged, or applied as commits with its mails'
//! authorship; #536: the same checks for every way of applying.
mod common;

use common::{commit_all, init_repo, read, write_file};
use git2::Repository;
use git_core::patch::{
    apply_patch_as, commits_patch, preview_patch, PatchApply, PatchError, PreviewStatus,
};
use std::path::Path;

fn head(repo: &Repository) -> git2::Commit<'_> {
    repo.head().unwrap().peel_to_commit().unwrap()
}

/// Commits everything as `name` at `seconds` (offset +0130).
fn commit_as(repo: &Repository, message: &str, name: &str, seconds: i64) -> String {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let author =
        git2::Signature::new(name, "author@example.com", &git2::Time::new(seconds, 90)).unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &author, &author, message, &tree, &parents)
        .unwrap()
        .to_string()
}

/// A base commit, then two by another author (the second adds `g.txt`); the series of those
/// two, and HEAD reset to the base.
fn series_on_base() -> (tempfile::TempDir, Repository, String, Vec<u8>) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "one\n");
    commit_all(&repo, "base");
    let base = head(&repo).id().to_string();
    write_file(dir.path(), "f.txt", "two\n");
    let second = commit_as(
        &repo,
        "Second\n\nWhy it changed.\n",
        "Ada Lovelace",
        1_000_000,
    );
    write_file(dir.path(), "f.txt", "three\n");
    write_file(dir.path(), "g.txt", "g\n");
    let third = commit_as(&repo, "Third", "Ada Lovelace", 2_000_000);
    let series = commits_patch(&repo, &[second, third]).unwrap();
    reset_hard(&repo, &base);
    assert!(!dir.path().join("g.txt").exists());
    (dir, repo, base, series)
}

fn reset_hard(repo: &Repository, id: &str) {
    let commit = repo.find_commit(git2::Oid::from_str(id).unwrap()).unwrap();
    repo.reset(commit.as_object(), git2::ResetType::Hard, None)
        .unwrap();
}

fn status(repo: &Repository) -> Vec<git_core::status::StatusEntry> {
    git_core::status::status(repo).unwrap()
}

#[test]
fn a_series_applied_as_commits_keeps_each_mails_author_date_and_message() {
    let (dir, repo, base, series) = series_on_base();
    let ids = apply_patch_as(&repo, &series, PatchApply::Commits).unwrap();
    assert_eq!(ids.len(), 2);
    let third = head(&repo);
    assert_eq!(third.id().to_string(), ids[1]);
    assert_eq!(third.message().unwrap(), "Third\n");
    assert_eq!(third.author().name().unwrap(), "Ada Lovelace");
    assert_eq!(third.author().email().unwrap(), "author@example.com");
    assert_eq!(third.author().when().seconds(), 2_000_000);
    assert_eq!(third.author().when().offset_minutes(), 90);
    assert_eq!(third.committer().name().unwrap(), "Test User");
    let second = third.parent(0).unwrap();
    assert_eq!(second.id().to_string(), ids[0]);
    assert_eq!(second.message().unwrap(), "Second\n\nWhy it changed.\n");
    assert_eq!(second.author().when().seconds(), 1_000_000);
    assert_eq!(second.parent_id(0).unwrap().to_string(), base);
    assert_eq!(read(dir.path(), "f.txt"), "three\n");
    assert_eq!(read(dir.path(), "g.txt"), "g\n");
    assert!(status(&repo).is_empty(), "{:?}", status(&repo));
}

#[test]
fn a_series_that_fails_as_commits_puts_head_the_index_and_the_files_back() {
    let (dir, repo, base, series) = series_on_base();
    // The second mail creates g.txt, which is there now (untracked).
    write_file(dir.path(), "g.txt", "mine\n");
    let err = apply_patch_as(&repo, &series, PatchApply::Commits).unwrap_err();
    assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
    assert_eq!(head(&repo).id().to_string(), base);
    assert_eq!(read(dir.path(), "f.txt"), "one\n");
    assert_eq!(read(dir.path(), "g.txt"), "mine\n");
    let entries = status(&repo);
    assert!(
        entries.len() == 1 && entries[0].path == "g.txt" && !entries[0].staged,
        "{entries:?}"
    );
}

#[test]
fn as_commits_starts_an_unborn_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "one\n");
    let root = commit_as(&repo, "Root", "Ada Lovelace", 1_000_000);
    let patch = commits_patch(&repo, &[root]).unwrap();

    let (other_dir, other) = init_repo();
    write_file(other_dir.path(), "h.txt", "h\n");
    let mut index = other.index().unwrap();
    index.add_path(Path::new("h.txt")).unwrap();
    index.write().unwrap();
    // Staged on an unborn branch: refused, then committed once nothing is staged.
    let err = apply_patch_as(&other, &patch, PatchApply::Commits).unwrap_err();
    assert!(matches!(err, PatchError::StagedChanges), "{err:?}");
    assert!(other.head().is_err());
    index.remove_path(Path::new("h.txt")).unwrap();
    index.write().unwrap();
    let ids = apply_patch_as(&other, &patch, PatchApply::Commits).unwrap();
    let commit = head(&other);
    assert_eq!(commit.id().to_string(), ids[0]);
    assert_eq!(commit.parent_count(), 0);
    assert_eq!(commit.author().name().unwrap(), "Ada Lovelace");
    assert_eq!(read(other_dir.path(), "f.txt"), "one\n");
}

#[test]
fn a_series_that_fails_on_an_unborn_branch_leaves_it_unborn() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "one\n");
    let root = commit_as(&repo, "Root", "Ada Lovelace", 1_000_000);
    write_file(dir.path(), "g.txt", "g\n");
    let second = commit_as(&repo, "Second", "Ada Lovelace", 1_000_100);
    let series = commits_patch(&repo, &[root, second]).unwrap();

    let (other_dir, other) = init_repo();
    write_file(other_dir.path(), "g.txt", "mine\n");
    let err = apply_patch_as(&other, &series, PatchApply::Commits).unwrap_err();
    assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
    assert!(other.head().is_err(), "the branch was left with a commit");
    assert!(!other_dir.path().join("f.txt").exists());
    assert!(other.index().unwrap().is_empty());
}

#[test]
fn as_commits_needs_mails_no_staged_changes_and_no_operation() {
    let (dir, repo, base, series) = series_on_base();
    let plain = "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-one\n+uno\n";
    let err = apply_patch_as(&repo, plain.as_bytes(), PatchApply::Commits).unwrap_err();
    assert!(matches!(err, PatchError::NotMails), "{err:?}");

    write_file(dir.path(), "h.txt", "h\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("h.txt")).unwrap();
    index.write().unwrap();
    let err = apply_patch_as(&repo, &series, PatchApply::Commits).unwrap_err();
    assert!(matches!(err, PatchError::StagedChanges), "{err:?}");
    index.remove_path(Path::new("h.txt")).unwrap();
    index.write().unwrap();

    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{base}\n")).unwrap();
    let err = apply_patch_as(&repo, &series, PatchApply::Commits).unwrap_err();
    assert!(matches!(err, PatchError::OperationInProgress), "{err:?}");
    std::fs::remove_file(repo.path().join("MERGE_HEAD")).unwrap();

    assert_eq!(head(&repo).id().to_string(), base);
    assert_eq!(read(dir.path(), "f.txt"), "one\n");
}

#[test]
fn as_commits_needs_an_identity() {
    let (_dir, repo, base, series) = series_on_base();
    let mut config = repo.config().unwrap();
    config.remove("user.name").unwrap();
    config.remove("user.email").unwrap();
    let err = apply_patch_as(&repo, &series, PatchApply::Commits).unwrap_err();
    assert!(matches!(err, PatchError::MissingIdentity), "{err:?}");
    assert_eq!(head(&repo).id().to_string(), base);
}

#[test]
fn a_series_applied_and_staged_lands_in_the_index() {
    let (dir, repo, base, series) = series_on_base();
    let ids = apply_patch_as(&repo, &series, PatchApply::Stage).unwrap();
    assert!(ids.is_empty());
    assert_eq!(head(&repo).id().to_string(), base);
    assert_eq!(read(dir.path(), "g.txt"), "g\n");
    let entries = status(&repo);
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert!(entries.iter().all(|e| e.staged), "{entries:?}");
}

#[test]
fn a_series_that_fails_staged_puts_the_index_back() {
    let (dir, repo, _, series) = series_on_base();
    write_file(dir.path(), "g.txt", "mine\n");
    let err = apply_patch_as(&repo, &series, PatchApply::Stage).unwrap_err();
    assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
    assert_eq!(read(dir.path(), "f.txt"), "one\n");
    let entries = status(&repo);
    assert!(entries.iter().all(|e| !e.staged), "{entries:?}");
}

#[test]
fn applying_and_staging_names_a_file_with_unstaged_changes() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "one\ntwo\nthree\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "f.txt", "one\ntwo\nthree\nfour\n");
    let patch =
        "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n-one\n+uno\n two\n three\n";
    let err = apply_patch_as(&repo, patch.as_bytes(), PatchApply::Stage).unwrap_err();
    assert!(
        matches!(&err, PatchError::UnstagedChanges(p) if p == "f.txt"),
        "{err:?}"
    );
    assert_eq!(read(dir.path(), "f.txt"), "one\ntwo\nthree\nfour\n");
    // Without staging it applies over the local edit.
    apply_patch_as(&repo, patch.as_bytes(), PatchApply::WorkTree).unwrap();
    assert_eq!(read(dir.path(), "f.txt"), "uno\ntwo\nthree\nfour\n");
}

#[test]
fn every_way_of_applying_refuses_a_path_in_git_or_outside() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let before = head(&repo).id();
    for name in ["../escape.txt", ".git/config2", "sub/.GIT/x"] {
        let diff = format!(
            "diff --git a/{name} b/{name}\nnew file mode 100644\n--- /dev/null\n+++ b/{name}\n@@ -0,0 +1 @@\n+x\n"
        );
        let mail = format!(
            "From 1234567890123456789012345678901234567890 Mon Sep 17 00:00:00 2001\nFrom: A <a@x>\nDate: Thu, 1 Jan 1970 00:00:00 +0000\nSubject: [PATCH] x\n\n---\n{diff}"
        );
        for how in [PatchApply::WorkTree, PatchApply::Stage, PatchApply::Commits] {
            let err = apply_patch_as(&repo, mail.as_bytes(), how).unwrap_err();
            assert!(
                matches!(err, PatchError::InvalidPath(_) | PatchError::Unreadable(_)),
                "{name} {how:?}: {err:?}"
            );
        }
        let preview = preview_patch(&repo, mail.as_bytes());
        match preview {
            Ok(preview) => assert!(preview.problem.is_some(), "{name}: {preview:?}"),
            Err(err) => assert!(matches!(err, PatchError::Unreadable(_)), "{name}: {err:?}"),
        }
    }
    assert!(!dir.path().parent().unwrap().join("escape.txt").exists());
    assert!(!dir.path().join(".git/config2").exists());
    assert_eq!(head(&repo).id(), before);
}

#[test]
fn every_way_of_applying_refuses_a_link_into_git() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "README", "readme\n");
    commit_all(&repo, "base");
    let patch = "diff --git a/l b/l\nnew file mode 120000\nindex 0000000..1111111\n--- /dev/null\n+++ b/l\n@@ -0,0 +1 @@\n+.git/hooks\n\\ No newline at end of file\n";
    for how in [PatchApply::WorkTree, PatchApply::Stage] {
        let err = apply_patch_as(&repo, patch.as_bytes(), how).unwrap_err();
        assert!(
            matches!(
                err,
                PatchError::LinkIntoGitDir { .. } | PatchError::InvalidPath(_)
            ),
            "{how:?}: {err:?}"
        );
    }
    let preview = preview_patch(&repo, patch.as_bytes()).unwrap();
    assert!(preview.problem.is_some(), "{preview:?}");
    assert!(dir.path().join("l").symlink_metadata().is_err());
}

#[test]
fn a_preview_lists_the_mails_and_their_files_and_changes_nothing() {
    let (dir, repo, base, series) = series_on_base();
    let preview = preview_patch(&repo, &series).unwrap();
    assert_eq!(preview.problem, None);
    assert_eq!(preview.parts.len(), 2);
    let first = preview.parts[0].mail.as_ref().unwrap();
    assert_eq!(first.subject, "Second");
    assert_eq!(first.body, "Why it changed.");
    assert_eq!(first.author_name, "Ada Lovelace");
    assert_eq!(first.author_email, "author@example.com");
    assert_eq!(first.timestamp, Some(1_000_000));
    let files = &preview.parts[1].files;
    assert_eq!(files.len(), 2, "{files:?}");
    let f = files.iter().find(|f| f.path == "f.txt").unwrap();
    assert_eq!(
        (f.status, f.additions, f.deletions),
        (PreviewStatus::Modified, 1, 1)
    );
    let g = files.iter().find(|f| f.path == "g.txt").unwrap();
    assert_eq!(
        (g.status, g.additions, g.deletions),
        (PreviewStatus::Added, 1, 0)
    );
    assert_eq!(head(&repo).id().to_string(), base);
    assert_eq!(read(dir.path(), "f.txt"), "one\n");
    assert!(!dir.path().join("g.txt").exists());
}

#[test]
fn a_preview_says_why_the_patch_does_not_apply() {
    let (dir, repo, _, series) = series_on_base();
    write_file(dir.path(), "f.txt", "changed since\n");
    let preview = preview_patch(&repo, &series).unwrap();
    let problem = preview.problem.expect("a problem");
    assert!(problem.contains("doesn't apply"), "{problem}");
    assert_eq!(read(dir.path(), "f.txt"), "changed since\n");
}

#[test]
fn a_plain_diff_previews_without_a_mail() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "one\n");
    commit_all(&repo, "base");
    let patch = "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-one\n+uno\n";
    let preview = preview_patch(&repo, patch.as_bytes()).unwrap();
    assert_eq!(preview.parts.len(), 1);
    assert_eq!(preview.parts[0].mail, None);
    assert_eq!(preview.parts[0].files[0].path, "f.txt");
    assert_eq!(preview.problem, None);
    let err = preview_patch(&repo, b"hello, this is not a patch\n").unwrap_err();
    assert!(
        matches!(err, PatchError::Unreadable(_) | PatchError::EmptyPatch),
        "{err:?}"
    );
}

#[test]
fn a_single_mail_applies_as_one_commit() {
    let (_dir, repo, base, series) = series_on_base();
    let text = String::from_utf8(series).unwrap();
    let second_mail = text.rfind("\nFrom ").unwrap();
    let first = &text[..second_mail + 1];
    let ids = apply_patch_as(&repo, first.as_bytes(), PatchApply::Commits).unwrap();
    assert_eq!(ids.len(), 1);
    let commit = head(&repo);
    assert_eq!(commit.summary().unwrap(), Some("Second"));
    assert_eq!(commit.parent_id(0).unwrap().to_string(), base);
}

#[test]
fn a_preview_names_deletions_renames_and_binary_files() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "gone.txt", "bye\n");
    write_file(dir.path(), "old.txt", "same\n");
    commit_all(&repo, "base");
    let patch = "diff --git a/gone.txt b/gone.txt\ndeleted file mode 100644\n--- a/gone.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-bye\n\
diff --git a/old.txt b/new.txt\nsimilarity index 100%\nrename from old.txt\nrename to new.txt\n\
diff --git a/logo.png b/logo.png\nnew file mode 100644\nindex 0000000..1111111\nBinary files /dev/null and b/logo.png differ\n\
diff --git a/old.txt b/copy.txt\nsimilarity index 100%\ncopy from old.txt\ncopy to copy.txt\n";
    let preview = preview_patch(&repo, patch.as_bytes()).unwrap();
    let files = &preview.parts[0].files;
    assert_eq!(files.len(), 4, "{files:?}");
    assert_eq!(files[0].path, "gone.txt");
    assert_eq!(
        (files[0].status, files[0].deletions, files[0].additions),
        (PreviewStatus::Deleted, 1, 0)
    );
    assert_eq!(files[1].path, "new.txt");
    assert_eq!(files[1].old_path.as_deref(), Some("old.txt"));
    assert_eq!(files[1].status, PreviewStatus::Renamed);
    assert_eq!(files[2].path, "logo.png");
    assert!(files[2].binary);
    assert!(!files[0].binary);
    assert_eq!(files[0].old_path, None);
    assert_eq!(files[3].path, "copy.txt");
    assert_eq!(files[3].status, PreviewStatus::Copied);
    assert_eq!(files[3].old_path.as_deref(), Some("old.txt"));
}

#[test]
fn as_commits_on_a_detached_head_moves_it_or_puts_it_back() {
    let (dir, repo, base, series) = series_on_base();
    repo.set_head_detached(git2::Oid::from_str(&base).unwrap())
        .unwrap();
    write_file(dir.path(), "g.txt", "mine\n");
    apply_patch_as(&repo, &series, PatchApply::Commits).unwrap_err();
    assert!(repo.head_detached().unwrap());
    assert_eq!(head(&repo).id().to_string(), base);
    // The branch HEAD left was never touched.
    let branch = repo.find_branch("master", git2::BranchType::Local).ok();
    let main = branch.or_else(|| repo.find_branch("main", git2::BranchType::Local).ok());
    assert_eq!(main.unwrap().get().target().unwrap().to_string(), base);

    std::fs::remove_file(dir.path().join("g.txt")).unwrap();
    let ids = apply_patch_as(&repo, &series, PatchApply::Commits).unwrap();
    assert!(repo.head_detached().unwrap());
    assert_eq!(head(&repo).id().to_string(), ids[1]);
}

#[test]
fn applying_and_staging_names_a_file_deleted_in_the_working_tree() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "one\n");
    commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("f.txt")).unwrap();
    let patch = "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-one\n+uno\n";
    let err = apply_patch_as(&repo, patch.as_bytes(), PatchApply::Stage).unwrap_err();
    assert!(
        matches!(&err, PatchError::UnstagedChanges(p) if p == "f.txt"),
        "{err:?}"
    );
    // A staged change alone is no reason to refuse.
    write_file(dir.path(), "f.txt", "one\n");
    write_file(dir.path(), "h.txt", "h\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("h.txt")).unwrap();
    index.write().unwrap();
    apply_patch_as(&repo, patch.as_bytes(), PatchApply::Stage).unwrap();
    assert_eq!(read(dir.path(), "f.txt"), "uno\n");
}

#[test]
fn as_commits_refuses_an_index_with_conflicts() {
    let (dir, repo, base, series) = series_on_base();
    let mut index = repo.index().unwrap();
    let mut entry = index.get_path(Path::new("f.txt"), 0).unwrap();
    index.remove_path(Path::new("f.txt")).unwrap();
    entry.flags = (entry.flags & !0x3000) | (1 << 12);
    index.add(&entry).unwrap();
    index.write().unwrap();
    assert!(repo.index().unwrap().has_conflicts());
    let err = apply_patch_as(&repo, &series, PatchApply::Commits).unwrap_err();
    assert!(matches!(err, PatchError::OperationInProgress), "{err:?}");
    assert_eq!(head(&repo).id().to_string(), base);
    assert_eq!(read(dir.path(), "f.txt"), "one\n");
}

/// #747: "Apply as commits" runs `git am`'s hooks: `applypatch-msg` (may rewrite or reject the
/// message) before each mail is applied, `pre-applypatch` (may reject) once the mail is in the
/// index and before its commit, `post-applypatch` (ignored) once the series landed.
#[cfg(unix)]
mod hooks {
    use super::*;
    use git_core::hooks::{hooks_dir, HookError, HookOptions, HookOutcome, HookReporter};
    use git_core::patch::apply_patch_with;

    fn apply_with_hooks(
        repo: &Repository,
        series: &[u8],
        how: PatchApply,
    ) -> Result<Vec<String>, PatchError> {
        apply_patch_with(repo, series, how, HookOptions::default())
    }

    fn log_path(repo: &Repository) -> std::path::PathBuf {
        repo.path().join("hook-log")
    }

    fn read_hook_log(repo: &Repository) -> String {
        std::fs::read_to_string(log_path(repo)).unwrap_or_default()
    }

    /// `name`'s hook appends `<name>` and then whatever `body` prints to the log.
    fn logging_hook(repo: &Repository, name: &str, body: &str) {
        let log = log_path(repo);
        common::install_hook(
            &hooks_dir(repo).unwrap(),
            name,
            &format!("{{ echo {name}; {body} }} >> '{}'", log.display()),
        );
    }

    fn assert_put_back(dir: &std::path::Path, repo: &Repository, base: &str) {
        assert_eq!(head(repo).id().to_string(), base);
        assert_eq!(read(dir, "f.txt"), "one\n");
        assert!(!dir.join("g.txt").exists());
        assert!(status(repo).is_empty(), "{:?}", status(repo));
    }

    #[test]
    fn applypatch_msg_gets_each_mails_message_in_a_file() {
        let (_dir, repo, _base, series) = series_on_base();
        logging_hook(&repo, "applypatch-msg", "cat \"$1\";");

        apply_with_hooks(&repo, &series, PatchApply::Commits).unwrap();

        assert_eq!(
            read_hook_log(&repo),
            "applypatch-msg\nSecond\n\nWhy it changed.\napplypatch-msg\nThird\n"
        );
    }

    #[test]
    fn applypatch_msg_can_rewrite_the_message() {
        let (_dir, repo, _base, series) = series_on_base();
        common::install_hook(
            &hooks_dir(&repo).unwrap(),
            "applypatch-msg",
            "printf 'Checked: %s\\n' \"$(head -n 1 \"$1\")\" > \"$1\"",
        );

        apply_with_hooks(&repo, &series, PatchApply::Commits).unwrap();

        let third = head(&repo);
        assert_eq!(third.message().unwrap(), "Checked: Third\n");
        assert_eq!(
            third.parent(0).unwrap().message().unwrap(),
            "Checked: Second\n"
        );
    }

    #[test]
    fn a_failing_applypatch_msg_puts_the_whole_series_back() {
        let (dir, repo, base, series) = series_on_base();
        // Accepts the first mail, rejects the second.
        common::install_hook(
            &hooks_dir(&repo).unwrap(),
            "applypatch-msg",
            "if grep -q Third \"$1\"; then echo 'no Third here' >&2; exit 1; fi",
        );

        let err = apply_with_hooks(&repo, &series, PatchApply::Commits).unwrap_err();

        match &err {
            PatchError::HookFailed {
                hook,
                output,
                subject,
            } => {
                assert_eq!(hook, "applypatch-msg");
                assert_eq!(output, "no Third here\n");
                assert_eq!(subject, "Third");
            }
            other => panic!("expected HookFailed, got {other:?}"),
        }
        assert_eq!(
            err.to_string(),
            "The applypatch-msg hook failed on \"Third\". Nothing was applied."
        );
        assert_put_back(dir.path(), &repo, &base);
    }

    #[test]
    fn pre_applypatch_runs_with_the_mail_in_the_index_before_its_commit() {
        if !common::program_available("git") {
            eprintln!("git CLI not found; skipping");
            return;
        }
        let (_dir, repo, base, series) = series_on_base();
        logging_hook(
            &repo,
            "pre-applypatch",
            "git diff --cached --name-only; git rev-parse HEAD;",
        );

        let ids = apply_with_hooks(&repo, &series, PatchApply::Commits).unwrap();

        assert_eq!(
            read_hook_log(&repo),
            format!(
                "pre-applypatch\nf.txt\n{base}\npre-applypatch\nf.txt\ng.txt\n{}\n",
                ids[0]
            )
        );
    }

    #[test]
    fn a_failing_pre_applypatch_puts_the_whole_series_back() {
        let (dir, repo, base, series) = series_on_base();
        // Accepts the first mail, rejects the second (which creates g.txt).
        common::install_hook(
            &hooks_dir(&repo).unwrap(),
            "pre-applypatch",
            "if [ -e g.txt ]; then echo 'g.txt is not allowed'; exit 3; fi",
        );

        let err = apply_with_hooks(&repo, &series, PatchApply::Commits).unwrap_err();

        match &err {
            PatchError::HookFailed {
                hook,
                output,
                subject,
            } => {
                assert_eq!(hook, "pre-applypatch");
                assert_eq!(output, "g.txt is not allowed\n");
                assert_eq!(subject, "Third");
            }
            other => panic!("expected HookFailed, got {other:?}"),
        }
        assert_put_back(dir.path(), &repo, &base);
    }

    #[test]
    fn files_pre_applypatch_stages_are_part_of_the_commit() {
        if !common::program_available("git") {
            eprintln!("git CLI not found; skipping");
            return;
        }
        let (dir, repo, _base, series) = series_on_base();
        write_file(dir.path(), "extra.txt", "made by the hook\n");
        common::install_hook(
            &hooks_dir(&repo).unwrap(),
            "pre-applypatch",
            "git add extra.txt",
        );

        let ids = apply_with_hooks(&repo, &series, PatchApply::Commits).unwrap();

        let first = repo
            .find_commit(git2::Oid::from_str(&ids[0]).unwrap())
            .unwrap();
        assert!(first.tree().unwrap().get_name("extra.txt").is_some());
        assert!(status(&repo).is_empty(), "{:?}", status(&repo));
    }

    #[test]
    fn post_applypatch_runs_for_each_commit_once_the_series_landed_and_its_failure_is_ignored() {
        let (_dir, repo, _base, series) = series_on_base();
        common::log_hooks(&repo, &["applypatch-msg", "pre-applypatch"]);
        let log = log_path(&repo);
        common::install_hook(
            &hooks_dir(&repo).unwrap(),
            "post-applypatch",
            &format!("echo \"post-applypatch $*\" >> '{}'; exit 1", log.display()),
        );

        let ids = apply_with_hooks(&repo, &series, PatchApply::Commits).unwrap();

        assert_eq!(ids.len(), 2);
        assert_eq!(head(&repo).id().to_string(), ids[1]);
        assert_eq!(
            common::read_log(&repo),
            "applypatch-msg <msg>\npre-applypatch \napplypatch-msg <msg>\npre-applypatch \n\
             post-applypatch \npost-applypatch \n"
        );
    }

    #[test]
    fn no_post_applypatch_runs_when_the_series_is_put_back() {
        let (dir, repo, base, series) = series_on_base();
        common::log_hooks(&repo, &["post-applypatch"]);
        common::install_hook(
            &hooks_dir(&repo).unwrap(),
            "pre-applypatch",
            "if [ -e g.txt ]; then exit 1; fi",
        );

        apply_with_hooks(&repo, &series, PatchApply::Commits).unwrap_err();

        assert_eq!(common::read_log(&repo), "");
        assert_put_back(dir.path(), &repo, &base);
    }

    #[test]
    fn no_verify_skips_applypatch_msg_and_pre_applypatch_but_not_post_applypatch() {
        let (_dir, repo, _base, series) = series_on_base();
        let hooks = hooks_dir(&repo).unwrap();
        common::install_hook(&hooks, "applypatch-msg", "exit 1");
        common::install_hook(&hooks, "pre-applypatch", "exit 1");
        let log = log_path(&repo);
        common::install_hook(
            &hooks,
            "post-applypatch",
            &format!("echo post-applypatch >> '{}'", log.display()),
        );

        let ids = apply_patch_with(
            &repo,
            &series,
            PatchApply::Commits,
            HookOptions {
                no_verify: true,
                ..HookOptions::default()
            },
        )
        .unwrap();

        assert_eq!(ids.len(), 2);
        assert_eq!(read_hook_log(&repo), "post-applypatch\npost-applypatch\n");
    }

    #[test]
    fn applying_without_committing_runs_no_hooks() {
        for how in [PatchApply::WorkTree, PatchApply::Stage] {
            let (_dir, repo, _base, series) = series_on_base();
            common::log_hooks(
                &repo,
                &["applypatch-msg", "pre-applypatch", "post-applypatch"],
            );

            apply_with_hooks(&repo, &series, how).unwrap();

            assert_eq!(common::read_log(&repo), "", "{how:?}");
        }
    }

    #[test]
    fn apply_patch_as_runs_the_hooks_too() {
        let (dir, repo, base, series) = series_on_base();
        common::install_hook(&hooks_dir(&repo).unwrap(), "applypatch-msg", "exit 1");

        let err = apply_patch_as(&repo, &series, PatchApply::Commits).unwrap_err();

        assert!(matches!(err, PatchError::HookFailed { .. }), "{err:?}");
        assert_put_back(dir.path(), &repo, &base);
    }

    #[test]
    fn a_hook_that_cannot_start_rejects_like_a_failing_one() {
        let (dir, repo, base, series) = series_on_base();
        let path = common::install_hook(&hooks_dir(&repo).unwrap(), "pre-applypatch", "");
        std::fs::write(&path, "#!/no/such/interpreter\n").unwrap();

        let err = apply_with_hooks(&repo, &series, PatchApply::Commits).unwrap_err();

        match &err {
            PatchError::HookFailed { hook, output, .. } => {
                assert_eq!(hook, "pre-applypatch");
                assert!(output.contains("#! line"), "{output}");
            }
            other => panic!("expected HookFailed, got {other:?}"),
        }
        assert_put_back(dir.path(), &repo, &base);
    }

    #[derive(Default)]
    struct Recorder(Vec<String>);
    impl HookReporter for Recorder {
        fn started(&mut self, hook: &str) {
            self.0.push(format!("start {hook}"));
        }
        fn finished(&mut self, hook: &str, _result: &Result<HookOutcome, HookError>) {
            self.0.push(format!("end {hook}"));
        }
    }

    #[test]
    fn the_reporter_sees_each_hook_and_cancel_puts_the_series_back() {
        let (dir, repo, base, series) = series_on_base();
        common::install_hook(&hooks_dir(&repo).unwrap(), "pre-applypatch", "sleep 30");
        let mut recorder = Recorder::default();
        let started = std::time::Instant::now();

        let err = apply_patch_with(
            &repo,
            &series,
            PatchApply::Commits,
            HookOptions {
                cancel: Some(&|| true),
                reporter: Some(&mut recorder),
                ..HookOptions::default()
            },
        )
        .unwrap_err();

        assert!(
            matches!(err, PatchError::Hook(HookError::Cancelled { .. })),
            "{err:?}"
        );
        assert_eq!(err.to_string(), "The pre-applypatch hook was cancelled.");
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
        assert_eq!(
            recorder.0,
            vec!["start pre-applypatch", "end pre-applypatch"]
        );
        assert_put_back(dir.path(), &repo, &base);
    }
}
