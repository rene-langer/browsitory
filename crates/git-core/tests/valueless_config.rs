//! #1180: a setting written without a value (`flag` alone in `.git/config`, boolean true to git)
//! never crashes the process. libgit2 dereferences the missing value when it removes or renames
//! the setting's section (Delete branch, Rename branch, Rename remote, Remove remote), compares
//! `branch.*.remote` with a remote's name (Rename remote, Remove remote) or adds a value next to
//! it (Add remote's `fetch`). #1182: nor does a merge driver (`merge.default`) or conflict style
//! (`merge.conflictstyle`) written without a value, which libgit2 reads at the start of every
//! merge and checkout. Each command runs in a child process (this test binary, running only that
//! test), so a crash fails the test instead of killing the test binary.

mod common;

use std::io::Write as _;
use std::path::Path;

use common::{commit_all, git_ok, init_repo, write_file};
use git2::Repository;
use tempfile::TempDir;

/// The child's repository folder; set only in the child.
const CHILD_REPO: &str = "BROWSITORY_VALUELESS_CONFIG_CHILD_REPO";
/// Marks the child's result line on its stdout.
const RESULT: &str = "valueless-config-result: ";

/// Runs `setup`, then `command` on its repository in a child process: this test binary, running
/// only `test`. Returns the repository and `"ok"` or the command's error text; fails the test
/// when the child crashed. In the child, only `command` runs, on the parent's repository.
fn run_isolated(
    test: &str,
    setup: impl FnOnce() -> (TempDir, Repository),
    command: impl FnOnce(&Repository) -> Result<(), String>,
) -> (TempDir, Repository, String) {
    if let Some(path) = std::env::var_os(CHILD_REPO) {
        let repo = Repository::open(path).expect("open the parent's repository");
        let result = command(&repo).err().unwrap_or_else(|| "ok".to_string());
        // libtest has printed `test <name> ... ` without a line end.
        println!("\n{RESULT}{}", result.replace('\n', " "));
        std::io::stdout().flush().unwrap();
        std::process::exit(0);
    }
    let (dir, repo) = setup();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([test, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_REPO, dir.path())
        .output()
        .expect("run the test binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "the command crashed the process: {}\nstdout: {stdout}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let result = stdout
        .lines()
        .find_map(|line| line.strip_prefix(RESULT))
        .unwrap_or_else(|| panic!("the child printed no result (test name {test:?}?): {stdout}"))
        .to_string();
    (dir, repo, result)
}

fn append_config(repo: &Repository, text: &str) {
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(repo.path().join("config"))
        .unwrap();
    file.write_all(text.as_bytes()).unwrap();
}

fn config_text(repo: &Repository) -> String {
    std::fs::read_to_string(repo.path().join("config")).unwrap()
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// The repository's own settings whose names match `pattern`, as `git config --get-regexp`
/// lists them (empty when there is none).
fn settings(dir: &Path, pattern: &str) -> String {
    let output = test_support::git(dir, &["config", "--local", "--get-regexp", pattern]);
    String::from_utf8(output.stdout).unwrap()
}

fn has_branch(repo: &Repository, name: &str) -> bool {
    repo.find_branch(name, git2::BranchType::Local).is_ok()
}

/// A repository with a commit on `main` and a branch `flagged` at it.
fn repo_with_flagged() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    git_ok(dir.path(), &["branch", "-M", "main"]);
    git_ok(dir.path(), &["branch", "flagged"]);
    (dir, repo)
}

/// [`repo_with_flagged`] with a remote `origin` and its `origin/main`; no branch tracks it.
fn repo_with_origin() -> (TempDir, Repository) {
    let (dir, repo) = repo_with_flagged();
    git_ok(
        dir.path(),
        &["remote", "add", "origin", "https://example.invalid/r.git"],
    );
    git_ok(
        dir.path(),
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    (dir, repo)
}

/// [`repo_with_flagged`] with `text` appended to its config.
fn flagged_with(text: &'static str) -> impl FnOnce() -> (TempDir, Repository) {
    move || {
        let (dir, repo) = repo_with_flagged();
        append_config(&repo, text);
        (dir, repo)
    }
}

/// [`repo_with_origin`] with `text` appended to its config.
fn origin_with(text: &'static str) -> impl FnOnce() -> (TempDir, Repository) {
    move || {
        let (dir, repo) = repo_with_origin();
        append_config(&repo, text);
        (dir, repo)
    }
}

fn delete(name: &'static str) -> impl FnOnce(&Repository) -> Result<(), String> {
    move |repo| git_core::branch::delete_branch(repo, name, true).map_err(|e| e.to_string())
}

fn rename(old: &'static str, new: &'static str) -> impl FnOnce(&Repository) -> Result<(), String> {
    move |repo| git_core::branch::rename_branch(repo, old, new).map_err(|e| e.to_string())
}

/// The section goes whole, as with `git branch -D`; another branch's setting without a value
/// stays as it is.
#[test]
fn delete_branch_removes_a_setting_written_without_a_value() {
    let (dir, repo, result) = run_isolated(
        "delete_branch_removes_a_setting_written_without_a_value",
        || {
            let (dir, repo) = repo_with_flagged();
            git_ok(dir.path(), &["branch", "kept"]);
            git_ok(
                dir.path(),
                &["config", "branch.flagged.description", "notes"],
            );
            append_config(
                &repo,
                "[branch \"flagged\"]\n\tflag\n[branch \"kept\"]\n\tflag\n",
            );
            (dir, repo)
        },
        delete("flagged"),
    );

    assert_eq!(result, "ok");
    assert!(!has_branch(&repo, "flagged"));
    assert_eq!(settings(dir.path(), r"^branch\.flagged\."), "");
    let config = config_text(&repo);
    assert!(config.ends_with("[branch \"kept\"]\n\tflag\n"), "{config}");
}

/// The setting moves with the branch, written as `true` (what git reads it as); the others move
/// unchanged and nothing stays under the old name.
#[test]
fn rename_branch_carries_a_setting_written_without_a_value_as_true() {
    let (dir, repo, result) = run_isolated(
        "rename_branch_carries_a_setting_written_without_a_value_as_true",
        || {
            let (dir, repo) = repo_with_flagged();
            git_ok(
                dir.path(),
                &["config", "branch.flagged.description", "notes"],
            );
            append_config(&repo, "[branch \"flagged\"]\n\tflag\n");
            (dir, repo)
        },
        rename("flagged", "renamed"),
    );
    let d = dir.path();

    assert_eq!(result, "ok");
    assert!(has_branch(&repo, "renamed"));
    assert!(!has_branch(&repo, "flagged"));
    assert_eq!(
        git_out(d, &["config", "--type=bool", "branch.renamed.flag"]),
        "true\n"
    );
    assert_eq!(
        git_out(d, &["config", "branch.renamed.description"]),
        "notes\n"
    );
    assert_eq!(settings(d, r"^branch\.flagged\."), "");
}

/// A setting without a value left under the new name (by a branch deleted with a hand-edited
/// config) is written as `true` too; the moved value joins it.
#[test]
fn rename_branch_onto_a_left_over_setting_written_without_a_value() {
    let (dir, _repo, result) = run_isolated(
        "rename_branch_onto_a_left_over_setting_written_without_a_value",
        || {
            let (dir, repo) = repo_with_flagged();
            git_ok(dir.path(), &["config", "branch.flagged.flag", "yes"]);
            append_config(&repo, "[branch \"renamed\"]\n\tflag\n");
            (dir, repo)
        },
        rename("flagged", "renamed"),
    );
    let d = dir.path();

    assert_eq!(result, "ok");
    let mut values: Vec<String> = git_out(d, &["config", "--get-all", "branch.renamed.flag"])
        .lines()
        .map(str::to_string)
        .collect();
    values.sort();
    assert_eq!(values, ["true", "yes"]);
    assert_eq!(settings(d, r"^branch\.flagged\."), "");
}

const MULTI_VALUED: &str = "The setting 'branch.flagged.flag' has several values, one of them \
     written without a value. Give that one a value or remove it (git config --edit), then try \
     again.";

const FLAG_TWICE: &str = "[branch \"flagged\"]\n\tflag = yes\n\tflag\n";

/// libgit2 can't rewrite a setting with several values when one has none, and Browsitory can't
/// give it one without touching the others: Delete branch refuses and changes nothing.
#[test]
fn delete_branch_refuses_a_setting_with_several_values_one_written_without_a_value() {
    let (_dir, repo, result) = run_isolated(
        "delete_branch_refuses_a_setting_with_several_values_one_written_without_a_value",
        flagged_with(FLAG_TWICE),
        delete("flagged"),
    );

    assert_eq!(result, MULTI_VALUED);
    assert!(has_branch(&repo, "flagged"));
    assert!(config_text(&repo).ends_with(FLAG_TWICE));
}

/// Rename branch refuses before it moves the ref (libgit2 moves the ref first, then the config).
#[test]
fn rename_branch_refuses_a_setting_with_several_values_one_written_without_a_value() {
    let (_dir, repo, result) = run_isolated(
        "rename_branch_refuses_a_setting_with_several_values_one_written_without_a_value",
        flagged_with(FLAG_TWICE),
        rename("flagged", "renamed"),
    );

    assert_eq!(result, MULTI_VALUED);
    assert!(has_branch(&repo, "flagged"));
    assert!(!has_branch(&repo, "renamed"));
    assert!(config_text(&repo).ends_with(FLAG_TWICE));
}

/// A setting in a file the repository's config includes isn't in the file libgit2 rewrites, so it
/// never crashed: Delete branch still works and leaves that file alone.
#[test]
fn delete_branch_leaves_an_included_setting_written_without_a_value_alone() {
    let (_dir, repo, result) = run_isolated(
        "delete_branch_leaves_an_included_setting_written_without_a_value_alone",
        || {
            let (dir, repo) = repo_with_flagged();
            std::fs::write(
                repo.path().join("extra.config"),
                "[branch \"flagged\"]\n\tflag\n",
            )
            .unwrap();
            git_ok(dir.path(), &["config", "include.path", "extra.config"]);
            (dir, repo)
        },
        delete("flagged"),
    );

    assert_eq!(result, "ok");
    assert!(!has_branch(&repo, "flagged"));
    assert_eq!(
        std::fs::read_to_string(repo.path().join("extra.config")).unwrap(),
        "[branch \"flagged\"]\n\tflag\n"
    );
}

/// libgit2 also rewrites the settings of a branch whose name continues the deleted one's after a
/// dot (`release.1` for `release`), so one without a value there crashed it too.
#[test]
fn delete_branch_with_a_dotted_namesake_written_without_a_value() {
    let (_dir, repo, result) = run_isolated(
        "delete_branch_with_a_dotted_namesake_written_without_a_value",
        || {
            let (dir, repo) = repo_with_flagged();
            git_ok(dir.path(), &["branch", "release"]);
            git_ok(dir.path(), &["branch", "release.1"]);
            append_config(&repo, "[branch \"release.1\"]\n\tflag\n");
            (dir, repo)
        },
        delete("release"),
    );

    assert_eq!(result, "ok");
    assert!(!has_branch(&repo, "release"));
    assert!(has_branch(&repo, "release.1"));
}

/// A worktree add that fails after creating its branch deletes the branch again; a setting
/// without a value left under that name must not crash it.
#[test]
fn a_failed_worktree_add_removes_its_branch_despite_a_setting_written_without_a_value() {
    let (_dir, repo, result) = run_isolated(
        "a_failed_worktree_add_removes_its_branch_despite_a_setting_written_without_a_value",
        || {
            let (dir, repo) = repo_with_flagged();
            append_config(&repo, "[branch \"x\"]\n\tflag\n");
            // A stale `.git/worktrees/wt` that libgit2 refuses to reuse: the add fails after
            // creating the branch.
            let admin = repo.path().join("worktrees").join("wt");
            std::fs::create_dir_all(&admin).unwrap();
            std::fs::write(admin.join("marker"), "kept").unwrap();
            (dir, repo)
        },
        |repo| {
            let linked = repo.workdir().unwrap().join("a").join("wt");
            match git_core::worktree::create_worktree(repo, "wt", &linked, "x", Some("HEAD")) {
                Ok(()) => Err("the add succeeded".to_string()),
                Err(_) => Ok(()),
            }
        },
    );

    assert_eq!(result, "ok");
    assert!(!has_branch(&repo, "x"));
}

/// The remote's setting moves with it, written as `true`; nothing stays under the old name.
#[test]
fn rename_remote_carries_a_setting_written_without_a_value_as_true() {
    let (dir, _repo, result) = run_isolated(
        "rename_remote_carries_a_setting_written_without_a_value_as_true",
        origin_with("[remote \"origin\"]\n\tflag\n"),
        |repo| {
            git_core::remote::rename_remote(repo, "origin", "upstream").map_err(|e| e.to_string())
        },
    );
    let d = dir.path();

    assert_eq!(result, "ok");
    assert_eq!(
        git_out(d, &["config", "--type=bool", "remote.upstream.flag"]),
        "true\n"
    );
    assert_eq!(
        git_out(d, &["config", "remote.upstream.url"]),
        "https://example.invalid/r.git\n"
    );
    assert_eq!(settings(d, r"^remote\.origin\."), "");
}

/// The remote's section goes whole, as with `git remote remove`.
#[test]
fn remove_remote_removes_a_setting_written_without_a_value() {
    let (dir, repo, result) = run_isolated(
        "remove_remote_removes_a_setting_written_without_a_value",
        origin_with("[remote \"origin\"]\n\tflag\n"),
        |repo| git_core::remote::remove_remote(repo, "origin").map_err(|e| e.to_string()),
    );

    assert_eq!(result, "ok");
    assert_eq!(settings(dir.path(), r"^remote\.origin\."), "");
    assert!(repo.find_remote("origin").is_err());
}

const BRANCH_REMOTE_MISSING: &str = "The setting 'branch.flagged.remote' is written without a \
     value; git rejects it as a bad config variable. Give it a value or remove it (git config \
     --edit), then try again.";

const BRANCH_REMOTE: &str = "[branch \"flagged\"]\n\tremote\n";

/// `branch.<name>.remote` must name a remote: git refuses `git remote rename` with one written
/// without a value ("bad config variable"), and so does Rename remote, changing nothing.
#[test]
fn rename_remote_refuses_a_branch_remote_written_without_a_value() {
    let (_dir, repo, result) = run_isolated(
        "rename_remote_refuses_a_branch_remote_written_without_a_value",
        origin_with(BRANCH_REMOTE),
        |repo| {
            git_core::remote::rename_remote(repo, "origin", "upstream").map_err(|e| e.to_string())
        },
    );

    assert_eq!(result, BRANCH_REMOTE_MISSING);
    let config = config_text(&repo);
    assert!(config.contains("[remote \"origin\"]"), "{config}");
    assert!(config.ends_with(BRANCH_REMOTE), "{config}");
    assert!(repo.find_reference("refs/remotes/origin/main").is_ok());
}

#[test]
fn remove_remote_refuses_a_branch_remote_written_without_a_value() {
    let (_dir, repo, result) = run_isolated(
        "remove_remote_refuses_a_branch_remote_written_without_a_value",
        origin_with(BRANCH_REMOTE),
        |repo| git_core::remote::remove_remote(repo, "origin").map_err(|e| e.to_string()),
    );

    assert_eq!(result, BRANCH_REMOTE_MISSING);
    let config = config_text(&repo);
    assert!(config.contains("[remote \"origin\"]"), "{config}");
    assert!(config.ends_with(BRANCH_REMOTE), "{config}");
    assert!(repo.find_reference("refs/remotes/origin/main").is_ok());
}

/// The refusal comes before the upstreams are cleared: `main` still tracks `origin/main`.
#[test]
fn remove_remote_and_clear_upstreams_refuses_before_clearing_any() {
    let (_dir, repo, result) = run_isolated(
        "remove_remote_and_clear_upstreams_refuses_before_clearing_any",
        || {
            let (dir, repo) = repo_with_origin();
            git_ok(dir.path(), &["branch", "-u", "origin/main", "main"]);
            append_config(&repo, BRANCH_REMOTE);
            (dir, repo)
        },
        |repo| {
            git_core::remote::remove_remote_and_clear_upstreams(repo, "origin")
                .map_err(|e| e.to_string())
        },
    );

    assert_eq!(result, BRANCH_REMOTE_MISSING);
    let config = config_text(&repo);
    assert!(
        config.contains("[branch \"main\"]\n\tremote = origin\n\tmerge = refs/heads/main\n"),
        "{config}"
    );
    assert!(config.ends_with(BRANCH_REMOTE), "{config}");
}

/// A `fetch` without a value left under the new remote's name: libgit2 adds the remote's refspec
/// next to it and crashed. git refuses it as a bad config variable, and so does Add remote.
#[test]
fn add_remote_refuses_a_left_over_fetch_written_without_a_value() {
    let (_dir, repo, result) = run_isolated(
        "add_remote_refuses_a_left_over_fetch_written_without_a_value",
        flagged_with("[remote \"new\"]\n\tfetch\n"),
        |repo| {
            git_core::remote::add_remote(repo, "new", "https://example.invalid/new.git", None)
                .map_err(|e| e.to_string())
        },
    );

    assert_eq!(
        result,
        "The setting 'remote.new.fetch' is written without a value; git rejects it as a bad \
         config variable. Give it a value or remove it (git config --edit), then try again."
    );
    let config = config_text(&repo);
    assert!(config.ends_with("[remote \"new\"]\n\tfetch\n"), "{config}");
    assert!(!config.contains("new.git"), "{config}");
}

/// A setting without a value whose name isn't UTF-8 (git2 can't write it back) belongs to branch
/// `release.\xff`: Delete branch `release` leaves it as it is, as `git branch -D` does (#1181).
#[test]
fn delete_branch_leaves_a_dotted_namesakes_setting_whose_name_is_not_utf8_alone() {
    const SECTION: &[u8] = b"[branch \"release.\xff\"]\n\tflag\n";
    let (_dir, repo, result) = run_isolated(
        "delete_branch_leaves_a_dotted_namesakes_setting_whose_name_is_not_utf8_alone",
        || {
            let (dir, repo) = repo_with_flagged();
            git_ok(dir.path(), &["branch", "release"]);
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(repo.path().join("config"))
                .unwrap();
            file.write_all(SECTION).unwrap();
            (dir, repo)
        },
        delete("release"),
    );

    assert_eq!(result, "ok");
    assert!(!has_branch(&repo, "release"));
    let config = std::fs::read(repo.path().join("config")).unwrap();
    assert!(config.ends_with(SECTION));
}

// #1182: libgit2 reads `merge.default` (the merge driver) at the start of every merge, and
// `merge.conflictstyle` at the start of every checkout that names no conflict style, without
// checking for a value; one written without a value crashes it. git refuses both ("missing value
// for 'merge.default'", "bad config variable"). So does every command that merges or checks out,
// before it changes anything; reads that merge in memory only to compare go on without it.

const DRIVER: &str = "merge.default";
const STYLE: &str = "merge.conflictstyle";

/// The refusal naming `setting`.
fn missing(setting: &str) -> String {
    format!(
        "The setting '{setting}' is written without a value; git rejects it as a bad config \
         variable. Give it a value or remove it (git config --edit), then try again."
    )
}

/// `setting` (`section.variable`) written without a value.
fn valueless(setting: &str) -> String {
    let (section, variable) = setting.split_once('.').unwrap();
    format!("[{section}]\n\t{variable}\n")
}

fn id(repo: &Repository, revision: &str) -> String {
    repo.revparse_single(revision)
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string()
}

/// What a refused command must leave as it was: HEAD, the index and the work tree (with file
/// contents), every ref, the stash list and the git dir's files (operation state).
fn snapshot(dir: &Path) -> String {
    let mut text = String::new();
    for args in [
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ][..],
        &["diff"],
        &["diff", "--cached"],
        &["for-each-ref"],
        &["stash", "list"],
    ] {
        text.push_str(&git_out(dir, args));
    }
    let mut names: Vec<String> = std::fs::read_dir(dir.join(".git"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    text.push_str(&names.join("\n"));
    text
}

/// `main`: `base` (a.txt), `main` (a.txt's first line) and `main2` (m.txt); `side` from `base`
/// (a.txt's last line, s.txt) and `conflict` from `base` (a.txt's first line otherwise); a stash
/// on `main` changing a.txt's second line; `origin/main` one commit ahead of `main`.
fn merge_repo() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\nb\nc\n");
    commit_all(&repo, "base");
    git_ok(d, &["branch", "-M", "main"]);
    git_ok(d, &["checkout", "-q", "-b", "side"]);
    write_file(d, "a.txt", "a\nb\nside\n");
    write_file(d, "s.txt", "s\n");
    commit_all(&repo, "side");
    git_ok(d, &["checkout", "-q", "-b", "conflict", "main"]);
    write_file(d, "a.txt", "conflict\nb\nc\n");
    commit_all(&repo, "conflict");
    git_ok(d, &["checkout", "-q", "main"]);
    write_file(d, "a.txt", "main\nb\nc\n");
    commit_all(&repo, "main");
    write_file(d, "m.txt", "m\n");
    commit_all(&repo, "main2");
    git_ok(d, &["checkout", "-q", "-b", "ahead"]);
    write_file(d, "u.txt", "u\n");
    commit_all(&repo, "upstream");
    git_ok(d, &["checkout", "-q", "main"]);
    git_ok(
        d,
        &["remote", "add", "origin", "https://example.invalid/r.git"],
    );
    git_ok(d, &["update-ref", "refs/remotes/origin/main", "ahead"]);
    git_ok(d, &["branch", "-D", "ahead"]);
    write_file(d, "a.txt", "main\nB\nc\n");
    git_ok(d, &["stash"]);
    (dir, repo)
}

/// In the child: `prepare` (before the setting is written), then `setting` written without a
/// value, then `command` on a newly opened repository. Asserts the command is refused with the
/// message naming `setting` and that it changed nothing.
fn assert_refused<T>(
    test: &str,
    setting: &'static str,
    prepare: impl FnOnce(&Repository) -> T,
    command: impl FnOnce(&mut Repository, T) -> Result<(), String>,
) {
    let (_dir, _repo, result) = run_isolated(test, merge_repo, move |repo| {
        let prepared = prepare(repo);
        append_config(repo, &valueless(setting));
        let mut repo = Repository::open(repo.path()).unwrap();
        let workdir = repo.workdir().unwrap().to_path_buf();
        let before = snapshot(&workdir);
        let result = command(&mut repo, prepared);
        let after = snapshot(&workdir);
        if after != before {
            return Err(format!("the command changed the repository: {result:?}"));
        }
        result
    });
    assert_eq!(result, missing(setting));
}

fn nothing(_repo: &Repository) {}

fn hooks() -> git_core::hooks::HookOptions<'static> {
    git_core::hooks::HookOptions::default()
}

fn text<T, E: std::fmt::Display>(result: Result<T, E>) -> Result<(), String> {
    result.map(|_| ()).map_err(|error| error.to_string())
}

fn sequence_text(run: git_core::pick::SequenceRun) -> Result<(), String> {
    text(run.outcome)
}

fn pick_entry(repo: &Repository, revision: &str) -> git_core::rebase::RebasePlanEntry {
    git_core::rebase::RebasePlanEntry {
        commit_id: id(repo, revision),
        action: git_core::rebase::RebaseAction::Pick,
        combined_message: None,
    }
}

/// `main` and `main2` replayed onto `side`, `main` stopped to edit.
fn rebase_stopped_to_edit(repo: &Repository) -> git_core::rebase::RebaseState {
    let mut edit = pick_entry(repo, "main~1");
    edit.action = git_core::rebase::RebaseAction::Edit;
    let plan = vec![edit, pick_entry(repo, "main")];
    let (state, result) = git_core::rebase::start_rebase(repo, &id(repo, "side"), plan).unwrap();
    assert_eq!(result, git_core::rebase::RebaseStepResult::PausedForEdit);
    state
}

/// A cherry-pick sequence of `conflict` and `side`, stopped on `conflict`'s conflict; resolved
/// and staged when `resolve`.
fn sequence_stopped_on_conflict(repo: &Repository, resolve: bool) -> git_core::pick::SequenceState {
    let ids = [id(repo, "conflict"), id(repo, "side")];
    let run = git_core::pick::cherry_pick_many(repo, &ids, hooks());
    let state = run.state.expect("the sequence stops on the conflict");
    if resolve {
        let workdir = repo.workdir().unwrap();
        write_file(workdir, "a.txt", "resolved\nb\nc\n");
        git_ok(workdir, &["add", "a.txt"]);
    }
    state
}

macro_rules! refused {
    ($test:ident, $setting:expr, $prepare:expr, $command:expr) => {
        #[test]
        fn $test() {
            assert_refused(stringify!($test), $setting, $prepare, $command);
        }
    };
}

refused!(
    start_merge_refuses_a_merge_driver_without_a_value,
    DRIVER,
    nothing,
    |repo, ()| text(git_core::merge::start_merge(
        repo,
        "side",
        git_core::merge::MergeOptions::default()
    ))
);

refused!(
    start_merge_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| text(git_core::merge::start_merge(
        repo,
        "side",
        git_core::merge::MergeOptions::default()
    ))
);

refused!(
    abort_merge_refuses_a_conflict_style_without_a_value,
    STYLE,
    |repo: &Repository| git_ok(repo.workdir().unwrap(), &["merge", "--no-commit", "side"]),
    |repo, ()| text(git_core::merge::abort_merge(repo))
);

refused!(
    resolve_whole_file_refuses_a_conflict_style_without_a_value,
    STYLE,
    |repo: &Repository| {
        let output = test_support::git(repo.workdir().unwrap(), &["merge", "conflict"]);
        assert!(!output.status.success(), "the merge conflicts: {output:?}");
    },
    |repo, ()| text(git_core::merge::resolve_whole_file(
        repo,
        "a.txt",
        git_core::merge::ConflictSide::Ours
    ))
);

refused!(
    cherry_pick_refuses_a_merge_driver_without_a_value,
    DRIVER,
    nothing,
    |repo, ()| {
        let side = id(repo, "side");
        text(git_core::pick::cherry_pick(repo, &side, None, hooks()))
    }
);

refused!(
    cherry_pick_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| {
        let side = id(repo, "side");
        text(git_core::pick::cherry_pick(repo, &side, None, hooks()))
    }
);

refused!(
    revert_refuses_a_merge_driver_without_a_value,
    DRIVER,
    nothing,
    |repo, ()| {
        let main = id(repo, "main~1");
        text(git_core::pick::revert(repo, &main, None, hooks()))
    }
);

refused!(
    revert_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| {
        let main = id(repo, "main~1");
        text(git_core::pick::revert(repo, &main, None, hooks()))
    }
);

refused!(
    cherry_pick_many_refuses_a_merge_driver_without_a_value,
    DRIVER,
    nothing,
    |repo, ()| {
        let ids = [id(repo, "side")];
        sequence_text(git_core::pick::cherry_pick_many(repo, &ids, hooks()))
    }
);

refused!(
    revert_many_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| {
        let ids = [id(repo, "main"), id(repo, "main~1")];
        sequence_text(git_core::pick::revert_many(repo, &ids, hooks()))
    }
);

refused!(
    continue_sequence_refuses_a_merge_driver_without_a_value,
    DRIVER,
    |repo: &Repository| sequence_stopped_on_conflict(repo, true),
    |repo, state| sequence_text(git_core::pick::continue_sequence(repo, state, hooks()))
);

refused!(
    continue_sequence_refuses_a_conflict_style_without_a_value,
    STYLE,
    |repo: &Repository| sequence_stopped_on_conflict(repo, true),
    |repo, state| sequence_text(git_core::pick::continue_sequence(repo, state, hooks()))
);

refused!(
    abort_sequence_refuses_a_conflict_style_without_a_value,
    STYLE,
    |repo: &Repository| sequence_stopped_on_conflict(repo, false),
    |repo, state| text(git_core::pick::abort_sequence(repo, &state))
);

refused!(
    start_rebase_refuses_a_merge_driver_without_a_value,
    DRIVER,
    nothing,
    |repo, ()| {
        let plan = vec![pick_entry(repo, "main~1"), pick_entry(repo, "main")];
        let onto = id(repo, "side");
        text(git_core::rebase::start_rebase(repo, &onto, plan))
    }
);

refused!(
    start_rebase_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| {
        let plan = vec![pick_entry(repo, "main~1"), pick_entry(repo, "main")];
        let onto = id(repo, "side");
        text(git_core::rebase::start_rebase(repo, &onto, plan))
    }
);

refused!(
    rebase_continue_refuses_a_merge_driver_without_a_value,
    DRIVER,
    rebase_stopped_to_edit,
    |repo, mut state| text(git_core::rebase::rebase_continue(repo, &mut state))
);

refused!(
    rebase_continue_refuses_a_conflict_style_without_a_value,
    STYLE,
    rebase_stopped_to_edit,
    |repo, mut state| text(git_core::rebase::rebase_continue(repo, &mut state))
);

refused!(
    abort_rebase_refuses_a_conflict_style_without_a_value,
    STYLE,
    rebase_stopped_to_edit,
    |repo, state| text(git_core::rebase::abort_rebase(repo, &state))
);

refused!(
    fold_staged_refuses_a_merge_driver_without_a_value,
    DRIVER,
    |repo: &Repository| {
        let workdir = repo.workdir().unwrap();
        write_file(workdir, "f.txt", "folded\n");
        git_ok(workdir, &["add", "f.txt"]);
    },
    |repo, ()| {
        let target = id(repo, "main~1");
        text(git_core::fold::fold_staged_into(repo, &target, hooks()))
    }
);

refused!(
    apply_stash_refuses_a_merge_driver_without_a_value,
    DRIVER,
    nothing,
    |repo, ()| text(git_core::stash::apply_stash(repo, 0, None, false))
);

refused!(
    apply_stash_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| text(git_core::stash::apply_stash(repo, 0, None, false))
);

refused!(
    pop_stash_refuses_a_merge_driver_without_a_value,
    DRIVER,
    nothing,
    |repo, ()| text(git_core::stash::pop_stash(repo, 0, None, false))
);

refused!(
    stash_and_switch_refuses_a_merge_driver_without_a_value_before_stashing,
    DRIVER,
    |repo: &Repository| write_file(repo.workdir().unwrap(), "a.txt", "dirty\nb\nc\n"),
    |repo, ()| text(git_core::branch::stash_and_switch(repo, "side", true))
);

refused!(
    stash_and_switch_refuses_a_conflict_style_without_a_value_before_stashing,
    STYLE,
    |repo: &Repository| write_file(repo.workdir().unwrap(), "a.txt", "dirty\nb\nc\n"),
    |repo, ()| text(git_core::branch::stash_and_switch(repo, "side", false))
);

refused!(
    save_stash_refuses_a_conflict_style_without_a_value,
    STYLE,
    |repo: &Repository| write_file(repo.workdir().unwrap(), "a.txt", "dirty\nb\nc\n"),
    |repo, ()| text(git_core::stash::save_stash(
        repo,
        &git_core::stash::StashOptions {
            message: None,
            keep_index: false,
            include_untracked: false,
            paths: Vec::new(),
        }
    ))
);

refused!(
    pull_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| text(git_core::remote::pull_after_fetch(repo, "origin", "main"))
);

refused!(
    switch_branch_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| text(git_core::branch::switch_branch(repo, "side"))
);

// The branch isn't created either: the switch would refuse after it.
refused!(
    create_branch_refuses_a_conflict_style_without_a_value_before_creating_it,
    STYLE,
    nothing,
    |repo, ()| text(git_core::branch::create_branch(repo, "new", "side"))
);

refused!(
    checkout_commit_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| {
        let side = id(repo, "side");
        text(git_core::branch::checkout_commit(repo, &side))
    }
);

refused!(
    hard_reset_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| {
        let base = id(repo, "main~2");
        text(git_core::reset::reset_to(
            repo,
            &base,
            git_core::reset::ResetMode::Hard,
        ))
    }
);

refused!(
    discard_file_refuses_a_conflict_style_without_a_value,
    STYLE,
    |repo: &Repository| write_file(repo.workdir().unwrap(), "a.txt", "dirty\nb\nc\n"),
    |repo, ()| text(git_core::stage::discard_file(repo, "a.txt"))
);

refused!(
    discard_all_unstaged_refuses_a_conflict_style_without_a_value,
    STYLE,
    |repo: &Repository| write_file(repo.workdir().unwrap(), "a.txt", "dirty\nb\nc\n"),
    |repo, ()| text(git_core::stage::discard_all_unstaged(
        repo,
        &["a.txt".to_string()]
    ))
);

refused!(
    discard_hunk_refuses_a_conflict_style_without_a_value,
    STYLE,
    |repo: &Repository| write_file(repo.workdir().unwrap(), "a.txt", "main\ndirty\nc\n"),
    |repo, ()| text(git_core::stage::discard_hunk(
        repo,
        "a.txt",
        1,
        1,
        common::hunk_ctx(3)
    ))
);

refused!(
    restore_file_version_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| {
        let base = id(repo, "main~2");
        text(git_core::stage::restore_file_version(repo, &base, "a.txt"))
    }
);

refused!(
    apply_commit_change_refuses_a_conflict_style_without_a_value,
    STYLE,
    nothing,
    |repo, ()| {
        let side = id(repo, "side");
        text(git_core::patch::apply_commit_change(
            repo,
            &side,
            "s.txt",
            None,
            None,
            false,
            git_core::patch::ApplyTarget::WorkTree,
        ))
    }
);

refused!(
    apply_patch_refuses_a_conflict_style_without_a_value,
    STYLE,
    |repo: &Repository| {
        let workdir = repo.workdir().unwrap();
        write_file(workdir, "a.txt", "main\nb\npatched\n");
        let patch = test_support::git(workdir, &["diff"]).stdout;
        git_ok(workdir, &["checkout", "--", "a.txt"]);
        patch
    },
    |repo, patch| text(git_core::patch::apply_patch_with(
        repo,
        &patch,
        git_core::patch::PatchApply::WorkTree,
        hooks()
    ))
);

refused!(
    undo_of_a_work_tree_change_refuses_a_conflict_style_without_a_value,
    STYLE,
    |repo: &Repository| {
        let scope = git_core::undo::SnapshotScope::Full;
        let before = git_core::undo::capture(repo, scope).unwrap();
        git_ok(
            repo.workdir().unwrap(),
            &["reset", "-q", "--hard", "HEAD~1"],
        );
        let after = git_core::undo::capture(repo, scope).unwrap();
        (before, after)
    },
    |repo, (before, after)| text(git_core::undo::restore(repo, &after, &before, "undo"))
);

refused!(
    create_worktree_refuses_a_conflict_style_without_a_value,
    STYLE,
    |_repo: &Repository| TempDir::new().unwrap(),
    |repo, outside| {
        let path = outside.path().join("linked");
        let result =
            git_core::worktree::create_worktree(repo, "linked", &path, "linked", Some("HEAD"));
        if path.exists() {
            return Err("the worktree folder was made".to_string());
        }
        text(result)
    }
);

/// A duplicate pick stopped as empty is a leftover when HEAD carries the pick, which an
/// in-memory pick shows; without the merge driver that can't be known, so the pick counts as
/// still paused, and nothing crashes.
#[test]
fn head_info_counts_a_pick_as_paused_when_the_merge_driver_has_no_value() {
    let (_dir, _repo, result) = run_isolated(
        "head_info_counts_a_pick_as_paused_when_the_merge_driver_has_no_value",
        || {
            let (dir, repo) = merge_repo();
            let d = dir.path();
            git_ok(d, &["cherry-pick", "side"]);
            let again = test_support::git(d, &["cherry-pick", "side"]);
            assert!(!again.status.success(), "stops as empty: {again:?}");
            append_config(&repo, &valueless(DRIVER));
            (dir, repo)
        },
        |repo| {
            let info = git_core::repo::head_info(repo).map_err(|e| e.to_string())?;
            match (info.operation, info.operation_leftover) {
                (Some(_), false) => Ok(()),
                other => Err(format!("{other:?}")),
            }
        },
    );

    assert_eq!(result, "ok");
}

/// Likewise for a revert whose commit is HEAD and whose state was left behind.
#[test]
fn head_info_counts_a_revert_as_paused_when_the_merge_driver_has_no_value() {
    let (_dir, _repo, result) = run_isolated(
        "head_info_counts_a_revert_as_paused_when_the_merge_driver_has_no_value",
        || {
            let (dir, repo) = merge_repo();
            let d = dir.path();
            let reverted = id(&repo, "main");
            git_ok(d, &["revert", "--no-edit", "main"]);
            std::fs::write(repo.path().join("REVERT_HEAD"), format!("{reverted}\n")).unwrap();
            append_config(&repo, &valueless(DRIVER));
            (dir, repo)
        },
        |repo| {
            let info = git_core::repo::head_info(repo).map_err(|e| e.to_string())?;
            match (info.operation, info.operation_leftover) {
                (Some(_), false) => Ok(()),
                other => Err(format!("{other:?}")),
            }
        },
    );

    assert_eq!(result, "ok");
}

/// The refusal naming `setting` written without a value in the user's own config (global or
/// XDG level).
fn missing_in_user_config(setting: &str) -> String {
    format!(
        "The setting '{setting}' is written without a value in your global Git config; git \
         rejects it as a bad config variable. Give it a value or remove it (git config --global \
         --edit), then try again."
    )
}

/// The refusal naming `setting` written without a value in the system's config.
fn missing_in_system_config(setting: &str) -> String {
    format!(
        "The setting '{setting}' is written without a value in the system's Git config; git \
         rejects it as a bad config variable. Give it a value or remove it (git config --system \
         --edit), then try again."
    )
}

/// In the child: makes `dir` libgit2's search path for `level`, so the config file there
/// (`.gitconfig` for the global level, `config` for XDG, `gitconfig` for the system) is read as
/// that level by every repository opened after this, a new clone's included.
fn use_config_level(level: git2::ConfigLevel, dir: &Path) {
    // SAFETY: the child runs one test on one thread; nothing else uses libgit2 meanwhile.
    unsafe { git2::opts::set_search_path(level, dir) }.unwrap();
}

/// `setting` written without a value in `file` of a new folder `user` in `repo`'s git dir.
fn user_config_dir(repo: &Repository, file: &str, setting: &str) -> std::path::PathBuf {
    let dir = repo.path().join("user");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), valueless(setting)).unwrap();
    dir
}

struct NoCredentials;

impl git_core::remote::CredentialProvider for NoCredentials {
    fn credential(
        &mut self,
        _url: &str,
        _username: Option<&str>,
        _allowed: git2::CredentialType,
    ) -> Result<git2::Cred, git2::Error> {
        Err(git2::Error::from_str("no credentials"))
    }
}

struct NoProgress;

impl git_core::remote::TransferReporter for NoProgress {
    fn report(&mut self, _event: git_core::remote::TransferProgress) {}
}

/// Clones `repo` into the `clone` folder of its git dir.
fn clone_into_git_dir(repo: &Repository) -> Result<std::path::PathBuf, String> {
    let dest = repo.path().join("clone");
    let options = git_core::clone::CloneOptions {
        url: repo.workdir().unwrap().to_string_lossy().into_owned(),
        dest: dest.clone(),
        branch: None,
        recurse_submodules: false,
    };
    git_core::clone::clone_repo(&options, &mut NoCredentials, &mut NoProgress, &|| false)
        .map(|_| dest.clone())
        .map_err(|error| {
            if dest.exists() {
                format!("the refused clone made its folder: {error}")
            } else {
                error.to_string()
            }
        })
}

/// #1189: a clone checks out in a new repository, whose config is the user's own: a conflict
/// style written without a value there crashed it (libgit2 compares the value at the start of
/// the checkout). The clone is refused before anything is made.
#[test]
fn clone_refuses_a_conflict_style_without_a_value_in_the_users_global_config() {
    let (_dir, _repo, result) = run_isolated(
        "clone_refuses_a_conflict_style_without_a_value_in_the_users_global_config",
        repo_with_flagged,
        |repo| {
            let user = user_config_dir(repo, ".gitconfig", STYLE);
            use_config_level(git2::ConfigLevel::Global, &user);
            text(clone_into_git_dir(repo))
        },
    );

    assert_eq!(result, missing_in_user_config(STYLE));
}

#[test]
fn clone_refuses_a_conflict_style_without_a_value_in_the_users_xdg_config() {
    let (_dir, _repo, result) = run_isolated(
        "clone_refuses_a_conflict_style_without_a_value_in_the_users_xdg_config",
        repo_with_flagged,
        |repo| {
            let user = user_config_dir(repo, "config", STYLE);
            use_config_level(git2::ConfigLevel::XDG, &user);
            text(clone_into_git_dir(repo))
        },
    );

    assert_eq!(result, missing_in_user_config(STYLE));
}

#[test]
fn clone_refuses_a_conflict_style_without_a_value_in_the_system_config() {
    let (_dir, _repo, result) = run_isolated(
        "clone_refuses_a_conflict_style_without_a_value_in_the_system_config",
        repo_with_flagged,
        |repo| {
            let system = user_config_dir(repo, "gitconfig", STYLE);
            use_config_level(git2::ConfigLevel::System, &system);
            text(clone_into_git_dir(repo))
        },
    );

    assert_eq!(result, missing_in_system_config(STYLE));
}

/// A clone's checkout never reads the merge driver: one without a value doesn't stop it.
#[test]
fn clone_goes_ahead_with_a_merge_driver_without_a_value_in_the_users_config() {
    let (_dir, _repo, result) = run_isolated(
        "clone_goes_ahead_with_a_merge_driver_without_a_value_in_the_users_config",
        repo_with_flagged,
        |repo| {
            let user = user_config_dir(repo, ".gitconfig", DRIVER);
            use_config_level(git2::ConfigLevel::Global, &user);
            let dest = clone_into_git_dir(repo)?;
            match std::fs::read_to_string(dest.join("a.txt")) {
                Ok(text) if text == "a\n" => Ok(()),
                other => Err(format!("a.txt: {other:?}")),
            }
        },
    );

    assert_eq!(result, "ok");
}

/// A command in a repository names the config the setting is in: the user's own, not the
/// repository's.
#[test]
fn a_merge_names_the_users_config_when_the_setting_is_there() {
    let (_dir, _repo, result) = run_isolated(
        "a_merge_names_the_users_config_when_the_setting_is_there",
        merge_repo,
        |repo| {
            let user = user_config_dir(repo, ".gitconfig", STYLE);
            use_config_level(git2::ConfigLevel::Global, &user);
            let repo = Repository::open(repo.path()).unwrap();
            text(git_core::merge::start_merge(
                &repo,
                "side",
                git_core::merge::MergeOptions::default(),
            ))
        },
    );

    assert_eq!(result, missing_in_user_config(STYLE));
}

/// A superproject with submodule `deps/child` (its source in the git dir's `child-source`, with
/// commits `c1` and `c2`), recorded at `c2` and checked out at `c1`.
fn superproject_with_submodule_behind() -> (TempDir, Repository) {
    let (dir, repo) = repo_with_flagged();
    let source = repo.path().join("child-source");
    std::fs::create_dir_all(&source).unwrap();
    git_ok(&source, &["init", "-q", "-b", "main"]);
    git_ok(&source, &["config", "user.name", "Test User"]);
    git_ok(&source, &["config", "user.email", "test@example.com"]);
    write_file(&source, "c.txt", "1\n");
    git_ok(&source, &["add", "c.txt"]);
    git_ok(&source, &["commit", "-q", "-m", "c1"]);
    write_file(&source, "c.txt", "2\n");
    git_ok(&source, &["commit", "-q", "-am", "c2"]);
    git_ok(
        dir.path(),
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            source.to_str().unwrap(),
            "deps/child",
        ],
    );
    git_ok(dir.path(), &["commit", "-q", "-m", "add child"]);
    git_ok(
        &dir.path().join("deps/child"),
        &["checkout", "-q", "HEAD~1"],
    );
    (dir, repo)
}

/// #1189: Update submodule checks out in the submodule's own repository, whose config the
/// superproject's refusal doesn't read.
#[test]
fn update_submodule_refuses_a_conflict_style_without_a_value_in_the_submodules_config() {
    let (dir, _repo, result) = run_isolated(
        "update_submodule_refuses_a_conflict_style_without_a_value_in_the_submodules_config",
        || {
            let (dir, repo) = superproject_with_submodule_behind();
            let mut config = std::fs::OpenOptions::new()
                .append(true)
                .open(repo.path().join("modules/deps/child/config"))
                .unwrap();
            config.write_all(valueless(STYLE).as_bytes()).unwrap();
            (dir, repo)
        },
        |repo| {
            let child = repo.workdir().unwrap().join("deps/child");
            // The submodule's `.git` is a file: its HEAD, index and work tree.
            let snapshot = |dir: &Path| {
                [
                    "rev-parse HEAD",
                    "status --porcelain=v2",
                    "diff",
                    "diff --cached",
                ]
                .map(|args| git_out(dir, &args.split(' ').collect::<Vec<_>>()))
                .concat()
            };
            let before = snapshot(&child);
            let result = git_core::submodule::update_submodule(repo, "deps/child", false);
            if snapshot(&child) != before {
                return Err(format!("the update changed the submodule: {result:?}"));
            }
            text(result)
        },
    );

    assert_eq!(result, missing(STYLE));
    let c1 = git_out(
        &dir.path().join(".git/child-source"),
        &["rev-parse", "HEAD~1"],
    );
    assert_eq!(
        git_out(&dir.path().join("deps/child"), &["rev-parse", "HEAD"]),
        c1
    );
}

/// #1189: a submodule not checked out yet is cloned with the user's own config: refused before
/// the clone, which leaves its folder empty.
#[test]
fn update_submodule_refuses_a_conflict_style_without_a_value_in_the_users_config() {
    let (dir, _repo, result) = run_isolated(
        "update_submodule_refuses_a_conflict_style_without_a_value_in_the_users_config",
        || {
            let (dir, repo) = superproject_with_submodule_behind();
            git_ok(
                dir.path(),
                &["submodule", "deinit", "-q", "-f", "deps/child"],
            );
            std::fs::remove_dir_all(repo.path().join("modules/deps/child")).unwrap();
            (dir, repo)
        },
        |repo| {
            let user = user_config_dir(repo, ".gitconfig", STYLE);
            use_config_level(git2::ConfigLevel::Global, &user);
            git_core::submodule::init_submodule(repo, "deps/child").map_err(|e| e.to_string())?;
            text(git_core::submodule::update_submodule(
                repo,
                "deps/child",
                false,
            ))
        },
    );

    assert_eq!(result, missing_in_user_config(STYLE));
    let folder = dir.path().join("deps/child");
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
    assert!(!dir.path().join(".git/modules/deps/child").exists());
}
