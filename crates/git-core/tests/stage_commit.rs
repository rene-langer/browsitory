use git_core::diff::DiffOptions;
mod common;

use common::{init_repo, write_file};
use git_core::commit::commit;
use git_core::stage::{discard_hunk, stage_file, stage_hunk, unstage_file, unstage_hunk};
use git_core::status::StatusKind;

#[test]
fn stage_file_adds_a_new_file_to_the_index() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "new.txt", "hello");

    stage_file(&repo, "new.txt").unwrap();

    let entries = git_core::status::status(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "new.txt");
    assert!(entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::New);
}

#[test]
fn stage_file_stages_a_deletion() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "hello");
    stage_file(&repo, "tracked.txt").unwrap();
    commit(
        &mut repo,
        "add file",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    std::fs::remove_file(dir.path().join("tracked.txt")).unwrap();
    stage_file(&repo, "tracked.txt").unwrap();

    let entries = git_core::status::status(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "tracked.txt");
    assert!(entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::Deleted);
}

#[test]
fn unstage_file_restores_the_index_entry_from_head() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "hello");
    stage_file(&repo, "tracked.txt").unwrap();
    commit(
        &mut repo,
        "add file",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    write_file(dir.path(), "tracked.txt", "changed");
    stage_file(&repo, "tracked.txt").unwrap();

    unstage_file(&repo, "tracked.txt").unwrap();

    let entries = git_core::status::status(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "tracked.txt");
    assert!(!entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::Modified);
}

#[test]
fn unstage_file_on_a_newly_staged_file_makes_it_untracked_again() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "new.txt", "hello");
    stage_file(&repo, "new.txt").unwrap();

    unstage_file(&repo, "new.txt").unwrap();

    let entries = git_core::status::status(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "new.txt");
    assert!(!entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::New);
}

#[test]
fn stage_hunk_stages_only_the_targeted_hunk_leaving_the_other_unstaged() {
    let (dir, mut repo) = init_repo();
    let original: String = (1..=15).map(|n| format!("line {n}\n")).collect();
    write_file(dir.path(), "tracked.txt", &original);
    stage_file(&repo, "tracked.txt").unwrap();
    commit(
        &mut repo,
        "initial commit",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let mut lines: Vec<String> = (1..=15).map(|n| format!("line {n}")).collect();
    lines[1] = "line 2 changed".to_string();
    lines[13] = "line 14 changed".to_string();
    let changed = lines.join("\n") + "\n";
    write_file(dir.path(), "tracked.txt", &changed);

    let hunks =
        git_core::diff::working_diff(&repo, "tracked.txt", false, &DiffOptions::default()).unwrap();
    assert_eq!(
        hunks.len(),
        2,
        "expected two separate hunks from two far-apart edits"
    );

    stage_hunk(
        &repo,
        "tracked.txt",
        hunks[0].old_start,
        hunks[0].new_start,
        3,
    )
    .unwrap();

    let staged =
        git_core::diff::working_diff(&repo, "tracked.txt", true, &DiffOptions::default()).unwrap();
    let staged_text: String = staged
        .iter()
        .flat_map(|h| h.lines.iter())
        .map(|l| l.content.clone())
        .collect();
    assert!(staged_text.contains("line 2 changed"));
    assert!(!staged_text.contains("line 14 changed"));

    let still_unstaged =
        git_core::diff::working_diff(&repo, "tracked.txt", false, &DiffOptions::default()).unwrap();
    let unstaged_text: String = still_unstaged
        .iter()
        .flat_map(|h| h.lines.iter())
        .map(|l| l.content.clone())
        .collect();
    assert!(unstaged_text.contains("line 14 changed"));
    assert!(!unstaged_text.contains("line 2 changed"));
}

#[test]
fn stage_hunk_on_a_hunk_that_no_longer_matches_returns_hunk_not_found() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\n");
    stage_file(&repo, "tracked.txt").unwrap();
    commit(
        &mut repo,
        "initial commit",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    write_file(dir.path(), "tracked.txt", "line one changed\n");

    let result = stage_hunk(&repo, "tracked.txt", 999, 999, 3);

    assert!(matches!(
        result,
        Err(git_core::stage::StageError::HunkNotFound)
    ));
}

#[test]
fn unstage_hunk_unstages_only_the_targeted_hunk_leaving_the_other_staged() {
    let (dir, mut repo) = init_repo();
    let original: String = (1..=15).map(|n| format!("line {n}\n")).collect();
    write_file(dir.path(), "tracked.txt", &original);
    stage_file(&repo, "tracked.txt").unwrap();
    commit(
        &mut repo,
        "initial commit",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let mut lines: Vec<String> = (1..=15).map(|n| format!("line {n}")).collect();
    lines[1] = "line 2 changed".to_string();
    lines[13] = "line 14 changed".to_string();
    let changed = lines.join("\n") + "\n";
    write_file(dir.path(), "tracked.txt", &changed);

    // Capture hunk identity before staging — same (old_start, new_start) pair identifies the
    // hunk in both the unstaged diff (index vs workdir) and, once staged, the staged diff
    // (HEAD vs index): HEAD is the "old" side in both cases, and staging the whole file just
    // copies workdir content into the index unchanged, so the "new" side matches too.
    let hunks_before =
        git_core::diff::working_diff(&repo, "tracked.txt", false, &DiffOptions::default()).unwrap();
    assert_eq!(hunks_before.len(), 2);
    let target = (hunks_before[0].old_start, hunks_before[0].new_start);

    stage_file(&repo, "tracked.txt").unwrap();

    unstage_hunk(&repo, "tracked.txt", target.0, target.1, 3).unwrap();

    let staged =
        git_core::diff::working_diff(&repo, "tracked.txt", true, &DiffOptions::default()).unwrap();
    let staged_text: String = staged
        .iter()
        .flat_map(|h| h.lines.iter())
        .map(|l| l.content.clone())
        .collect();
    assert!(!staged_text.contains("line 2 changed"));
    assert!(staged_text.contains("line 14 changed"));

    let unstaged =
        git_core::diff::working_diff(&repo, "tracked.txt", false, &DiffOptions::default()).unwrap();
    let unstaged_text: String = unstaged
        .iter()
        .flat_map(|h| h.lines.iter())
        .map(|l| l.content.clone())
        .collect();
    assert!(unstaged_text.contains("line 2 changed"));
    assert!(!unstaged_text.contains("line 14 changed"));
}

#[test]
fn unstage_hunk_on_a_hunk_that_no_longer_matches_returns_hunk_not_found() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\n");
    stage_file(&repo, "tracked.txt").unwrap();
    commit(
        &mut repo,
        "initial commit",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    write_file(dir.path(), "tracked.txt", "line one changed\n");
    stage_file(&repo, "tracked.txt").unwrap();

    let result = unstage_hunk(&repo, "tracked.txt", 999, 999, 3);

    assert!(matches!(
        result,
        Err(git_core::stage::StageError::HunkNotFound)
    ));
}

#[test]
fn discard_hunk_reverts_only_the_targeted_hunk_in_the_workdir() {
    let (dir, mut repo) = init_repo();
    let original: String = (1..=15).map(|n| format!("line {n}\n")).collect();
    write_file(dir.path(), "tracked.txt", &original);
    stage_file(&repo, "tracked.txt").unwrap();
    commit(
        &mut repo,
        "initial commit",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let mut lines: Vec<String> = (1..=15).map(|n| format!("line {n}")).collect();
    lines[1] = "line 2 changed".to_string();
    lines[13] = "line 14 changed".to_string();
    let changed = lines.join("\n") + "\n";
    write_file(dir.path(), "tracked.txt", &changed);

    let hunks =
        git_core::diff::working_diff(&repo, "tracked.txt", false, &DiffOptions::default()).unwrap();
    assert_eq!(hunks.len(), 2);

    discard_hunk(
        &repo,
        "tracked.txt",
        hunks[0].old_start,
        hunks[0].new_start,
        3,
    )
    .unwrap();

    let on_disk = std::fs::read_to_string(dir.path().join("tracked.txt")).unwrap();
    assert!(
        on_disk.contains("line 2\n"),
        "discarded hunk's line should be back to original"
    );
    assert!(!on_disk.contains("line 2 changed"));
    assert!(
        on_disk.contains("line 14 changed"),
        "the other hunk's edit must survive"
    );
}

#[test]
fn discard_hunk_on_a_hunk_that_no_longer_matches_returns_hunk_not_found() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\n");
    stage_file(&repo, "tracked.txt").unwrap();
    commit(
        &mut repo,
        "initial commit",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    write_file(dir.path(), "tracked.txt", "line one changed\n");

    let result = discard_hunk(&repo, "tracked.txt", 999, 999, 3);

    assert!(matches!(
        result,
        Err(git_core::stage::StageError::HunkNotFound)
    ));
}

#[test]
fn commit_creates_a_commit_with_the_given_message_and_staged_content() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "greeting.txt", "hello");
    stage_file(&repo, "greeting.txt").unwrap();

    let oid = commit(
        &mut repo,
        "add greeting",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    assert!(git_core::status::status(&repo).unwrap().is_empty());
    let commit = repo
        .find_commit(git2::Oid::from_str(&oid).unwrap())
        .unwrap();
    assert_eq!(commit.message().unwrap(), "add greeting");
}

#[test]
fn commit_on_a_fresh_repo_creates_a_parentless_first_commit() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "greeting.txt", "hello");
    stage_file(&repo, "greeting.txt").unwrap();

    let oid = commit(
        &mut repo,
        "add greeting",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let commit = repo
        .find_commit(git2::Oid::from_str(&oid).unwrap())
        .unwrap();
    assert_eq!(commit.parent_count(), 0);
}

#[test]
fn commit_without_a_configured_identity_returns_an_error() {
    // `common`'s hermetic-git guard (#111) hides the host's global/system config for the whole
    // test binary, so the host's own `user.name`/`user.email` can't satisfy `repo.signature()`.
    let dir = tempfile::TempDir::new().unwrap();
    let mut repo = git2::Repository::init(dir.path()).unwrap();
    write_file(dir.path(), "greeting.txt", "hello");
    stage_file(&repo, "greeting.txt").unwrap();

    assert!(commit(&mut repo, "msg", git_core::hooks::HookOptions::default()).is_err());
}

#[test]
fn stage_hunk_with_wider_context_stages_the_merged_hunk() {
    let (dir, repo) = common::init_repo();
    let base: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    common::write_file(dir.path(), "f.txt", &base);
    common::commit_all(&repo, "base");
    let changed = base
        .replace("line 2\n", "line 2 changed\n")
        .replace("line 10\n", "line 10 changed\n");
    common::write_file(dir.path(), "f.txt", &changed);

    let five = git_core::diff::DiffOptions {
        ignore_whitespace: false,
        context_lines: 5,
    };
    let hunks = git_core::diff::working_diff(&repo, "f.txt", false, &five).unwrap();
    assert_eq!(hunks.len(), 1);
    git_core::stage::stage_hunk(&repo, "f.txt", hunks[0].old_start, hunks[0].new_start, 5).unwrap();

    let remaining = git_core::diff::working_diff(&repo, "f.txt", false, &five).unwrap();
    assert!(remaining.is_empty(), "both edits staged in one go");
}

#[cfg(unix)]
#[derive(Default)]
struct SigningRecorder {
    started: Vec<String>,
    finished: Vec<String>,
}

#[cfg(unix)]
impl git_core::hooks::HookReporter for SigningRecorder {
    fn started(&mut self, hook: &str) {
        self.started.push(hook.to_string());
    }
    fn finished(
        &mut self,
        hook: &str,
        _result: &Result<git_core::hooks::HookOutcome, git_core::hooks::HookError>,
    ) {
        self.finished.push(hook.to_string());
    }
}

#[cfg(unix)]
fn hung_signer_repo() -> (tempfile::TempDir, git2::Repository, tempfile::TempDir) {
    use std::os::unix::fs::PermissionsExt;
    let bin = tempfile::TempDir::new().unwrap();
    let signer = bin.path().join("gpg");
    std::fs::write(&signer, "#!/bin/sh\nsleep 30\n").unwrap();
    std::fs::set_permissions(&signer, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "a.txt", "b\n");
    git_core::stage::stage_file(&repo, "a.txt").unwrap();
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config
        .set_str("gpg.program", signer.to_str().unwrap())
        .unwrap();
    (dir, repo, bin)
}

#[test]
#[cfg(unix)]
fn commit_reports_signing_to_the_hook_reporter_and_honours_cancel() {
    let (_dir, mut repo, _bin) = hung_signer_repo();
    let head = repo.head().unwrap().target().unwrap();
    let started = std::time::Instant::now();
    let cancel = || started.elapsed() > std::time::Duration::from_millis(200);
    let mut recorder = SigningRecorder::default();

    let result = git_core::commit::commit(
        &mut repo,
        "signed",
        git_core::hooks::HookOptions {
            cancel: Some(&cancel),
            reporter: Some(&mut recorder),
            ..Default::default()
        },
    );

    assert_eq!(result.unwrap_err().to_string(), "Signing was cancelled.");
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(recorder.started, vec!["signing".to_string()]);
    assert_eq!(recorder.finished, vec!["signing".to_string()]);
    assert_eq!(repo.head().unwrap().target().unwrap(), head);
}

#[test]
#[cfg(unix)]
fn amend_reports_signing_and_honours_cancel() {
    let (_dir, repo, _bin) = hung_signer_repo();
    let started = std::time::Instant::now();
    let cancel = || started.elapsed() > std::time::Duration::from_millis(200);
    let mut recorder = SigningRecorder::default();

    let result = git_core::commit::amend_head(
        &repo,
        "amended",
        None,
        git_core::hooks::HookOptions {
            cancel: Some(&cancel),
            reporter: Some(&mut recorder),
            ..Default::default()
        },
    );

    assert_eq!(result.unwrap_err().to_string(), "Signing was cancelled.");
    assert_eq!(recorder.started, vec!["signing".to_string()]);
}

#[test]
fn an_unsigned_commit_reports_no_signing() {
    let (dir, mut repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    git_core::stage::stage_file(&repo, "a.txt").unwrap();
    let mut recorder = Recorderless::default();
    git_core::commit::commit(
        &mut repo,
        "plain",
        git_core::hooks::HookOptions {
            reporter: Some(&mut recorder),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!recorder.0.iter().any(|hook| hook == "signing"));
}

#[derive(Default)]
struct Recorderless(Vec<String>);

impl git_core::hooks::HookReporter for Recorderless {
    fn started(&mut self, hook: &str) {
        self.0.push(hook.to_string());
    }
    fn finished(
        &mut self,
        _hook: &str,
        _result: &Result<git_core::hooks::HookOutcome, git_core::hooks::HookError>,
    ) {
    }
}

#[test]
fn unstaging_a_staged_rename_unstages_both_paths() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "old.txt", "one\ntwo\nthree\n");
    common::commit_all(&repo, "base");
    std::fs::rename(dir.path().join("old.txt"), dir.path().join("new.txt")).unwrap();
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new("old.txt")).unwrap();
    index.add_path(std::path::Path::new("new.txt")).unwrap();
    index.write().unwrap();

    unstage_file(&repo, "new.txt").unwrap();

    let mut entries = git_core::status::status(&repo).unwrap();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    assert!(entries.iter().all(|entry| !entry.staged), "{entries:?}");
    assert_eq!(
        entries
            .iter()
            .map(|entry| (entry.path.as_str(), entry.kind))
            .collect::<Vec<_>>(),
        vec![
            ("new.txt", StatusKind::New),
            ("old.txt", StatusKind::Deleted)
        ]
    );
}

#[test]
#[cfg(unix)]
fn staging_a_file_below_a_symlinked_folder_stages_its_deletion() {
    let out = common::outside_dir();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "d/f", "inner\n");
    common::commit_all(&repo, "base");
    common::replace_with_link(dir.path(), "d", out.path());

    stage_file(&repo, "d/f").unwrap();

    // git sees no file at `d/f` (it is beyond the link): staging it stages its deletion, and
    // nothing is read through the link (#157).
    let index = repo.index().unwrap();
    assert!(index.get_path(std::path::Path::new("d/f"), 0).is_none());
    assert!(index.get_path(std::path::Path::new("d"), 0).is_none());
    let summary: Vec<_> = git_core::status::status(&repo)
        .unwrap()
        .into_iter()
        .map(|entry| (entry.path, entry.staged, entry.kind))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("d".to_string(), false, StatusKind::New),
            ("d/f".to_string(), true, StatusKind::Deleted),
        ]
    );
    common::assert_outside_untouched(out.path());
}

#[test]
#[cfg(unix)]
fn staging_a_dangling_symlink_stages_the_link() {
    let (dir, repo) = init_repo();
    std::os::unix::fs::symlink("nowhere", dir.path().join("l")).unwrap();

    stage_file(&repo, "l").unwrap();

    let entry = repo
        .index()
        .unwrap()
        .get_path(std::path::Path::new("l"), 0)
        .unwrap();
    assert_eq!(entry.mode, 0o120000);
}

#[test]
fn staging_rejects_a_path_outside_the_repository() {
    let (_dir, repo) = init_repo();
    assert!(stage_file(&repo, "../x").is_err());
}
