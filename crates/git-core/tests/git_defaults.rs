//! Git's own defaults the app follows (#652, #653, #656, #689): the pull strategy from config,
//! switching branches over local changes, creating a branch without switching, and a missing
//! commit identity.
mod common;

#[cfg(unix)]
use common::install_hook;
use common::{commit_all, init_repo, write_file};
use git2::Repository;
use git_core::branch::{
    create_branch, create_branch_only, stash_and_switch, switch_branch, switch_conflicts,
    BranchError, StashSwitchOutcome,
};
use git_core::commit::CommitError;
use git_core::hooks::HookOptions;
use git_core::remote::{pull_defaults, strategy_from_pull_rebase, PullStrategy};

fn current_branch(repo: &Repository) -> String {
    repo.head().unwrap().shorthand().unwrap().to_string()
}

fn read(dir: &std::path::Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

/// `main` and `other`, where `other` changes `file.txt`; HEAD on `main`.
fn two_branches() -> (tempfile::TempDir, Repository, String) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "base\n");
    write_file(dir.path(), "keep.txt", "keep\n");
    commit_all(&repo, "base");
    let main = current_branch(&repo);
    create_branch(&repo, "other", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "other\n");
    commit_all(&repo, "other");
    switch_branch(&repo, &main).unwrap();
    (dir, repo, main)
}

// --- pull strategy (#652) ---

#[test]
fn pull_strategy_is_none_without_config() {
    let (_dir, repo) = init_repo();
    assert_eq!(pull_defaults(&repo), None);
}

#[test]
fn pull_rebase_true_is_a_rebase_and_false_a_merge() {
    let (_dir, repo) = init_repo();
    repo.config()
        .unwrap()
        .set_str("pull.rebase", "true")
        .unwrap();
    assert_eq!(pull_defaults(&repo), Some(PullStrategy::Rebase));
    repo.config()
        .unwrap()
        .set_str("pull.rebase", "false")
        .unwrap();
    assert_eq!(pull_defaults(&repo), Some(PullStrategy::Merge));
}

#[test]
fn pull_ff_only_is_fast_forward_only_unless_pull_rebase_says_otherwise() {
    let (_dir, repo) = init_repo();
    repo.config().unwrap().set_str("pull.ff", "only").unwrap();
    assert_eq!(pull_defaults(&repo), Some(PullStrategy::FastForwardOnly));
    repo.config()
        .unwrap()
        .set_str("pull.rebase", "merges")
        .unwrap();
    assert_eq!(pull_defaults(&repo), Some(PullStrategy::Rebase));
}

#[test]
fn pull_ff_other_than_only_sets_no_strategy() {
    let (_dir, repo) = init_repo();
    repo.config().unwrap().set_str("pull.ff", "false").unwrap();
    assert_eq!(pull_defaults(&repo), None);
}

#[test]
fn the_current_branchs_rebase_setting_wins_over_pull_rebase() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "a");
    let branch = current_branch(&repo);
    let mut config = repo.config().unwrap();
    config.set_str("pull.rebase", "true").unwrap();
    config
        .set_str(&format!("branch.{branch}.rebase"), "false")
        .unwrap();
    assert_eq!(pull_defaults(&repo), Some(PullStrategy::Merge));
}

#[test]
fn pull_rebase_values_map_like_git() {
    for value in [
        "true",
        "YES",
        "on",
        "1",
        "merges",
        "m",
        "interactive",
        "i",
        " true ",
    ] {
        assert_eq!(
            strategy_from_pull_rebase(value),
            Some(PullStrategy::Rebase),
            "{value}"
        );
    }
    for value in ["false", "no", "off", "0"] {
        assert_eq!(
            strategy_from_pull_rebase(value),
            Some(PullStrategy::Merge),
            "{value}"
        );
    }
    assert_eq!(strategy_from_pull_rebase("sometimes"), None);
}

// --- switching over local changes (#653) ---

#[test]
fn a_switch_over_conflicting_local_changes_is_local_changes_and_names_the_files() {
    let (dir, repo, _main) = two_branches();
    write_file(dir.path(), "file.txt", "mine\n");
    write_file(dir.path(), "keep.txt", "edited but not in the way\n");

    let error = switch_branch(&repo, "other").unwrap_err();

    assert!(
        matches!(error, BranchError::LocalChanges(ref name) if name == "other"),
        "{error}"
    );
    assert_eq!(switch_conflicts(&repo, "other").unwrap(), vec!["file.txt"]);
    // The dry run wrote nothing.
    assert_eq!(read(dir.path(), "file.txt"), "mine\n");
}

#[test]
fn switch_conflicts_is_empty_when_the_switch_would_go_through() {
    let (dir, repo, _main) = two_branches();
    write_file(dir.path(), "keep.txt", "edited\n");
    assert!(switch_conflicts(&repo, "other").unwrap().is_empty());
}

#[test]
fn an_untracked_file_in_the_way_is_named_too() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "base\n");
    commit_all(&repo, "base");
    let main = current_branch(&repo);
    create_branch(&repo, "other", "HEAD").unwrap();
    write_file(dir.path(), "new.txt", "theirs\n");
    commit_all(&repo, "add new.txt");
    switch_branch(&repo, &main).unwrap();
    write_file(dir.path(), "new.txt", "mine, untracked\n");

    assert!(matches!(
        switch_branch(&repo, "other"),
        Err(BranchError::LocalChanges(_))
    ));
    assert_eq!(switch_conflicts(&repo, "other").unwrap(), vec!["new.txt"]);
}

#[test]
fn stash_and_switch_leaves_the_changes_in_the_stash() {
    let (dir, mut repo, main) = two_branches();
    write_file(dir.path(), "file.txt", "mine\n");
    write_file(dir.path(), "untracked.txt", "new\n");

    let outcome = stash_and_switch(&mut repo, "other", false).unwrap();

    assert_eq!(outcome, StashSwitchOutcome::Stashed);
    assert_eq!(current_branch(&repo), "other");
    assert_eq!(read(dir.path(), "file.txt"), "other\n");
    assert!(!dir.path().join("untracked.txt").exists());
    let stashes = git_core::stash::list_stashes(&mut repo).unwrap();
    assert_eq!(stashes.len(), 1);
    assert!(
        stashes[0]
            .message
            .contains(&format!("Changes on {main} before switching to other")),
        "{}",
        stashes[0].message
    );
}

#[test]
fn stash_and_switch_reapplies_changes_that_fit_the_new_branch() {
    let (dir, mut repo, _main) = two_branches();
    write_file(dir.path(), "keep.txt", "edited\n");
    write_file(dir.path(), "untracked.txt", "new\n");

    let outcome = stash_and_switch(&mut repo, "other", true).unwrap();

    assert_eq!(outcome, StashSwitchOutcome::Reapplied);
    assert_eq!(current_branch(&repo), "other");
    assert_eq!(read(dir.path(), "keep.txt"), "edited\n");
    assert_eq!(read(dir.path(), "untracked.txt"), "new\n");
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
}

#[test]
fn stash_and_switch_keeps_the_stash_when_reapplying_conflicts() {
    let (dir, mut repo, _main) = two_branches();
    write_file(dir.path(), "file.txt", "mine\n");

    let outcome = stash_and_switch(&mut repo, "other", true).unwrap();

    assert_eq!(
        outcome,
        StashSwitchOutcome::Conflicted {
            files: vec!["file.txt".to_string()]
        }
    );
    assert_eq!(current_branch(&repo), "other");
    assert_eq!(git_core::stash::list_stashes(&mut repo).unwrap().len(), 1);
    assert!(read(dir.path(), "file.txt").contains("<<<<<<<"));
}

#[test]
fn stash_and_switch_to_a_missing_branch_stashes_nothing() {
    let (dir, mut repo, main) = two_branches();
    write_file(dir.path(), "file.txt", "mine\n");

    assert!(stash_and_switch(&mut repo, "missing", true).is_err());

    assert_eq!(current_branch(&repo), main);
    assert_eq!(read(dir.path(), "file.txt"), "mine\n");
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
}

// Needs a case-sensitive drive to commit both names of the case pair.
#[cfg(target_os = "linux")]
#[test]
fn a_switch_refused_after_stashing_puts_the_changes_back() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "base.txt", "base\n");
    commit_all(&repo, "base");
    let main = current_branch(&repo);
    // `other` brings in a case pair while `core.ignorecase` is on: the switch is refused only
    // once the changes are stashed.
    create_branch(&repo, "other", "HEAD").unwrap();
    write_file(dir.path(), "A.txt", "upper\n");
    write_file(dir.path(), "a.txt", "lower\n");
    commit_all(&repo, "case pair");
    switch_branch(&repo, &main).unwrap();
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    write_file(dir.path(), "base.txt", "mine\n");

    assert!(stash_and_switch(&mut repo, "other", true).is_err());

    assert_eq!(current_branch(&repo), main);
    assert_eq!(read(dir.path(), "base.txt"), "mine\n");
    assert!(git_core::stash::list_stashes(&mut repo).unwrap().is_empty());
}

// --- creating a branch without switching (#656) ---

#[test]
fn create_branch_only_leaves_head_where_it_was() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "a");
    let main = current_branch(&repo);

    create_branch_only(&repo, "feature", "HEAD").unwrap();

    assert_eq!(current_branch(&repo), main);
    let head = repo.head().unwrap().target().unwrap();
    let feature = repo
        .find_branch("feature", git2::BranchType::Local)
        .unwrap()
        .get()
        .target()
        .unwrap();
    assert_eq!(feature, head);
}

// --- commit identity (#689) ---

fn repo_without_identity() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let hooks = repo.path().join("hooks");
    repo.config()
        .unwrap()
        .set_str("core.hooksPath", hooks.to_str().unwrap())
        .unwrap();
    (dir, repo)
}

#[cfg(unix)]
#[test]
fn a_commit_without_an_identity_is_missing_identity_before_any_hook_runs() {
    let (dir, mut repo) = repo_without_identity();
    let marker = dir.path().join("hook-ran");
    install_hook(
        &repo.path().join("hooks"),
        "pre-commit",
        &format!("touch '{}'", marker.display()),
    );
    write_file(dir.path(), "a.txt", "a\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    let error = git_core::commit::commit(&mut repo, "first", HookOptions::default()).unwrap_err();

    assert!(matches!(error, CommitError::MissingIdentity), "{error}");
    assert!(!marker.exists());
    assert!(repo.head().is_err(), "nothing was committed");
}

#[test]
fn an_amend_without_an_identity_is_missing_identity() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "a");
    let mut config = repo
        .config()
        .unwrap()
        .open_level(git2::ConfigLevel::Local)
        .unwrap();
    config.remove("user.name").unwrap();

    let error =
        git_core::commit::amend_head(&repo, "reworded", None, HookOptions::default()).unwrap_err();

    assert!(matches!(error, CommitError::MissingIdentity), "{error}");
}

#[test]
fn a_malformed_identity_reports_git_s_error_not_a_missing_one() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "a");
    let mut config = repo
        .config()
        .unwrap()
        .open_level(git2::ConfigLevel::Local)
        .unwrap();
    config.set_str("user.name", "Ada <x>").unwrap();

    let error =
        git_core::commit::amend_head(&repo, "reworded", None, HookOptions::default()).unwrap_err();

    // Set, but not usable: git's own reason, not the "set your name and email" prompt.
    assert!(!matches!(error, CommitError::MissingIdentity), "{error}");
}

#[test]
fn set_identity_writes_the_repository_config_and_the_commit_then_goes_through() {
    let (dir, mut repo) = repo_without_identity();
    write_file(dir.path(), "a.txt", "a\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    let mut config = git_core::commit::identity_config(&repo, false).unwrap();
    git_core::commit::set_identity(&mut config, "  Ada Lovelace ", "ada@example.com").unwrap();
    git_core::commit::commit(&mut repo, "first", HookOptions::default()).unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.author().name().unwrap(), "Ada Lovelace");
    assert_eq!(head.author().email().unwrap(), "ada@example.com");
}

#[test]
fn set_identity_refuses_a_blank_or_malformed_identity() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("gitconfig");
    std::fs::write(&path, "").unwrap();
    let mut config = git2::Config::open(&path).unwrap();
    for (name, email) in [
        ("", "a@example.com"),
        ("Ada", " "),
        ("Ada <x>", "a@example.com"),
        ("Ada", "a b@example.com"),
        ("Ada\n", "a@example.com\nx"),
        // A control character that isn't whitespace, so only the control check refuses it.
        ("A\u{7}da", "a@example.com"),
    ] {
        assert!(
            matches!(
                git_core::commit::set_identity(&mut config, name, email),
                Err(CommitError::InvalidAuthor)
            ),
            "{name:?} {email:?}"
        );
    }
    assert!(config.get_string("user.name").is_err());
}
