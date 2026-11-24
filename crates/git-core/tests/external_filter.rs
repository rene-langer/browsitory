//! External filter drivers (#1063): libgit2 runs only its built-in filters, so a path whose
//! `filter` attribute names a configured driver (git-crypt, Git LFS) would be staged or checked
//! out raw. Every operation that would pass such a path through a filter is refused before
//! anything is written. The driver commands here would fail if run; no test runs them.
mod common;

use std::path::Path;

use common::{commit_all, init_repo, write_file};
use git2::{Oid, Repository, RepositoryState};
use git_core::diff::{working_diff, DiffOptions};
use git_core::hooks::HookOptions;
use git_core::merge::{ConflictSide, MergeOptions, MergeOutcome};
use git_core::rebase::{RebaseAction, RebasePlanEntry};
use git_core::reset::ResetMode;
use git_core::stash::StashOptions;
use tempfile::TempDir;

const SECRET: &str = "secret.env";
const PLAIN: &str = "plain.txt";

/// `secret.env` (filtered) and `plain.txt` committed on `main`; no driver configured yet.
fn fixture() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(
        dir.path(),
        ".gitattributes",
        "secret.env filter=crypt\nnew/** filter=crypt\nlink filter=crypt\n",
    );
    write_file(dir.path(), SECRET, "v1\n");
    write_file(dir.path(), PLAIN, "p1\n");
    commit_all(&repo, "base");
    (dir, repo)
}

/// What git-crypt's `git-crypt init` writes, with commands that fail if anything runs them.
fn configure_driver(repo: &Repository) {
    let mut config = repo.config().unwrap();
    config.set_str("filter.crypt.clean", "false").unwrap();
    config.set_str("filter.crypt.smudge", "false").unwrap();
}

fn head_commit(repo: &Repository) -> Oid {
    repo.head().unwrap().peel_to_commit().unwrap().id()
}

/// Commits `files` (top-level paths) onto `parent`'s tree on `branch`, without touching the
/// work tree or the index.
fn commit_on(repo: &Repository, branch: &str, parent: Oid, files: &[(&str, &str)]) -> Oid {
    let parent = repo.find_commit(parent).unwrap();
    let mut builder = repo.treebuilder(Some(&parent.tree().unwrap())).unwrap();
    for (path, content) in files {
        let blob = repo.blob(content.as_bytes()).unwrap();
        builder.insert(path, blob, 0o100644).unwrap();
    }
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let signature = repo.signature().unwrap();
    repo.commit(
        Some(&format!("refs/heads/{branch}")),
        &signature,
        &signature,
        &format!("change on {branch}"),
        &tree,
        &[&parent],
    )
    .unwrap()
}

#[derive(Debug, PartialEq, Eq)]
struct State {
    head: (Option<String>, Option<Oid>),
    index: Vec<(String, Oid, u32, i32)>,
    secret: Option<Vec<u8>>,
    plain: Option<Vec<u8>>,
    stashes: usize,
    state: RepositoryState,
}

fn state(dir: &Path, repo: &Repository) -> State {
    let head = repo.head().unwrap();
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    let entries = index
        .iter()
        .map(|entry| {
            (
                String::from_utf8_lossy(&entry.path).into_owned(),
                entry.id,
                entry.mode,
                i32::from((entry.flags >> 12) & 0x3),
            )
        })
        .collect();
    State {
        head: (head.name().ok().map(String::from), head.target()),
        index: entries,
        secret: std::fs::read(dir.join(SECRET)).ok(),
        plain: std::fs::read(dir.join(PLAIN)).ok(),
        stashes: repo.reflog("refs/stash").map(|log| log.len()).unwrap_or(0),
        state: repo.state(),
    }
}

/// The refusal names the path and the driver.
fn assert_refused<T: std::fmt::Debug, E: std::fmt::Display + std::fmt::Debug>(
    result: Result<T, E>,
    path: &str,
) {
    let error = result.expect_err("the operation should be refused");
    let message = error.to_string();
    assert!(
        message.contains(&format!("'{path}'")) && message.contains("'crypt'"),
        "{message} ({error:?})"
    );
}

fn hunk_options() -> DiffOptions {
    DiffOptions {
        ignore_whitespace: false,
        context_lines: 3,
        algorithm: git_core::diff::DiffAlgorithm::Default,
        max_file_bytes: None,
    }
}

// --- Clean direction: work tree to the object database ---

#[test]
fn stage_file_refuses_a_filtered_path() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), SECRET, "plaintext\n");
    let before = state(dir.path(), &repo);

    assert_refused(git_core::stage::stage_file(&repo, SECRET), SECRET);

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn stage_paths_refuses_all_when_one_path_is_filtered() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), PLAIN, "p2\n");
    write_file(dir.path(), SECRET, "plaintext\n");
    let before = state(dir.path(), &repo);

    let result = git_core::stage::stage_paths(&repo, &[PLAIN.into(), SECRET.into()]);

    assert_refused(result, SECRET);
    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn staging_an_untracked_folder_refuses_a_filtered_file_in_it() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), "new/key.env", "plaintext\n");
    let before = state(dir.path(), &repo);

    assert_refused(git_core::stage::stage_file(&repo, "new/"), "new/key.env");

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn stage_hunk_refuses_a_filtered_path() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), SECRET, "v1\nplaintext\n");
    let hunks = working_diff(&repo, SECRET, false, &hunk_options()).unwrap();
    let before = state(dir.path(), &repo);

    let result = git_core::stage::stage_hunk(
        &repo,
        SECRET,
        hunks[0].old_start,
        hunks[0].new_start,
        common::hunk_ctx(3),
    );

    assert_refused(result, SECRET);
    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn stage_lines_refuses_a_filtered_path() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), SECRET, "v1\nplaintext\n");
    let hunks = working_diff(&repo, SECRET, false, &hunk_options()).unwrap();
    let shown: Vec<String> = hunks[0].lines.iter().map(|l| l.content.clone()).collect();
    let before = state(dir.path(), &repo);

    let result = git_core::stage::stage_lines(
        &repo,
        SECRET,
        hunks[0].old_start,
        hunks[0].new_start,
        common::hunk_ctx(3),
        &[1],
        &shown,
    );

    assert_refused(result, SECRET);
    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn stash_refuses_a_filtered_change() {
    let (dir, mut repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), SECRET, "plaintext\n");
    let before = state(dir.path(), &repo);

    let result = git_core::stash::save_stash(&mut repo, &StashOptions::default());

    assert_refused(result, SECRET);
    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn stash_of_a_filtered_path_is_refused() {
    let (dir, mut repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), SECRET, "plaintext\n");
    let before = state(dir.path(), &repo);
    let options = StashOptions {
        paths: vec![SECRET.into()],
        ..StashOptions::default()
    };

    let result = git_core::stash::save_stash(&mut repo, &options);

    assert_refused(result, SECRET);
    assert_eq!(state(dir.path(), &repo), before);
}

/// A conflict on `secret.env` from merging `other`, made before the driver is configured.
fn conflicted() -> (TempDir, Repository) {
    let (dir, repo) = fixture();
    let base = head_commit(&repo);
    commit_on(&repo, "other", base, &[(SECRET, "other\n")]);
    write_file(dir.path(), SECRET, "main\n");
    commit_all(&repo, "main side");
    let outcome = git_core::merge::start_merge(&repo, "other", MergeOptions::default()).unwrap();
    assert!(
        matches!(outcome, MergeOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    configure_driver(&repo);
    (dir, repo)
}

#[test]
fn mark_resolved_refuses_a_filtered_path() {
    let (dir, repo) = conflicted();
    write_file(dir.path(), SECRET, "resolved\n");
    let before = state(dir.path(), &repo);

    assert_refused(git_core::merge::mark_resolved(&repo, SECRET, false), SECRET);

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn resolving_with_one_side_refuses_a_filtered_path() {
    let (dir, repo) = conflicted();
    let before = state(dir.path(), &repo);

    assert_refused(
        git_core::merge::resolve_whole_file(&repo, SECRET, ConflictSide::Theirs),
        SECRET,
    );

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn a_driver_that_is_only_required_is_refused_too() {
    let (dir, repo) = fixture();
    repo.config()
        .unwrap()
        .set_bool("filter.crypt.required", true)
        .unwrap();
    write_file(dir.path(), SECRET, "plaintext\n");
    let before = state(dir.path(), &repo);

    assert_refused(git_core::stage::stage_file(&repo, SECRET), SECRET);

    assert_eq!(state(dir.path(), &repo), before);
}

// --- Smudge direction: the object database to the work tree ---

/// `other` changes `secret.env` on top of `main`.
fn with_other_branch() -> (TempDir, Repository, Oid) {
    let (dir, repo) = fixture();
    let other = commit_on(&repo, "other", head_commit(&repo), &[(SECRET, "v2\n")]);
    configure_driver(&repo);
    (dir, repo, other)
}

#[test]
fn switch_refuses_to_write_a_filtered_file() {
    let (dir, repo, _) = with_other_branch();
    let before = state(dir.path(), &repo);

    assert_refused(git_core::branch::switch_branch(&repo, "other"), SECRET);

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn checkout_commit_refuses_to_write_a_filtered_file() {
    let (dir, repo, other) = with_other_branch();
    let before = state(dir.path(), &repo);

    assert_refused(
        git_core::branch::checkout_commit(&repo, &other.to_string()),
        SECRET,
    );

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn fast_forward_merge_refuses_to_write_a_filtered_file() {
    let (dir, repo, _) = with_other_branch();
    let before = state(dir.path(), &repo);

    assert_refused(
        git_core::merge::start_merge(&repo, "other", MergeOptions::default()),
        SECRET,
    );

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn merge_refuses_to_write_a_filtered_file() {
    let (dir, repo, _) = with_other_branch();
    write_file(dir.path(), PLAIN, "p2\n");
    commit_all(&repo, "main side");
    let before = state(dir.path(), &repo);

    assert_refused(
        git_core::merge::start_merge(&repo, "other", MergeOptions::default()),
        SECRET,
    );

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn cherry_pick_refuses_to_write_a_filtered_file() {
    let (dir, repo, other) = with_other_branch();
    let before = state(dir.path(), &repo);

    let result =
        git_core::pick::cherry_pick(&repo, &other.to_string(), None, HookOptions::default());

    assert_refused(result, SECRET);
    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn revert_refuses_to_write_a_filtered_file() {
    let (dir, repo) = fixture();
    write_file(dir.path(), SECRET, "v2\n");
    commit_all(&repo, "change the secret");
    let changed = head_commit(&repo);
    configure_driver(&repo);
    let before = state(dir.path(), &repo);

    let result = git_core::pick::revert(&repo, &changed.to_string(), None, HookOptions::default());

    assert_refused(result, SECRET);
    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn rebase_refuses_to_write_a_filtered_file() {
    let (dir, repo) = fixture();
    let base = head_commit(&repo);
    let onto = commit_on(&repo, "onto", base, &[(PLAIN, "p2\n")]);
    write_file(dir.path(), SECRET, "v2\n");
    commit_all(&repo, "change the secret");
    let changed = head_commit(&repo);
    configure_driver(&repo);
    let before = state(dir.path(), &repo);

    let plan = vec![RebasePlanEntry {
        commit_id: changed.to_string(),
        action: RebaseAction::Pick,
        combined_message: None,
    }];
    let result = git_core::rebase::start_rebase(&repo, &onto.to_string(), plan);

    assert_refused(result, SECRET);
    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn hard_reset_refuses_to_write_a_filtered_file() {
    let (dir, repo) = fixture();
    let base = head_commit(&repo);
    write_file(dir.path(), SECRET, "v2\n");
    commit_all(&repo, "change the secret");
    configure_driver(&repo);
    let before = state(dir.path(), &repo);

    assert_refused(
        git_core::reset::reset_to(&repo, &base.to_string(), ResetMode::Hard),
        SECRET,
    );

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn discard_refuses_to_write_a_filtered_file() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), SECRET, "plaintext\n");
    let before = state(dir.path(), &repo);

    assert_refused(git_core::stage::discard_file(&repo, SECRET), SECRET);

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn discard_hunk_refuses_to_write_a_filtered_file() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), SECRET, "v1\nplaintext\n");
    let hunks = working_diff(&repo, SECRET, false, &hunk_options()).unwrap();
    let before = state(dir.path(), &repo);

    let result = git_core::stage::discard_hunk(
        &repo,
        SECRET,
        hunks[0].old_start,
        hunks[0].new_start,
        common::hunk_ctx(3),
    );

    assert_refused(result, SECRET);
    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn restore_version_refuses_to_write_a_filtered_file() {
    let (dir, repo) = fixture();
    let base = head_commit(&repo);
    write_file(dir.path(), SECRET, "v2\n");
    commit_all(&repo, "change the secret");
    configure_driver(&repo);
    let before = state(dir.path(), &repo);

    assert_refused(
        git_core::stage::restore_file_version(&repo, &base.to_string(), SECRET),
        SECRET,
    );

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn stash_apply_refuses_to_write_a_filtered_file() {
    let (dir, mut repo) = fixture();
    write_file(dir.path(), SECRET, "stashed\n");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    configure_driver(&repo);
    let before = state(dir.path(), &repo);

    assert_refused(
        git_core::stash::apply_stash(&mut repo, 0, None, false),
        SECRET,
    );

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn patch_apply_refuses_to_write_a_filtered_file() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    let patch = "diff --git a/secret.env b/secret.env\n\
                 --- a/secret.env\n\
                 +++ b/secret.env\n\
                 @@ -1 +1 @@\n\
                 -v1\n\
                 +v3\n";
    let before = state(dir.path(), &repo);

    assert_refused(
        git_core::patch::apply_patch(&repo, patch.as_bytes()),
        SECRET,
    );

    assert_eq!(state(dir.path(), &repo), before);
}

// --- Unaffected ---

#[test]
fn paths_without_the_filter_still_stage_switch_and_discard() {
    let (dir, repo) = fixture();
    commit_on(&repo, "same", head_commit(&repo), &[(PLAIN, "p3\n")]);
    configure_driver(&repo);

    write_file(dir.path(), PLAIN, "p2\n");
    git_core::stage::stage_file(&repo, PLAIN).unwrap();
    git_core::stage::discard_file(&repo, PLAIN).unwrap();
    git_core::reset::reset_to(&repo, &head_commit(&repo).to_string(), ResetMode::Hard).unwrap();
    git_core::branch::switch_branch(&repo, "same").unwrap();

    assert_eq!(
        repo.head().unwrap().name().ok(),
        Some("refs/heads/same"),
        "the switch happened"
    );
    assert_eq!(std::fs::read(dir.path().join(PLAIN)).unwrap(), b"p3\n");
    assert_eq!(std::fs::read(dir.path().join(SECRET)).unwrap(), b"v1\n");
}

#[test]
fn a_filter_attribute_without_driver_config_is_ignored_as_git_does() {
    let (dir, repo) = fixture();
    write_file(dir.path(), SECRET, "v2\n");

    git_core::stage::stage_file(&repo, SECRET).unwrap();

    let index = repo.index().unwrap();
    let entry = index.get_path(Path::new(SECRET), 0).unwrap();
    assert_eq!(repo.find_blob(entry.id).unwrap().content(), b"v2\n");
}

#[test]
fn an_empty_clean_command_is_no_driver() {
    let (dir, repo) = fixture();
    repo.config()
        .unwrap()
        .set_str("filter.crypt.clean", "")
        .unwrap();
    write_file(dir.path(), SECRET, "v2\n");

    git_core::stage::stage_file(&repo, SECRET).unwrap();

    let index = repo.index().unwrap();
    let entry = index.get_path(Path::new(SECRET), 0).unwrap();
    assert_eq!(repo.find_blob(entry.id).unwrap().content(), b"v2\n");
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_the_filter_names_still_stages() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    std::os::unix::fs::symlink(PLAIN, dir.path().join("link")).unwrap();

    git_core::stage::stage_file(&repo, "link").unwrap();

    let index = repo.index().unwrap();
    let entry = index.get_path(Path::new("link"), 0).unwrap();
    assert_eq!(entry.mode, 0o120000);
}

// --- Mutation testing follow-ups ---

#[test]
fn applying_a_stash_with_its_index_refuses_a_filtered_staged_change() {
    let (dir, mut repo) = fixture();
    // Staged `v2`, work tree back to `v1`: only the stash's index tree changes the file.
    write_file(dir.path(), SECRET, "v2\n");
    git_core::stage::stage_file(&repo, SECRET).unwrap();
    write_file(dir.path(), SECRET, "v1\n");
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    configure_driver(&repo);
    let before = state(dir.path(), &repo);

    assert_refused(
        git_core::stash::apply_stash(&mut repo, 0, None, true),
        SECRET,
    );

    assert_eq!(state(dir.path(), &repo), before);
}

#[test]
fn aborting_a_merge_refuses_before_removing_what_it_added() {
    let (dir, repo) = fixture();
    let base = head_commit(&repo);
    commit_on(
        &repo,
        "other",
        base,
        &[(SECRET, "other\n"), ("added.txt", "added\n")],
    );
    write_file(dir.path(), SECRET, "main\n");
    commit_all(&repo, "main side");
    let outcome = git_core::merge::start_merge(&repo, "other", MergeOptions::default()).unwrap();
    assert!(
        matches!(outcome, MergeOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    configure_driver(&repo);
    let before = state(dir.path(), &repo);

    assert_refused(git_core::merge::abort_merge(&repo), SECRET);

    assert_eq!(state(dir.path(), &repo), before);
    assert_eq!(
        std::fs::read(dir.path().join("added.txt")).unwrap(),
        b"added\n"
    );
}

#[test]
fn a_driver_name_that_is_not_utf8_is_refused_too() {
    let (dir, repo) = fixture();
    let mut attributes = b"secret.env filter=crypt\xff\n".to_vec();
    attributes.extend_from_slice(b"new/** filter=crypt\n");
    std::fs::write(dir.path().join(".gitattributes"), attributes).unwrap();
    let config_path = repo.path().join("config");
    let mut config = std::fs::read(&config_path).unwrap();
    config.extend_from_slice(b"[filter \"crypt\xff\"]\n\tclean = false\n");
    std::fs::write(&config_path, config).unwrap();
    write_file(dir.path(), SECRET, "plaintext\n");
    let before = state(dir.path(), &repo);

    let error = git_core::stage::stage_file(&repo, SECRET).unwrap_err();

    assert!(
        error.to_string().contains(&format!("'{SECRET}'")),
        "{error}"
    );
    assert_eq!(state(dir.path(), &repo), before);
}
