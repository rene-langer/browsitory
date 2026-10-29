//! Probe and regression test for #156 (CVE-2021-21300 class): on a case-insensitive file system
//! a tree holding a symbolic link `D -> /outside` and a file `d/x` names the same folder twice.
//! A checkout that writes `d/x` after creating the link `D` would write through the link,
//! outside the repository.
//!
//! The test self-skips unless the temp directory is case-insensitive (macOS APFS, Windows NTFS
//! on CI). Set `BROWSITORY_CASE_PROBE_FORCE=1` to run it on a case-sensitive file system, which
//! only proves the harness (the names never collide there).
//!
//! Every row runs a work-tree-writing operation (raw libgit2 and git-core) against such a tree
//! and records what it left behind. The test prints one Markdown table row per row
//! (`--nocapture`) and asserts only that no file outside the repository changed. See
//! `docs/superpowers/specs/2026-10-28-security-followups-design.md` §1.3.

// Hide the developer's / CI runner's global and system git config (#111).
test_support::hermetic_git!();

use std::path::{Path, PathBuf};

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{ConfigLevel, Oid, Repository, Signature};
use tempfile::TempDir;

use git_core::branch;
use git_core::clone::{self, CloneOptions};
use git_core::merge::{self, FastForward, MergeOptions};
use git_core::pick;
use git_core::rebase::{self, RebaseAction, RebasePlanEntry};
use git_core::remote::{self, CredentialProvider, TransferProgress, TransferReporter};
use git_core::reset::{self, ResetMode};
use git_core::stash;
use git_core::submodule;
use git_core::undo::{self, SnapshotScope};
use git_core::worktree;

const MODE_BLOB: i32 = 0o100644;
const MODE_LINK: i32 = 0o120000;
const MODE_GITLINK: i32 = 0o160000;
const OUTSIDE: &str = "OUTSIDE\n";

/// Which spelling the link gets: `D` (link) + `d/x`, or `d` (link) + `D/x`.
#[derive(Clone, Copy, Debug)]
enum Order {
    LinkUpper,
    LinkLower,
}

impl Order {
    fn link(self) -> &'static str {
        match self {
            Order::LinkUpper => "D",
            Order::LinkLower => "d",
        }
    }
    fn folder(self) -> &'static str {
        match self {
            Order::LinkUpper => "d",
            Order::LinkLower => "D",
        }
    }
}

/// `core.ignorecase` as `Repository::init`/clone probed it, or forced to `false`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum IgnoreCase {
    Probed,
    False,
}

struct Row {
    name: String,
    order: Order,
    ignore_case: IgnoreCase,
    result: String,
    /// What the work tree holds under any spelling of `d` afterwards.
    at_d: String,
    outside_intact: bool,
}

fn case_insensitive(dir: &Path) -> bool {
    let _ = std::fs::write(dir.join("probe"), "");
    dir.join("PROBE").exists()
}

/// The evil source repository and the outside folder its link points at.
struct Fixture {
    _src_dir: TempDir,
    src: PathBuf,
    outside: TempDir,
    base: Oid,
    evil: Oid,
    evil2: Oid,
}

enum Item {
    Blob(Vec<u8>),
    Link(String),
    Gitlink(Oid),
}

/// Builds a tree from `/`-separated paths, nesting folders as needed.
fn build_tree(repo: &Repository, entries: &[(String, &Item)]) -> Oid {
    let mut builder = repo.treebuilder(None).unwrap();
    let mut folders: Vec<(String, Vec<(String, &Item)>)> = Vec::new();
    for (path, item) in entries {
        match path.split_once('/') {
            None => {
                let (id, mode) = match item {
                    Item::Blob(bytes) => (repo.blob(bytes).unwrap(), MODE_BLOB),
                    Item::Link(target) => (repo.blob(target.as_bytes()).unwrap(), MODE_LINK),
                    Item::Gitlink(id) => (*id, MODE_GITLINK),
                };
                builder.insert(path, id, mode).unwrap();
            }
            Some((first, rest)) => match folders.iter_mut().find(|(name, _)| name == first) {
                Some((_, children)) => children.push((rest.to_string(), *item)),
                None => folders.push((first.to_string(), vec![(rest.to_string(), *item)])),
            },
        }
    }
    for (name, children) in folders {
        let id = build_tree(repo, &children);
        builder.insert(&name, id, 0o040000).unwrap();
    }
    builder.write().unwrap()
}

fn commit(repo: &Repository, tree: Oid, parents: &[Oid], message: &str) -> Oid {
    let sig = Signature::now("T", "t@example.com").unwrap();
    let tree = repo.find_tree(tree).unwrap();
    let parents: Vec<_> = parents
        .iter()
        .map(|id| repo.find_commit(*id).unwrap())
        .collect();
    let parents: Vec<_> = parents.iter().collect();
    repo.commit(None, &sig, &sig, message, &tree, &parents)
        .unwrap()
}

fn configure(repo: &Repository) {
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "T").unwrap();
    config.set_str("user.email", "t@example.com").unwrap();
    config.set_bool("core.autocrlf", false).unwrap();
    config.set_str("core.eol", "lf").unwrap();
    config.set_bool("core.safecrlf", false).unwrap();
    config.set_bool("commit.gpgSign", false).unwrap();
    let hooks = repo.path().join("hooks");
    config
        .set_str("core.hooksPath", hooks.to_str().unwrap())
        .unwrap();
    // Windows: as a user who enabled links in Git for Windows. libgit2 decides at init from the
    // global config and a test link; this makes sure every repository here creates real links.
    if cfg!(windows) {
        config.set_bool("core.symlinks", true).unwrap();
    }
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn fixture(order: Order) -> Fixture {
    let src_dir = TempDir::new().unwrap();
    let src = src_dir.path().join("src");
    let repo = Repository::init(&src).unwrap();
    configure(&repo);
    let outside = TempDir::new().unwrap();
    std::fs::write(outside.path().join("x"), OUTSIDE).unwrap();
    let link_target = outside.path().to_string_lossy().into_owned();

    let a = Item::Blob(b"base\n".to_vec());
    let base_tree = build_tree(&repo, &[("a.txt".into(), &a)]);
    let base = commit(&repo, base_tree, &[], "base");

    let link = Item::Link(link_target);
    let pwned = Item::Blob(b"PWNED\n".to_vec());
    let pwned2 = Item::Blob(b"PWNED2\n".to_vec());
    let x = format!("{}/x", order.folder());
    let evil_tree = build_tree(
        &repo,
        &[
            ("a.txt".into(), &a),
            (order.link().into(), &link),
            (x.clone(), &pwned),
        ],
    );
    let evil = commit(&repo, evil_tree, &[base], "evil");
    let evil2_tree = build_tree(
        &repo,
        &[
            ("a.txt".into(), &a),
            (order.link().into(), &link),
            (x, &pwned2),
        ],
    );
    let evil2 = commit(&repo, evil2_tree, &[evil], "evil2");

    repo.reference("refs/heads/main", base, true, "").unwrap();
    repo.reference("refs/heads/evil", evil, true, "").unwrap();
    repo.reference("refs/heads/evil2", evil2, true, "").unwrap();
    repo.set_head("refs/heads/main").unwrap();
    Fixture {
        _src_dir: src_dir,
        src,
        outside,
        base,
        evil,
        evil2,
    }
}

/// A fresh clone of the fixture at `main` (base), with local `evil`/`evil2` branches and
/// `core.ignorecase` as asked.
fn fresh(fx: &Fixture, ignore_case: IgnoreCase) -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = RepoBuilder::new()
        .branch("main")
        .clone(&slash_path(&fx.src), dir.path())
        .unwrap();
    configure(&repo);
    if ignore_case == IgnoreCase::False {
        repo.config()
            .unwrap()
            .set_bool("core.ignorecase", false)
            .unwrap();
    }
    repo.reference("refs/heads/evil", fx.evil, true, "")
        .unwrap();
    repo.reference("refs/heads/evil2", fx.evil2, true, "")
        .unwrap();
    (dir, repo)
}

/// What stands under any spelling of `d` in `workdir`, e.g. `D=link d/x=missing`.
fn describe(workdir: &Path) -> String {
    let mut parts = Vec::new();
    let Ok(entries) = std::fs::read_dir(workdir) else {
        return "workdir missing".into();
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.eq_ignore_ascii_case("d") {
            continue;
        }
        let kind = match std::fs::symlink_metadata(entry.path()) {
            Ok(meta) if meta.file_type().is_symlink() => "link",
            Ok(meta) if meta.is_dir() => "dir",
            Ok(_) => "file",
            Err(_) => "?",
        };
        let inner = if kind == "dir" {
            let names: Vec<String> = std::fs::read_dir(entry.path())
                .map(|it| {
                    it.flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            format!("[{}]", names.join(","))
        } else {
            String::new()
        };
        parts.push(format!("{name}={kind}{inner}"));
    }
    if parts.is_empty() {
        "none".into()
    } else {
        parts.sort();
        parts.join(" ")
    }
}

fn outside_intact(outside: &Path) -> bool {
    let names: Vec<String> = std::fs::read_dir(outside)
        .map(|it| {
            it.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names == ["x"] && std::fs::read_to_string(outside.join("x")).is_ok_and(|text| text == OUTSIDE)
}

/// Restores `outside` for the next row, so one write-through can't hide another.
fn reset_outside(outside: &Path) {
    if let Ok(entries) = std::fs::read_dir(outside) {
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    std::fs::write(outside.join("x"), OUTSIDE).unwrap();
}

fn show<T, E: std::fmt::Display>(result: Result<T, E>) -> String {
    match result {
        Ok(_) => "Ok".into(),
        Err(error) => {
            let text = error.to_string().replace('|', "/").replace('\n', " ");
            let short: String = text.chars().take(90).collect();
            format!("Err({short})")
        }
    }
}

struct NoCredentials;
impl CredentialProvider for NoCredentials {
    fn credential(
        &mut self,
        _url: &str,
        _username: Option<&str>,
        _allowed: git2::CredentialType,
    ) -> Result<git2::Cred, git2::Error> {
        Err(git2::Error::from_str("no credentials"))
    }
}
struct NoReport;
impl TransferReporter for NoReport {
    fn report(&mut self, _event: TransferProgress) {}
}

fn checkout_two_step(repo: &Repository, fx: &Fixture, force: bool) -> Result<(), git2::Error> {
    for id in [fx.evil, fx.evil2] {
        let mut opts = CheckoutBuilder::new();
        if force {
            opts.force();
        } else {
            opts.safe();
        }
        let commit = repo.find_commit(id)?;
        repo.checkout_tree(commit.as_object(), Some(&mut opts))?;
        repo.set_head_detached(id)?;
    }
    Ok(())
}

/// A stash entry whose work-tree tree is `tree` on top of HEAD (`base`).
fn fake_stash(repo: &Repository, base: Oid, tree: Oid) {
    let base_commit = repo.find_commit(base).unwrap();
    let index_commit = commit(repo, base_commit.tree_id(), &[base], "index on main");
    let stash = commit(repo, tree, &[base, index_commit], "WIP on main");
    repo.reference_ensure_log("refs/stash").unwrap();
    repo.reference("refs/stash", stash, true, "WIP on main")
        .unwrap();
}

/// A super-repository whose submodule `sub` (URL = the fixture) is at `evil`.
fn super_repo(fx: &Fixture) -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    configure(&repo);
    let gitmodules = Item::Blob(
        format!(
            "[submodule \"sub\"]\n\tpath = sub\n\turl = {}\n",
            slash_path(&fx.src)
        )
        .into_bytes(),
    );
    let gitlink = Item::Gitlink(fx.evil);
    let tree = build_tree(
        &repo,
        &[
            (".gitmodules".into(), &gitmodules),
            ("sub".into(), &gitlink),
        ],
    );
    let id = commit(&repo, tree, &[], "super");
    repo.reference("refs/heads/main", id, true, "").unwrap();
    repo.set_head("refs/heads/main").unwrap();
    repo.checkout_head(Some(CheckoutBuilder::new().force()))
        .unwrap();
    (dir, repo)
}

fn run_rows(order: Order, ignore_case: IgnoreCase, rows: &mut Vec<Row>) {
    let fx = fixture(order);
    let outside = fx.outside.path().to_path_buf();
    let mut record = |name: &str, result: String, workdir: &Path| {
        rows.push(Row {
            name: name.into(),
            order,
            ignore_case,
            result,
            at_d: describe(workdir),
            outside_intact: outside_intact(&outside),
        });
        reset_outside(&outside);
    };

    // --- raw libgit2 ---
    if ignore_case == IgnoreCase::Probed {
        let dir = TempDir::new().unwrap();
        let result = RepoBuilder::new()
            .branch("evil")
            .clone(&slash_path(&fx.src), dir.path());
        record("raw clone", show(result), dir.path());
    } else {
        // A fresh clone's ignorecase can't be preset: clone without a checkout, set it, then
        // check out HEAD as the clone would have (forced into an empty work tree).
        let dir = TempDir::new().unwrap();
        let result = (|| {
            let mut none = CheckoutBuilder::new();
            none.dry_run();
            let repo = RepoBuilder::new()
                .branch("evil")
                .with_checkout(none)
                .clone(&slash_path(&fx.src), dir.path())?;
            configure(&repo);
            repo.config()?.set_bool("core.ignorecase", false)?;
            repo.checkout_head(Some(CheckoutBuilder::new().force()))
        })();
        record(
            "raw clone (no-checkout, then forced)",
            show(result),
            dir.path(),
        );
    }
    {
        let (dir, repo) = fresh(&fx, ignore_case);
        let result = checkout_two_step(&repo, &fx, false);
        record("raw safe checkout_tree x2", show(result), dir.path());
    }
    {
        let (dir, repo) = fresh(&fx, ignore_case);
        let result = checkout_two_step(&repo, &fx, true);
        record("raw forced checkout_tree x2", show(result), dir.path());
    }
    {
        let (dir, repo) = fresh(&fx, ignore_case);
        let result = (|| {
            let ours = repo.find_commit(fx.base)?;
            let theirs = repo.find_commit(fx.evil)?;
            let mut index = repo.merge_commits(&ours, &theirs, None)?;
            repo.checkout_index(Some(&mut index), Some(CheckoutBuilder::new().safe()))
        })();
        record(
            "raw merge_commits + checkout_index",
            show(result),
            dir.path(),
        );
    }
    if ignore_case == IgnoreCase::Probed {
        let (dir, repo) = super_repo(&fx);
        let result = (|| {
            let mut sub = repo.find_submodule("sub")?;
            sub.init(false)?;
            sub.update(true, None)
        })();
        record(
            "raw submodule.update",
            show(result),
            &dir.path().join("sub"),
        );
    }

    // --- git-core ---
    if ignore_case == IgnoreCase::Probed {
        let dir = TempDir::new().unwrap();
        let dest = dir.path().join("clone");
        let result = clone::clone_repo(
            &CloneOptions {
                url: fx.src.to_string_lossy().into_owned(),
                dest: dest.clone(),
                branch: Some("evil".into()),
                recurse_submodules: false,
            },
            &mut NoCredentials,
            &mut NoReport,
            &|| false,
        );
        record("clone_repo", show(result), &dest);
    }
    {
        let (dir, repo) = fresh(&fx, ignore_case);
        let result = branch::switch_branch(&repo, "evil")
            .and_then(|()| branch::switch_branch(&repo, "evil2"));
        record("switch_branch x2", show(result), dir.path());
    }
    {
        let (dir, repo) = fresh(&fx, ignore_case);
        let result = branch::checkout_commit(&repo, &fx.evil.to_string())
            .and_then(|()| branch::checkout_commit(&repo, &fx.evil2.to_string()));
        record("checkout_commit x2", show(result), dir.path());
    }
    for (name, fast_forward) in [
        ("start_merge ff", FastForward::Allow),
        ("start_merge --no-ff", FastForward::Never),
    ] {
        let (dir, repo) = fresh(&fx, ignore_case);
        let result = merge::start_merge(
            &repo,
            "evil",
            MergeOptions {
                fast_forward,
                squash: false,
            },
        );
        record(name, show(result), dir.path());
    }
    {
        let (dir, repo) = fresh(&fx, ignore_case);
        let result = pick::cherry_pick(&repo, &fx.evil.to_string(), None);
        record("cherry_pick", show(result), dir.path());
    }
    {
        let (dir, repo) = fresh(&fx, ignore_case);
        // A topic commit on main, rebased onto `evil`.
        let result = (|| -> Result<_, String> {
            std::fs::write(dir.path().join("b.txt"), "b\n").map_err(|e| e.to_string())?;
            let mut index = repo.index().map_err(|e| e.to_string())?;
            index
                .add_path(Path::new("b.txt"))
                .map_err(|e| e.to_string())?;
            index.write().map_err(|e| e.to_string())?;
            let tree = index.write_tree().map_err(|e| e.to_string())?;
            let topic = commit(&repo, tree, &[fx.base], "topic");
            repo.reference("refs/heads/main", topic, true, "")
                .map_err(|e| e.to_string())?;
            let plan = vec![RebasePlanEntry {
                commit_id: topic.to_string(),
                action: RebaseAction::Pick,
                combined_message: None,
            }];
            rebase::start_rebase(&repo, "evil", plan).map_err(|e| e.to_string())
        })();
        record("start_rebase", show(result), dir.path());
    }
    {
        let (dir, mut repo) = fresh(&fx, ignore_case);
        let evil_tree = repo.find_commit(fx.evil).unwrap().tree_id();
        fake_stash(&repo, fx.base, evil_tree);
        let result = stash::apply_stash(&mut repo, 0, None, false);
        record("apply_stash", show(result), dir.path());
    }
    {
        let (dir, repo) = fresh(&fx, ignore_case);
        let result = reset::reset_to(&repo, &fx.evil.to_string(), ResetMode::Hard)
            .and_then(|()| reset::reset_to(&repo, &fx.evil2.to_string(), ResetMode::Hard));
        record("reset_to Hard x2", show(result), dir.path());
    }
    {
        let (dir, repo) = fresh(&fx, ignore_case);
        let result = (|| -> Result<(), undo::UndoError> {
            let now = undo::capture(&repo, SnapshotScope::Full)?;
            let evil_tree = repo.find_commit(fx.evil)?.tree_id().to_string();
            let mut target = now.clone();
            target
                .refs
                .insert("refs/heads/main".into(), fx.evil.to_string());
            target.index_tree = evil_tree.clone();
            target.worktree_tree = Some(evil_tree);
            undo::restore(&repo, &now, &target, "undo")
        })();
        record("undo::restore", show(result), dir.path());
    }
    {
        let (dir, repo) = fresh(&fx, ignore_case);
        let path = dir.path().join("wt");
        let result = worktree::create_worktree(&repo, "wt", &path, "evil", None);
        record("create_worktree", show(result), &path);
    }
    if ignore_case == IgnoreCase::Probed {
        let (dir, repo) = super_repo(&fx);
        let result = submodule::init_submodule(&repo, "sub")
            .and_then(|()| submodule::update_submodule(&repo, "sub", false));
        record("update_submodule", show(result), &dir.path().join("sub"));
    }
    {
        let (dir, repo) = fresh(&fx, ignore_case);
        // As if a fetch had moved origin/main to `evil`.
        repo.reference("refs/remotes/origin/main", fx.evil, true, "")
            .unwrap();
        let result = remote::pull_after_fetch(&repo, "origin", "main");
        record("pull_after_fetch ff", show(result), dir.path());
    }
}

#[test]
fn case_insensitive_links_never_write_outside() {
    let probe = TempDir::new().unwrap();
    let forced = std::env::var_os("BROWSITORY_CASE_PROBE_FORCE").is_some();
    if !case_insensitive(probe.path()) && !forced {
        eprintln!("SKIP case-sensitive file system");
        return;
    }

    // libgit2 enables links on Windows only from the global config (Git for Windows' installer
    // option), so this binary points its global search path at a `.gitconfig` that says so.
    // Next to the hermetic dirs under target/, never deleted while tests run.
    let global = test_support::paths()
        .search_dir
        .parent()
        .unwrap()
        .join("case-probe-global");
    std::fs::create_dir_all(&global).unwrap();
    std::fs::write(global.join(".gitconfig"), "[core]\n\tsymlinks = true\n").unwrap();
    // SAFETY: this binary has exactly one test, so no other thread is using libgit2 while the
    // search path changes.
    unsafe {
        git2::opts::set_search_path(ConfigLevel::Global, &global).unwrap();
    }

    let link_probe = probe.path().join("link-probe");
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(probe.path(), &link_probe);
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_dir(probe.path(), &link_probe);
    if let Err(error) = linked {
        eprintln!("SKIP links unsupported: {error}");
        return;
    }

    {
        // What a fresh repository decides on its own (Windows: from the global config above).
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let config = repo.config().unwrap();
        eprintln!(
            "global config file: {:?}; fresh repo core.symlinks = {:?}, core.ignorecase = {:?}",
            git2::Config::find_global().ok(),
            config.get_bool("core.symlinks").ok(),
            config.get_bool("core.ignorecase").ok()
        );
    }

    let mut rows = Vec::new();
    for order in [Order::LinkUpper, Order::LinkLower] {
        for ignore_case in [IgnoreCase::Probed, IgnoreCase::False] {
            run_rows(order, ignore_case, &mut rows);
        }
    }

    eprintln!(
        "case-insensitive probe on {} (case-insensitive: {})",
        std::env::consts::OS,
        case_insensitive(probe.path())
    );
    eprintln!("| Row | Link | ignorecase | Result | Afterwards | Outside intact |");
    eprintln!("|---|---|---|---|---|---|");
    for row in &rows {
        eprintln!(
            "| {} | {} | {:?} | {} | {} | {} |",
            row.name,
            row.order.link(),
            row.ignore_case,
            row.result,
            row.at_d,
            if row.outside_intact { "yes" } else { "**NO**" }
        );
    }
    let failing: Vec<String> = rows
        .iter()
        .filter(|row| !row.outside_intact)
        .map(|row| format!("{} ({:?}, {:?})", row.name, row.order, row.ignore_case))
        .collect();
    assert!(
        failing.is_empty(),
        "rows wrote outside the repository: {failing:?}"
    );
}
