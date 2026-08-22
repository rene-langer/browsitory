# Multi-Repo Workspaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user scan a root folder for git repos, save a chosen subset as a named "workspace," reopen all of them as tabs in one action, and see them visually clustered in the tab strip.

**Architecture:** A new `Workspace` record (id/name/root/member-paths snapshot) persisted in `crates/config`'s existing `config.toml`, alongside a shape change to the already-persisted `open_repos` list so each open tab remembers which workspace (if any) it belongs to. Five new Tauri commands expose scan/CRUD to the frontend; a new `useWorkspaces` hook and `WorkspaceEditor` component drive creation/editing from `RepoPicker`; `RepoTabs` groups contiguous same-workspace tabs behind a name chip.

**Tech Stack:** Rust (Tauri backend, `crates/config` + `crates/tauri-app`), TypeScript/React (frontend), WebdriverIO (E2E).

**Spec:** `docs/superpowers/specs/2026-08-21-multi-repo-workspaces-design.md`

## Global Constraints

- Discovery scans only a root's **immediate children** — no recursion (spec Non-goals).
- A workspace's membership is a **snapshot**, never auto-re-synced — re-scanning only happens when the user explicitly opens the Edit flow (spec Non-goals).
- Workspace identity is the generated `id`, never `name` or `root_path` — this is what keeps two workspaces with identically-named member repos (the multi-clone case) collision-free.
- No new Rust dependency is needed for id generation (nanosecond timestamp + in-process counter, formatted as hex) — avoid adding a `uuid` crate for this.
- `ConfigFile`'s fields that serialize as TOML array-of-tables (`open_repos: Vec<OpenRepoEntry>`, `workspaces: Vec<Workspace>`) must be declared **after** all scalar/inline-array fields in the struct — the `toml` crate serializes fields in declaration order, and TOML syntax requires all plain `key = value` lines before any `[[array-of-tables]]` section.
- Every git-fixture repo created for tests uses `user.name = "Test User"` / `user.email = "test@example.com"`, matching the existing convention in `crates/git-core/tests/common/mod.rs` and `e2e/wdio.conf.ts`.

---

## Task 1: Config crate — `Workspace` type and CRUD

**Files:**
- Modify: `crates/config/src/lib.rs`
- Test: `crates/config/tests/workspaces.rs` (new)

**Interfaces:**
- Produces: `pub struct Workspace { pub id: String, pub name: String, pub root_path: PathBuf, pub member_paths: Vec<PathBuf> }` (public, `Debug + Clone + Serialize + Deserialize`); `pub fn list_workspaces() -> Result<Vec<Workspace>, ConfigError>`; `pub fn list_workspaces_at(config_file: &Path) -> Result<Vec<Workspace>, ConfigError>`; `pub fn save_workspace(name: &str, root: &Path, members: &[PathBuf]) -> Result<String, ConfigError>`; `pub fn save_workspace_at(config_file: &Path, name: &str, root: &Path, members: &[PathBuf]) -> Result<String, ConfigError>` (returns the new workspace's `id`); `pub fn update_workspace(id: &str, name: &str, members: &[PathBuf]) -> Result<(), ConfigError>`; `pub fn update_workspace_at(config_file: &Path, id: &str, name: &str, members: &[PathBuf]) -> Result<(), ConfigError>`; `pub fn delete_workspace(id: &str) -> Result<(), ConfigError>`; `pub fn delete_workspace_at(config_file: &Path, id: &str) -> Result<(), ConfigError>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/config/tests/workspaces.rs`:

```rust
use std::path::PathBuf;

use config::{
    delete_workspace_at, list_workspaces_at, save_workspace_at, update_workspace_at,
};

#[test]
fn save_workspace_at_creates_a_workspace_returned_by_list_workspaces_at() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let id = save_workspace_at(
        &config_file,
        "My Services",
        &PathBuf::from("/projects/root"),
        &[PathBuf::from("/projects/root/a"), PathBuf::from("/projects/root/b")],
    )
    .unwrap();

    let workspaces = list_workspaces_at(&config_file).unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].id, id);
    assert_eq!(workspaces[0].name, "My Services");
    assert_eq!(workspaces[0].root_path, PathBuf::from("/projects/root"));
    assert_eq!(
        workspaces[0].member_paths,
        vec![PathBuf::from("/projects/root/a"), PathBuf::from("/projects/root/b")]
    );
}

#[test]
fn save_workspace_at_generates_distinct_ids_for_each_call() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let id1 = save_workspace_at(&config_file, "One", &PathBuf::from("/a"), &[]).unwrap();
    let id2 = save_workspace_at(&config_file, "Two", &PathBuf::from("/b"), &[]).unwrap();

    assert_ne!(id1, id2);
}

#[test]
fn save_workspace_at_dedupes_a_colliding_name() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    save_workspace_at(&config_file, "project", &PathBuf::from("/clone-1"), &[]).unwrap();
    save_workspace_at(&config_file, "project", &PathBuf::from("/clone-2"), &[]).unwrap();
    save_workspace_at(&config_file, "project", &PathBuf::from("/clone-3"), &[]).unwrap();

    let mut names: Vec<String> = list_workspaces_at(&config_file)
        .unwrap()
        .into_iter()
        .map(|w| w.name)
        .collect();
    names.sort();
    assert_eq!(names, vec!["project", "project (2)", "project (3)"]);
}

#[test]
fn list_workspaces_at_on_a_missing_file_returns_empty() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    assert!(list_workspaces_at(&config_file).unwrap().is_empty());
}

#[test]
fn update_workspace_at_changes_name_and_members() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let id = save_workspace_at(
        &config_file,
        "Original",
        &PathBuf::from("/root"),
        &[PathBuf::from("/root/a")],
    )
    .unwrap();

    update_workspace_at(
        &config_file,
        &id,
        "Renamed",
        &[PathBuf::from("/root/a"), PathBuf::from("/root/c")],
    )
    .unwrap();

    let workspaces = list_workspaces_at(&config_file).unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].name, "Renamed");
    assert_eq!(
        workspaces[0].member_paths,
        vec![PathBuf::from("/root/a"), PathBuf::from("/root/c")]
    );
    // root_path is immutable after creation.
    assert_eq!(workspaces[0].root_path, PathBuf::from("/root"));
}

#[test]
fn update_workspace_at_on_an_unknown_id_is_a_no_op() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    save_workspace_at(&config_file, "Kept", &PathBuf::from("/root"), &[]).unwrap();

    update_workspace_at(&config_file, "nonexistent-id", "Ignored", &[]).unwrap();

    let workspaces = list_workspaces_at(&config_file).unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].name, "Kept");
}

#[test]
fn delete_workspace_at_removes_only_the_matching_workspace() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let keep_id = save_workspace_at(&config_file, "Keep", &PathBuf::from("/keep"), &[]).unwrap();
    let remove_id = save_workspace_at(&config_file, "Remove", &PathBuf::from("/remove"), &[]).unwrap();

    delete_workspace_at(&config_file, &remove_id).unwrap();

    let workspaces = list_workspaces_at(&config_file).unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].id, keep_id);
}

#[test]
fn delete_workspace_at_on_an_unknown_id_is_a_no_op() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    save_workspace_at(&config_file, "Kept", &PathBuf::from("/root"), &[]).unwrap();

    delete_workspace_at(&config_file, "nonexistent-id").unwrap();

    assert_eq!(list_workspaces_at(&config_file).unwrap().len(), 1);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p config --test workspaces`
Expected: FAIL to compile — `Workspace`, `save_workspace_at`, `list_workspaces_at`, `update_workspace_at`, `delete_workspace_at` don't exist yet.

- [ ] **Step 3: Implement `Workspace` and its CRUD functions**

In `crates/config/src/lib.rs`, add near the top (after the `ConfigError` enum):

```rust
use std::sync::atomic::{AtomicU64, Ordering};
```

Add after `MAX_RECENT_REPOS`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root_path: PathBuf,
    pub member_paths: Vec<PathBuf>,
}

static WORKSPACE_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A nanosecond timestamp plus an in-process counter, hex-formatted. Not a UUID — this repo
/// has no `uuid` dependency and doesn't need one: workspace ids are generated locally by a
/// single user, never compared across machines, so global uniqueness guarantees are overkill.
/// The counter alone (not just the timestamp) is what protects against two calls landing in
/// the same clock tick on a coarse-grained OS clock.
fn generate_workspace_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = WORKSPACE_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{counter:x}")
}

fn dedupe_workspace_name(existing: &[Workspace], name: &str, excluding_id: Option<&str>) -> String {
    let taken = |candidate: &str| {
        existing
            .iter()
            .any(|w| w.name == candidate && Some(w.id.as_str()) != excluding_id)
    };
    if !taken(name) {
        return name.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{name} ({n})");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

pub fn list_workspaces() -> Result<Vec<Workspace>, ConfigError> {
    list_workspaces_at(&config_file_path()?)
}

pub fn list_workspaces_at(config_file: &Path) -> Result<Vec<Workspace>, ConfigError> {
    Ok(read_config(config_file)?.workspaces)
}

pub fn save_workspace(name: &str, root: &Path, members: &[PathBuf]) -> Result<String, ConfigError> {
    save_workspace_at(&config_file_path()?, name, root, members)
}

pub fn save_workspace_at(
    config_file: &Path,
    name: &str,
    root: &Path,
    members: &[PathBuf],
) -> Result<String, ConfigError> {
    let mut config = read_config(config_file)?;
    let id = generate_workspace_id();
    let deduped_name = dedupe_workspace_name(&config.workspaces, name, None);
    config.workspaces.push(Workspace {
        id: id.clone(),
        name: deduped_name,
        root_path: root.to_path_buf(),
        member_paths: members.to_vec(),
    });
    write_config(config_file, &config)?;
    Ok(id)
}

pub fn update_workspace(id: &str, name: &str, members: &[PathBuf]) -> Result<(), ConfigError> {
    update_workspace_at(&config_file_path()?, id, name, members)
}

pub fn update_workspace_at(
    config_file: &Path,
    id: &str,
    name: &str,
    members: &[PathBuf],
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    let deduped_name = dedupe_workspace_name(&config.workspaces, name, Some(id));
    if let Some(workspace) = config.workspaces.iter_mut().find(|w| w.id == id) {
        workspace.name = deduped_name;
        workspace.member_paths = members.to_vec();
    }
    write_config(config_file, &config)
}

pub fn delete_workspace(id: &str) -> Result<(), ConfigError> {
    delete_workspace_at(&config_file_path()?, id)
}

pub fn delete_workspace_at(config_file: &Path, id: &str) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.workspaces.retain(|w| w.id != id);
    write_config(config_file, &config)
}
```

Add `#[serde(default)] workspaces: Vec<Workspace>,` as the **last** field of `ConfigFile` (after `active_repo`) — it must come after every scalar/inline-array field per this plan's Global Constraints.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p config --test workspaces`
Expected: PASS, all 8 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/config/src/lib.rs crates/config/tests/workspaces.rs
git commit -m "feat(config): add Workspace type and CRUD functions"
```

---

## Task 2: Config crate — scan a root for member repos

**Files:**
- Modify: `crates/config/src/lib.rs`
- Test: `crates/config/tests/scan_repos.rs` (new)

**Interfaces:**
- Consumes: nothing from Task 1 (independent function).
- Produces: `pub fn scan_repos_in_root(root: &Path) -> Result<Vec<PathBuf>, ConfigError>` — sorted list of `root`'s immediate child directories that contain a `.git` entry (directory or file, so worktree/submodule-style children count).

- [ ] **Step 1: Write the failing tests**

Create `crates/config/tests/scan_repos.rs`:

```rust
use std::fs;
use std::path::PathBuf;

use config::scan_repos_in_root;

#[test]
fn scan_repos_in_root_finds_only_immediate_children_with_a_dot_git_entry() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("repo-a/.git")).unwrap();
    fs::create_dir_all(root.join("repo-b/.git")).unwrap();
    fs::create_dir_all(root.join("not-a-repo")).unwrap();
    fs::write(root.join("a-file.txt"), "not a directory").unwrap();
    // Nested repo two levels down must NOT be found — scan is one level only.
    fs::create_dir_all(root.join("not-a-repo/nested-repo/.git")).unwrap();

    let found = scan_repos_in_root(root).unwrap();

    assert_eq!(found, vec![root.join("repo-a"), root.join("repo-b")]);
}

#[test]
fn scan_repos_in_root_counts_a_dot_git_file_as_well_as_a_dot_git_directory() {
    // Worktrees and submodules have a `.git` *file* (containing a `gitdir:` pointer), not a
    // directory — a root scan over a folder of worktree checkouts must still find them.
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("worktree-style")).unwrap();
    fs::write(root.join("worktree-style/.git"), "gitdir: /elsewhere/.git/worktrees/x\n").unwrap();

    let found = scan_repos_in_root(root).unwrap();

    assert_eq!(found, vec![root.join("worktree-style")]);
}

#[test]
fn scan_repos_in_root_returns_empty_for_a_root_with_no_repos() {
    let dir = tempfile::TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("plain-folder")).unwrap();

    assert_eq!(scan_repos_in_root(dir.path()).unwrap(), Vec::<PathBuf>::new());
}

#[test]
fn scan_repos_in_root_errors_on_a_nonexistent_root() {
    let dir = tempfile::TempDir::new().unwrap();
    let missing = dir.path().join("does-not-exist");

    assert!(scan_repos_in_root(&missing).is_err());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p config --test scan_repos`
Expected: FAIL to compile — `scan_repos_in_root` doesn't exist yet.

- [ ] **Step 3: Implement `scan_repos_in_root`**

In `crates/config/src/lib.rs`, add:

```rust
pub fn scan_repos_in_root(root: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    let mut repos = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        if path.join(".git").exists() {
            repos.push(path);
        }
    }
    repos.sort();
    Ok(repos)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p config --test scan_repos`
Expected: PASS, all 4 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/config/src/lib.rs crates/config/tests/scan_repos.rs
git commit -m "feat(config): add scan_repos_in_root for workspace discovery"
```

---

## Task 3: Config crate — thread `workspace_id` through `open_repos`

**Files:**
- Modify: `crates/config/src/lib.rs`
- Modify: `crates/config/tests/recent_repos.rs`

**Interfaces:**
- Consumes: nothing from Tasks 1-2.
- Produces: `pub struct OpenRepoEntry { pub path: PathBuf, pub workspace_id: Option<String> }` (public, `Debug + Clone + Serialize + Deserialize`); `list_open_repos`/`list_open_repos_at` now return `Result<(Vec<OpenRepoEntry>, Option<PathBuf>), ConfigError>`; `set_open_repos`/`set_open_repos_at` now take `entries: &[OpenRepoEntry]` instead of `paths: &[PathBuf]`.

This is the one behavior-preserving-but-shape-changing task in the config crate: an existing `config.toml` written by a version of the app before this change has `open_repos` as a plain array of path strings, not an array of `{ path, workspace_id }` tables. That file must still load correctly (as entries with `workspace_id: None`), not fail to parse and silently drop the user's persisted tabs.

- [ ] **Step 1: Write the failing tests**

In `crates/config/tests/recent_repos.rs`, replace the three existing tests that call `set_open_repos_at`/`list_open_repos_at` (`set_open_repos_at_persists_paths_and_active_repo`, `list_open_repos_at_on_a_missing_file_returns_empty`, `set_open_repos_at_does_not_disturb_recent_repos`) with:

```rust
use config::OpenRepoEntry;

#[test]
fn set_open_repos_at_persists_entries_and_active_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    set_open_repos_at(
        &config_file,
        &[
            OpenRepoEntry { path: PathBuf::from("/repos/a"), workspace_id: None },
            OpenRepoEntry { path: PathBuf::from("/repos/b"), workspace_id: Some("ws-1".into()) },
        ],
        Some(&PathBuf::from("/repos/b")),
    )
    .unwrap();

    let (entries, active) = list_open_repos_at(&config_file).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, PathBuf::from("/repos/a"));
    assert_eq!(entries[0].workspace_id, None);
    assert_eq!(entries[1].path, PathBuf::from("/repos/b"));
    assert_eq!(entries[1].workspace_id, Some("ws-1".to_string()));
    assert_eq!(active, Some(PathBuf::from("/repos/b")));
}

#[test]
fn list_open_repos_at_on_a_missing_file_returns_empty() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let (entries, active) = list_open_repos_at(&config_file).unwrap();
    assert!(entries.is_empty());
    assert_eq!(active, None);
}

#[test]
fn set_open_repos_at_does_not_disturb_recent_repos() {
    let dir = dir_with_recent_repo();
    let config_file = dir.0;

    set_open_repos_at(
        &config_file,
        &[OpenRepoEntry { path: PathBuf::from("/repos/a"), workspace_id: None }],
        None,
    )
    .unwrap();

    let recent = list_recent_repos_at(&config_file).unwrap();
    assert_eq!(recent, vec![PathBuf::from("/repos/recent")]);
}

fn dir_with_recent_repo() -> (PathBuf, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    add_recent_repo_at(&config_file, &PathBuf::from("/repos/recent")).unwrap();
    (config_file, dir)
}

#[test]
fn open_repos_round_trip_preserves_a_none_workspace_id() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    set_open_repos_at(
        &config_file,
        &[OpenRepoEntry { path: PathBuf::from("/repos/a"), workspace_id: None }],
        None,
    )
    .unwrap();

    let (entries, _) = list_open_repos_at(&config_file).unwrap();
    assert_eq!(entries[0].workspace_id, None);
}

#[test]
fn list_open_repos_at_parses_a_pre_workspaces_config_file() {
    // A config.toml written by a version of the app before workspaces existed: `open_repos`
    // is a bare array of path strings, not an array of `{ path, workspace_id }` tables.
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    std::fs::write(
        &config_file,
        "open_repos = [\"/repos/a\", \"/repos/b\"]\nactive_repo = \"/repos/a\"\n",
    )
    .unwrap();

    let (entries, active) = list_open_repos_at(&config_file).unwrap();

    assert_eq!(
        entries.iter().map(|e| e.path.clone()).collect::<Vec<_>>(),
        vec![PathBuf::from("/repos/a"), PathBuf::from("/repos/b")]
    );
    assert!(entries.iter().all(|e| e.workspace_id.is_none()));
    assert_eq!(active, Some(PathBuf::from("/repos/a")));
}
```

Note the helper `dir_with_recent_repo` returns `(PathBuf, TempDir)` so the `TempDir` guard stays alive for the duration of the test (it deletes its directory on drop) — mirror this pattern if adding further tests that build a fixture in a helper function.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p config --test recent_repos`
Expected: FAIL to compile — `OpenRepoEntry` doesn't exist, `set_open_repos_at`/`list_open_repos_at` still take/return `Vec<PathBuf>`.

- [ ] **Step 3: Implement the shape change**

In `crates/config/src/lib.rs`:

1. Add the new type (near `Workspace`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRepoEntry {
    pub path: PathBuf,
    #[serde(default)]
    pub workspace_id: Option<String>,
}
```

2. Change `ConfigFile`'s `open_repos` field type and reorder fields so scalar/inline-array fields come first and array-of-table fields (`open_repos`, `workspaces`) come last:

```rust
#[derive(Debug, Default, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    recent_repos: Vec<PathBuf>,
    #[serde(default)]
    active_repo: Option<PathBuf>,
    // These two fields serialize as TOML array-of-tables (`[[open_repos]]` / `[[workspaces]]`)
    // because their element type is a struct, not a scalar — TOML requires all plain
    // `key = value` lines to precede any array-of-tables section, so they must stay last.
    #[serde(default)]
    open_repos: Vec<OpenRepoEntry>,
    #[serde(default)]
    workspaces: Vec<Workspace>,
}
```

   (If Task 1 hasn't already added `workspaces: Vec<Workspace>` as the last field, add it now in this position instead.)

3. Add a `LegacyConfigFile` struct and change `read_config` to fall back to it:

```rust
#[derive(Debug, Default, Deserialize)]
struct LegacyConfigFile {
    #[serde(default)]
    recent_repos: Vec<PathBuf>,
    #[serde(default)]
    open_repos: Vec<PathBuf>,
    #[serde(default)]
    active_repo: Option<PathBuf>,
}

fn read_config(path: &Path) -> Result<ConfigFile, ConfigError> {
    if !path.exists() {
        return Ok(ConfigFile::default());
    }
    let contents = fs::read_to_string(path)?;
    match toml::from_str::<ConfigFile>(&contents) {
        Ok(config) => Ok(config),
        Err(_) => {
            // Pre-workspaces config.toml: `open_repos` was a bare array of path strings, not
            // an array of `{ path, workspace_id }` tables. Parse under the old shape and
            // normalize, rather than silently dropping a user's persisted tabs on first
            // launch after upgrade.
            let legacy: LegacyConfigFile = toml::from_str(&contents)?;
            Ok(ConfigFile {
                recent_repos: legacy.recent_repos,
                active_repo: legacy.active_repo,
                open_repos: legacy
                    .open_repos
                    .into_iter()
                    .map(|path| OpenRepoEntry { path, workspace_id: None })
                    .collect(),
                workspaces: Vec::new(),
            })
        }
    }
}
```

4. Update `list_open_repos`/`list_open_repos_at`/`set_open_repos`/`set_open_repos_at` signatures:

```rust
pub fn list_open_repos() -> Result<(Vec<OpenRepoEntry>, Option<PathBuf>), ConfigError> {
    list_open_repos_at(&config_file_path()?)
}

pub fn set_open_repos(entries: &[OpenRepoEntry], active: Option<&Path>) -> Result<(), ConfigError> {
    set_open_repos_at(&config_file_path()?, entries, active)
}

pub fn list_open_repos_at(
    config_file: &Path,
) -> Result<(Vec<OpenRepoEntry>, Option<PathBuf>), ConfigError> {
    let config = read_config(config_file)?;
    Ok((config.open_repos, config.active_repo))
}

pub fn set_open_repos_at(
    config_file: &Path,
    entries: &[OpenRepoEntry],
    active: Option<&Path>,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.open_repos = entries.to_vec();
    config.active_repo = active.map(|p| p.to_path_buf());
    write_config(config_file, &config)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p config`
Expected: PASS — every test in `recent_repos.rs`, `scan_repos.rs`, and `workspaces.rs`.

- [ ] **Step 5: Commit**

```bash
git add crates/config/src/lib.rs crates/config/tests/recent_repos.rs
git commit -m "feat(config): thread workspace_id through open_repos, with legacy-format fallback"
```

---

## Task 4: Backend — Tauri commands and registration

**Files:**
- Modify: `crates/tauri-app/src/commands.rs`
- Modify: `crates/tauri-app/src/main.rs`

**Interfaces:**
- Consumes: `config::Workspace`, `config::OpenRepoEntry`, `config::scan_repos_in_root`, `config::list_workspaces`, `config::save_workspace`, `config::update_workspace`, `config::delete_workspace`, `config::list_open_repos`, `config::set_open_repos` (Tasks 1-3).
- Produces: Tauri commands `scan_repos_in_root(root: String) -> Result<Vec<String>, String>`, `list_workspaces() -> Result<Vec<WorkspaceDto>, String>`, `save_workspace(name: String, root: String, members: Vec<String>) -> Result<String, String>`, `update_workspace(id: String, name: String, members: Vec<String>) -> Result<(), String>`, `delete_workspace(id: String) -> Result<(), String>`; updated `list_open_repos() -> Result<(Vec<OpenRepoEntryDto>, Option<String>), String>` and `persist_open_repos(entries: Vec<OpenRepoEntryInput>, active_path: Option<String>) -> Result<(), String>`.

This task has no Rust unit tests of its own — `commands.rs`'s existing test module only covers DTO `From` conversions (see `crates/tauri-app/src/commands.rs:1491` onward), not command bodies, since command bodies need a live `tauri::State` the test module doesn't construct. This one is verified by the workspace build compiling and by the E2E suite in Task 11. Follow the DTO pattern already used for `WorktreeInfoDto`/`SubmoduleInfoDto` (`#[derive(Serialize)] #[serde(rename_all = "camelCase")] struct XDto { ... } impl From<domain::X> for XDto`).

- [ ] **Step 1: Add DTOs and commands to `commands.rs`**

Near the other `*Dto` structs (e.g. after `WorktreeInfoDto`'s `impl From` block), add:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDto {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub member_paths: Vec<String>,
}

impl From<config::Workspace> for WorkspaceDto {
    fn from(workspace: config::Workspace) -> Self {
        Self {
            id: workspace.id,
            name: workspace.name,
            root_path: workspace.root_path.to_string_lossy().into_owned(),
            member_paths: workspace
                .member_paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRepoEntryDto {
    pub path: String,
    pub workspace_id: Option<String>,
}

impl From<config::OpenRepoEntry> for OpenRepoEntryDto {
    fn from(entry: config::OpenRepoEntry) -> Self {
        Self {
            path: entry.path.to_string_lossy().into_owned(),
            workspace_id: entry.workspace_id,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRepoEntryInput {
    pub path: String,
    pub workspace_id: Option<String>,
}
```

Near `pick_repo_folder`, add the new commands:

```rust
#[tauri::command]
pub fn scan_repos_in_root(root: String) -> Result<Vec<String>, String> {
    config::scan_repos_in_root(Path::new(&root))
        .map(|paths| paths.into_iter().map(|p| p.to_string_lossy().into_owned()).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_workspaces() -> Result<Vec<WorkspaceDto>, String> {
    config::list_workspaces()
        .map(|workspaces| workspaces.into_iter().map(WorkspaceDto::from).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_workspace(name: String, root: String, members: Vec<String>) -> Result<String, String> {
    config::save_workspace(
        &name,
        Path::new(&root),
        &members.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_workspace(id: String, name: String, members: Vec<String>) -> Result<(), String> {
    config::update_workspace(
        &id,
        &name,
        &members.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_workspace(id: String) -> Result<(), String> {
    config::delete_workspace(&id).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Update `list_open_repos`/`persist_open_repos`**

Replace the existing `list_open_repos`/`persist_open_repos` command bodies (currently working in terms of `Vec<String>`/`Vec<PathBuf>`) with:

```rust
#[tauri::command]
pub fn list_open_repos() -> Result<(Vec<OpenRepoEntryDto>, Option<String>), String> {
    let (entries, active) = config::list_open_repos().map_err(|e| e.to_string())?;
    Ok((
        entries.into_iter().map(OpenRepoEntryDto::from).collect(),
        active.map(|p| p.to_string_lossy().into_owned()),
    ))
}

#[tauri::command]
pub fn persist_open_repos(
    entries: Vec<OpenRepoEntryInput>,
    active_path: Option<String>,
) -> Result<(), String> {
    let entries: Vec<config::OpenRepoEntry> = entries
        .into_iter()
        .map(|e| config::OpenRepoEntry {
            path: PathBuf::from(e.path),
            workspace_id: e.workspace_id,
        })
        .collect();
    config::set_open_repos(&entries, active_path.as_deref().map(Path::new)).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Register the new commands**

In `crates/tauri-app/src/main.rs`, add `scan_repos_in_root, list_workspaces, save_workspace, update_workspace, delete_workspace` to the `use commands::{ ... }` import block (alphabetically, matching the existing ordering), and add the same five command names to the `tauri::generate_handler![ ... ]` list (anywhere in the list — that macro doesn't require alphabetical order, but match the existing entries' grouping by adding them near `pick_repo_folder`/`list_open_repos`/`persist_open_repos`).

- [ ] **Step 4: Verify the workspace builds**

Run: `cargo build --workspace`
Expected: succeeds with no errors.

Run: `cargo test -p tauri-app`
Expected: PASS — existing DTO tests still pass unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/tauri-app/src/commands.rs crates/tauri-app/src/main.rs
git commit -m "feat(tauri-app): expose workspace scan/CRUD and updated open-repos commands"
```

---

## Task 5: Frontend — `RepoClient` interface and `tauriRepoClient`

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`
- Modify: `frontend/src/ipc/tauriRepoClient.ts`

**Interfaces:**
- Consumes: Tauri commands from Task 4 (`scan_repos_in_root`, `list_workspaces`, `save_workspace`, `update_workspace`, `delete_workspace`, updated `list_open_repos`/`persist_open_repos`).
- Produces: `export interface Workspace { id: string; name: string; rootPath: string; memberPaths: string[] }`; `export interface OpenRepoEntry { path: string; workspaceId: string | null }`; `RepoClient.listOpenRepos(): Promise<{ entries: OpenRepoEntry[]; activePath: string | null }>`; `RepoClient.persistOpenRepos(entries: OpenRepoEntry[], activePath: string | null): Promise<void>`; `RepoClient.scanReposInRoot(root: string): Promise<string[]>`; `RepoClient.listWorkspaces(): Promise<Workspace[]>`; `RepoClient.saveWorkspace(name: string, root: string, members: string[]): Promise<string>`; `RepoClient.updateWorkspace(id: string, name: string, members: string[]): Promise<void>`; `RepoClient.deleteWorkspace(id: string): Promise<void>`.

This task has no dedicated unit test file — `RepoClient.ts` is a type-only interface and `tauriRepoClient.ts` is a thin `invoke()` mapping with no branching logic of its own (matching every other method in that file, none of which has a unit test). It's verified by the type checker and by Tasks 6-10's tests, which exercise these methods through fakes.

- [ ] **Step 1: Add types and interface methods to `RepoClient.ts`**

Add near the other domain interfaces (e.g. after `PullRequestList`):

```ts
export interface OpenRepoEntry {
  path: string;
  workspaceId: string | null;
}

export interface Workspace {
  id: string;
  name: string;
  rootPath: string;
  memberPaths: string[];
}
```

In the `RepoClient` interface, replace:

```ts
  listOpenRepos(): Promise<{ paths: string[]; activePath: string | null }>;
  persistOpenRepos(paths: string[], activePath: string | null): Promise<void>;
```

with:

```ts
  listOpenRepos(): Promise<{ entries: OpenRepoEntry[]; activePath: string | null }>;
  persistOpenRepos(entries: OpenRepoEntry[], activePath: string | null): Promise<void>;
  scanReposInRoot(root: string): Promise<string[]>;
  listWorkspaces(): Promise<Workspace[]>;
  saveWorkspace(name: string, root: string, members: string[]): Promise<string>;
  updateWorkspace(id: string, name: string, members: string[]): Promise<void>;
  deleteWorkspace(id: string): Promise<void>;
```

- [ ] **Step 2: Update `tauriRepoClient.ts`**

Add `OpenRepoEntry` and `Workspace` to the `import type { ... } from "./RepoClient"` block.

Replace:

```ts
  listOpenRepos: () =>
    invoke<[string[], string | null]>("list_open_repos").then(([paths, activePath]) => ({ paths, activePath })),
  persistOpenRepos: (paths: string[], activePath: string | null) =>
    invoke("persist_open_repos", { paths, activePath }),
```

with:

```ts
  listOpenRepos: () =>
    invoke<[OpenRepoEntry[], string | null]>("list_open_repos").then(([entries, activePath]) => ({
      entries,
      activePath,
    })),
  persistOpenRepos: (entries: OpenRepoEntry[], activePath: string | null) =>
    invoke("persist_open_repos", { entries, activePath }),
  scanReposInRoot: (root: string) => invoke<string[]>("scan_repos_in_root", { root }),
  listWorkspaces: () => invoke<Workspace[]>("list_workspaces"),
  saveWorkspace: (name: string, root: string, members: string[]) =>
    invoke<string>("save_workspace", { name, root, members }),
  updateWorkspace: (id: string, name: string, members: string[]) =>
    invoke("update_workspace", { id, name, members }),
  deleteWorkspace: (id: string) => invoke("delete_workspace", { id }),
```

- [ ] **Step 3: Verify the frontend type-checks**

Run: `cd frontend && npx tsc -b`
Expected: FAILS — every other file implementing/consuming `RepoClient` (`useOpenRepos.ts`, `App.tsx`, and the fake-client test files) still uses the old `listOpenRepos`/`persistOpenRepos` shape. This is expected at this point in the plan; Tasks 6 and 9-10 fix the real call sites. Confirm the *only* errors reported are about `listOpenRepos`/`persistOpenRepos`'s changed shape (in `useOpenRepos.ts` and `App.tsx`) — not about the five brand-new methods, which don't need every fake client to implement them (TypeScript structural typing on an object literal typed as `Partial<RepoClient>` doesn't require every property).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ipc/RepoClient.ts frontend/src/ipc/tauriRepoClient.ts
git commit -m "feat(frontend): add workspace methods to RepoClient, thread OpenRepoEntry shape"
```

---

## Task 6: Frontend — `useOpenRepos` workspace tagging and `openWorkspace`

**Files:**
- Modify: `frontend/src/state/useOpenRepos.ts`
- Modify: `frontend/src/state/useOpenRepos.test.ts`

**Interfaces:**
- Consumes: `RepoClient.listOpenRepos`/`persistOpenRepos` (new shape, Task 5), `Workspace` type (Task 5).
- Produces: `OpenRepo` gains `workspaceId: string | null`; `UseOpenReposResult` gains `openWorkspace(workspace: Workspace): Promise<void>`.

- [ ] **Step 1: Update existing tests for the new `listOpenRepos`/`persistOpenRepos` shape, and add tests for `openWorkspace`**

In `frontend/src/state/useOpenRepos.test.ts`:

Replace the `fakeClient` default mocks:

```ts
function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return {
    openRepo: vi.fn().mockResolvedValue(undefined),
    closeRepo: vi.fn().mockResolvedValue(undefined),
    listOpenRepos: vi.fn().mockResolvedValue({ entries: [], activePath: null }),
    persistOpenRepos: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as unknown as RepoClient;
}

function entry(path: string, workspaceId: string | null = null) {
  return { path, workspaceId };
}
```

Update every existing test that builds `listOpenRepos`'s mocked resolution from `{ paths: [...], activePath }` to `{ entries: [...].map((p) => entry(p)), activePath }`, and every assertion on `client.persistOpenRepos`'s arguments from `(paths, active)` to `(entries, active)` (e.g. `toHaveBeenLastCalledWith(["/repos/new"], "/repos/new")` becomes `toHaveBeenLastCalledWith([entry("/repos/new")], "/repos/new")`).

Then add:

```ts
  it("restores a persisted workspaceId onto its tab", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({
        entries: [entry("/repos/a", "ws-1"), entry("/repos/b")],
        activePath: "/repos/a",
      }),
    });
    const { result } = renderHook(() => useOpenRepos(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.openRepos[0].workspaceId).toBe("ws-1");
    expect(result.current.openRepos[1].workspaceId).toBeNull();
  });

  it("openWorkspace opens every member path, tagging each tab with the workspace id", async () => {
    const client = fakeClient();
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.openWorkspace({
        id: "ws-1",
        name: "Services",
        rootPath: "/projects",
        memberPaths: ["/projects/a", "/projects/b"],
      });
    });

    expect(client.openRepo).toHaveBeenCalledWith("/projects/a");
    expect(client.openRepo).toHaveBeenCalledWith("/projects/b");
    expect(result.current.openRepos.map((r) => [r.path, r.workspaceId])).toEqual([
      ["/projects/a", "ws-1"],
      ["/projects/b", "ws-1"],
    ]);
    expect(result.current.activePath).toBe("/projects/b");
    expect(client.persistOpenRepos).toHaveBeenLastCalledWith(
      [entry("/projects/a", "ws-1"), entry("/projects/b", "ws-1")],
      "/projects/b",
    );
  });

  it("openWorkspace skips a member that fails to open, rather than failing the whole open", async () => {
    const client = fakeClient({
      openRepo: vi.fn().mockImplementation((path: string) =>
        path === "/projects/gone" ? Promise.reject(new Error("not a git repository")) : Promise.resolve(undefined),
      ),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.openWorkspace({
        id: "ws-1",
        name: "Services",
        rootPath: "/projects",
        memberPaths: ["/projects/a", "/projects/gone"],
      });
    });

    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/projects/a"]);
  });

  it("openWorkspace does not duplicate a member that is already an open tab", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ entries: [entry("/projects/a")], activePath: "/projects/a" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.openWorkspace({
        id: "ws-1",
        name: "Services",
        rootPath: "/projects",
        memberPaths: ["/projects/a", "/projects/b"],
      });
    });

    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/projects/a", "/projects/b"]);
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx vitest run src/state/useOpenRepos.test.ts`
Expected: FAIL — `openWorkspace` doesn't exist; existing tests fail on the shape mismatch.

- [ ] **Step 3: Implement the changes in `useOpenRepos.ts`**

Add `Workspace` to the `import type { RepoClient } from "../ipc/RepoClient"` line (`import type { RepoClient, Workspace } from "../ipc/RepoClient";`).

Change `OpenRepo`:

```ts
export interface OpenRepo {
  path: string;
  displayName: string;
  workspaceId: string | null;
}
```

Add `openWorkspace` to `UseOpenReposResult`:

```ts
  openWorkspace(workspace: Workspace): Promise<void>;
```

Add a shared entry-mapping helper near `displayNameFor`:

```ts
function toEntries(repos: OpenRepo[]): { path: string; workspaceId: string | null }[] {
  return repos.map((r) => ({ path: r.path, workspaceId: r.workspaceId }));
}
```

Replace every `persist(next, ...)` call site's `persist` callback body (it currently calls `client.persistOpenRepos(repos.map((r) => r.path), active)`) with:

```ts
  const persist = useCallback(
    (repos: OpenRepo[], active: string | null) => {
      void client.persistOpenRepos(toEntries(repos), active);
    },
    [client],
  );
```

Update the mount-time restore effect: replace `client.listOpenRepos().then(async ({ paths, activePath: restoredActive }) => {` and its body's use of `paths` with the `entries`-based equivalent:

```ts
    client.listOpenRepos().then(async ({ entries, activePath: restoredActive }) => {
      if (ignore) return;
      const reopened = await Promise.all(
        entries.map((entry) => client.openRepo(entry.path).then(() => entry, () => null)),
      );
      if (ignore) return;
      const restored = reopened.filter((entry): entry is { path: string; workspaceId: string | null } => entry !== null);
      setOpenRepos(
        restored.map((entry) => ({
          path: entry.path,
          displayName: displayNameFor(entry.path),
          workspaceId: entry.workspaceId,
        })),
      );
      setActivePath(
        restoredActive !== null && restored.some((entry) => entry.path === restoredActive)
          ? restoredActive
          : restored[0]?.path ?? null,
      );
      setLoading(false);
    }).catch((error: unknown) => {
```

(The `.catch` block below is unchanged.)

Update `openRepo` to set `workspaceId: null` on the new tab:

```ts
  const openRepo = useCallback(
    async (path: string) => {
      await client.openRepo(path);
      setOpenRepos((prev) => {
        const next = prev.some((r) => r.path === path)
          ? prev
          : [...prev, { path, displayName: displayNameFor(path), workspaceId: null }];
        persist(next, path);
        return next;
      });
      setActivePath(path);
    },
    [client, persist],
  );
```

Add `openWorkspace` (after `openRepo`):

```ts
  const openWorkspace = useCallback(
    async (workspace: Workspace) => {
      const opened: string[] = [];
      for (const path of workspace.memberPaths) {
        try {
          await client.openRepo(path);
          opened.push(path);
        } catch {
          // Skipped, same silent-drop rule the mount-time restore uses for a path that fails
          // to (re-)open — moved, deleted, or permissions changed since the workspace was saved.
        }
      }
      if (opened.length === 0) return;
      setOpenRepos((prev) => {
        const existingPaths = new Set(prev.map((r) => r.path));
        const added = opened
          .filter((path) => !existingPaths.has(path))
          .map((path) => ({ path, displayName: displayNameFor(path), workspaceId: workspace.id }));
        const next = [...prev, ...added];
        persist(next, opened[opened.length - 1]);
        return next;
      });
      setActivePath(opened[opened.length - 1]);
    },
    [client, persist],
  );
```

Add `openWorkspace` to the final returned object: `return { openRepos, activePath, loading, restoreError, openRepo, closeRepo, switchTo, openWorkspace };`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx vitest run src/state/useOpenRepos.test.ts`
Expected: PASS, all tests.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/state/useOpenRepos.ts frontend/src/state/useOpenRepos.test.ts
git commit -m "feat(frontend): thread workspaceId through useOpenRepos, add openWorkspace"
```

---

## Task 7: Frontend — `useWorkspaces` hook

**Files:**
- Create: `frontend/src/state/useWorkspaces.ts`
- Test: `frontend/src/state/useWorkspaces.test.ts` (new)

**Interfaces:**
- Consumes: `RepoClient.listWorkspaces`/`saveWorkspace`/`updateWorkspace`/`deleteWorkspace` (Task 5).
- Produces: `export function useWorkspaces(client: RepoClient): UseWorkspacesResult` where `UseWorkspacesResult = { workspaces: Workspace[]; loading: boolean; error: string | null; createWorkspace(name: string, root: string, members: string[]): Promise<string>; editWorkspace(id: string, name: string, members: string[]): Promise<void>; deleteWorkspace(id: string): Promise<void> }`.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/state/useWorkspaces.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";
import { useWorkspaces } from "./useWorkspaces";

const workspace: Workspace = {
  id: "ws-1",
  name: "Services",
  rootPath: "/projects",
  memberPaths: ["/projects/a", "/projects/b"],
};

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return {
    listWorkspaces: vi.fn().mockResolvedValue([]),
    saveWorkspace: vi.fn().mockResolvedValue("ws-new"),
    updateWorkspace: vi.fn().mockResolvedValue(undefined),
    deleteWorkspace: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as unknown as RepoClient;
}

describe("useWorkspaces", () => {
  it("loads the workspace list on mount", async () => {
    const client = fakeClient({ listWorkspaces: vi.fn().mockResolvedValue([workspace]) });
    const { result } = renderHook(() => useWorkspaces(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.workspaces).toEqual([workspace]);
    expect(result.current.error).toBeNull();
  });

  it("surfaces a rejected listWorkspaces as error, with loading still resolving", async () => {
    const client = fakeClient({ listWorkspaces: vi.fn().mockRejectedValue(new Error("config unreadable")) });
    const { result } = renderHook(() => useWorkspaces(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.workspaces).toEqual([]);
    expect(result.current.error).toContain("config unreadable");
  });

  it("createWorkspace calls saveWorkspace and refreshes the list", async () => {
    const client = fakeClient({
      saveWorkspace: vi.fn().mockResolvedValue("ws-new"),
      listWorkspaces: vi
        .fn()
        .mockResolvedValueOnce([])
        .mockResolvedValueOnce([workspace]),
    });
    const { result } = renderHook(() => useWorkspaces(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    let id: string | undefined;
    await act(async () => {
      id = await result.current.createWorkspace("Services", "/projects", ["/projects/a", "/projects/b"]);
    });

    expect(id).toBe("ws-new");
    expect(client.saveWorkspace).toHaveBeenCalledWith("Services", "/projects", ["/projects/a", "/projects/b"]);
    expect(result.current.workspaces).toEqual([workspace]);
  });

  it("editWorkspace calls updateWorkspace and refreshes the list", async () => {
    const client = fakeClient({
      listWorkspaces: vi
        .fn()
        .mockResolvedValueOnce([workspace])
        .mockResolvedValueOnce([{ ...workspace, name: "Renamed" }]),
    });
    const { result } = renderHook(() => useWorkspaces(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.editWorkspace("ws-1", "Renamed", ["/projects/a"]);
    });

    expect(client.updateWorkspace).toHaveBeenCalledWith("ws-1", "Renamed", ["/projects/a"]);
    expect(result.current.workspaces[0].name).toBe("Renamed");
  });

  it("deleteWorkspace calls deleteWorkspace and refreshes the list", async () => {
    const client = fakeClient({
      listWorkspaces: vi.fn().mockResolvedValueOnce([workspace]).mockResolvedValueOnce([]),
    });
    const { result } = renderHook(() => useWorkspaces(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.deleteWorkspace("ws-1");
    });

    expect(client.deleteWorkspace).toHaveBeenCalledWith("ws-1");
    expect(result.current.workspaces).toEqual([]);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx vitest run src/state/useWorkspaces.test.ts`
Expected: FAIL — `useWorkspaces.ts` doesn't exist.

- [ ] **Step 3: Implement `useWorkspaces.ts`**

Create `frontend/src/state/useWorkspaces.ts`:

```ts
import { useCallback, useEffect, useState } from "react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";

export interface UseWorkspacesResult {
  workspaces: Workspace[];
  loading: boolean;
  error: string | null;
  createWorkspace(name: string, root: string, members: string[]): Promise<string>;
  editWorkspace(id: string, name: string, members: string[]): Promise<void>;
  deleteWorkspace(id: string): Promise<void>;
}

export function useWorkspaces(client: RepoClient): UseWorkspacesResult {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await client.listWorkspaces();
      setWorkspaces(list);
      setError(null);
    } catch (err: unknown) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [client]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const createWorkspace = useCallback(
    async (name: string, root: string, members: string[]) => {
      const id = await client.saveWorkspace(name, root, members);
      await refresh();
      return id;
    },
    [client, refresh],
  );

  const editWorkspace = useCallback(
    async (id: string, name: string, members: string[]) => {
      await client.updateWorkspace(id, name, members);
      await refresh();
    },
    [client, refresh],
  );

  const deleteWorkspace = useCallback(
    async (id: string) => {
      await client.deleteWorkspace(id);
      await refresh();
    },
    [client, refresh],
  );

  return { workspaces, loading, error, createWorkspace, editWorkspace, deleteWorkspace };
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx vitest run src/state/useWorkspaces.test.ts`
Expected: PASS, all 5 tests.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/state/useWorkspaces.ts frontend/src/state/useWorkspaces.test.ts
git commit -m "feat(frontend): add useWorkspaces hook for workspace CRUD"
```

---

## Task 8: Frontend — `WorkspaceEditor` component

**Files:**
- Create: `frontend/src/components/WorkspaceEditor.tsx`
- Create: `frontend/src/components/WorkspaceEditor.module.css`
- Test: `frontend/src/components/WorkspaceEditor.test.tsx` (new)

**Interfaces:**
- Consumes: `RepoClient.pickRepoFolder`/`scanReposInRoot` (Task 5), `Workspace` type (Task 5).
- Produces: `WorkspaceEditor({ client, existing, onSave, onCancel }: { client: RepoClient; existing?: Workspace; onSave: (name: string, root: string, members: string[]) => Promise<void>; onCancel: () => void })`. Create mode (`existing` undefined): shows a "Choose Root Folder" button first; picking a folder scans it and shows the checklist, all pre-checked, name defaulted to the root's basename. Edit mode (`existing` set): scans `existing.rootPath` immediately; checklist entries already in `existing.memberPaths` start checked, any newly-discovered path starts unchecked; name defaults to `existing.name`; no root-picking step (root is immutable after creation).

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/components/WorkspaceEditor.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient, Workspace } from "../ipc/RepoClient";
import { WorkspaceEditor } from "./WorkspaceEditor";

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return {
    pickRepoFolder: vi.fn().mockResolvedValue(null),
    scanReposInRoot: vi.fn().mockResolvedValue([]),
    ...overrides,
  } as unknown as RepoClient;
}

describe("WorkspaceEditor", () => {
  describe("create mode", () => {
    it("shows a root-picker button before any root is chosen", () => {
      render(<WorkspaceEditor client={fakeClient()} onSave={vi.fn()} onCancel={vi.fn()} />);
      expect(screen.getByText("Choose Root Folder")).toBeInTheDocument();
      expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    });

    it("scans the picked root and pre-checks every found repo, defaulting the name to the root's basename", async () => {
      const client = fakeClient({
        pickRepoFolder: vi.fn().mockResolvedValue("/projects/root"),
        scanReposInRoot: vi.fn().mockResolvedValue(["/projects/root/a", "/projects/root/b"]),
      });
      render(<WorkspaceEditor client={client} onSave={vi.fn()} onCancel={vi.fn()} />);

      fireEvent.click(screen.getByText("Choose Root Folder"));

      await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(2));
      for (const checkbox of screen.getAllByRole("checkbox")) {
        expect(checkbox).toBeChecked();
      }
      expect(screen.getByLabelText("Workspace name")).toHaveValue("root");
    });

    it("save calls onSave with the name, root, and only the checked members", async () => {
      const client = fakeClient({
        pickRepoFolder: vi.fn().mockResolvedValue("/projects/root"),
        scanReposInRoot: vi.fn().mockResolvedValue(["/projects/root/a", "/projects/root/b"]),
      });
      const onSave = vi.fn().mockResolvedValue(undefined);
      render(<WorkspaceEditor client={client} onSave={onSave} onCancel={vi.fn()} />);

      fireEvent.click(screen.getByText("Choose Root Folder"));
      await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(2));

      fireEvent.click(screen.getByLabelText("/projects/root/b"));
      fireEvent.change(screen.getByLabelText("Workspace name"), { target: { value: "My Root" } });
      fireEvent.click(screen.getByText("Save"));

      await waitFor(() => expect(onSave).toHaveBeenCalledWith("My Root", "/projects/root", ["/projects/root/a"]));
    });

    it("save is disabled when no member is checked", async () => {
      const client = fakeClient({
        pickRepoFolder: vi.fn().mockResolvedValue("/projects/root"),
        scanReposInRoot: vi.fn().mockResolvedValue(["/projects/root/a"]),
      });
      render(<WorkspaceEditor client={client} onSave={vi.fn()} onCancel={vi.fn()} />);

      fireEvent.click(screen.getByText("Choose Root Folder"));
      await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(1));
      fireEvent.click(screen.getByLabelText("/projects/root/a"));

      expect(screen.getByText("Save")).toBeDisabled();
    });

    it("cancelling the root-picker dialog leaves the picker step showing", async () => {
      const client = fakeClient({ pickRepoFolder: vi.fn().mockResolvedValue(null) });
      render(<WorkspaceEditor client={client} onSave={vi.fn()} onCancel={vi.fn()} />);

      fireEvent.click(screen.getByText("Choose Root Folder"));

      await waitFor(() => expect(client.pickRepoFolder).toHaveBeenCalled());
      expect(screen.getByText("Choose Root Folder")).toBeInTheDocument();
    });
  });

  describe("edit mode", () => {
    const existing: Workspace = {
      id: "ws-1",
      name: "Services",
      rootPath: "/projects/root",
      memberPaths: ["/projects/root/a"],
    };

    it("scans the existing root immediately, pre-checking current members and leaving new finds unchecked", async () => {
      const client = fakeClient({
        scanReposInRoot: vi.fn().mockResolvedValue(["/projects/root/a", "/projects/root/c"]),
      });
      render(<WorkspaceEditor client={client} existing={existing} onSave={vi.fn()} onCancel={vi.fn()} />);

      await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(2));
      expect(screen.getByLabelText("/projects/root/a")).toBeChecked();
      expect(screen.getByLabelText("/projects/root/c")).not.toBeChecked();
      expect(screen.getByLabelText("Workspace name")).toHaveValue("Services");
      expect(screen.queryByText("Choose Root Folder")).not.toBeInTheDocument();
    });

    it("save calls onSave with the immutable existing root", async () => {
      const client = fakeClient({ scanReposInRoot: vi.fn().mockResolvedValue(["/projects/root/a"]) });
      const onSave = vi.fn().mockResolvedValue(undefined);
      render(<WorkspaceEditor client={client} existing={existing} onSave={onSave} onCancel={vi.fn()} />);

      await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(1));
      fireEvent.click(screen.getByText("Save"));

      await waitFor(() => expect(onSave).toHaveBeenCalledWith("Services", "/projects/root", ["/projects/root/a"]));
    });
  });

  it("cancel calls onCancel", () => {
    const onCancel = vi.fn();
    render(<WorkspaceEditor client={fakeClient()} onSave={vi.fn()} onCancel={onCancel} />);
    fireEvent.click(screen.getByText("Cancel"));
    expect(onCancel).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx vitest run src/components/WorkspaceEditor.test.tsx`
Expected: FAIL — `WorkspaceEditor.tsx` doesn't exist.

- [ ] **Step 3: Implement `WorkspaceEditor.tsx`**

Create `frontend/src/components/WorkspaceEditor.module.css`:

```css
.list {
  list-style: none;
  margin: var(--space-2) 0;
  padding: 0;
}

.row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) 0;
}

.actions {
  display: flex;
  gap: var(--space-2);
  margin-top: var(--space-2);
}
```

Create `frontend/src/components/WorkspaceEditor.tsx`:

```tsx
import { useEffect, useState } from "react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";
import { Panel } from "./primitives/Panel";
import styles from "./WorkspaceEditor.module.css";

function basename(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const segments = trimmed.split(/[\\/]/);
  return segments[segments.length - 1] || trimmed;
}

export function WorkspaceEditor({
  client,
  existing,
  onSave,
  onCancel,
}: {
  client: RepoClient;
  existing?: Workspace;
  onSave: (name: string, root: string, members: string[]) => Promise<void>;
  onCancel: () => void;
}) {
  const [root, setRoot] = useState<string | null>(existing?.rootPath ?? null);
  const [candidates, setCandidates] = useState<string[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set(existing?.memberPaths ?? []));
  const [name, setName] = useState(existing?.name ?? "");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (root === null) return;
    client
      .scanReposInRoot(root)
      .then((found) => {
        setCandidates(found);
        if (existing === undefined) {
          setSelected(new Set(found));
        }
      })
      .catch((err: unknown) => setError(String(err)));
    // Only re-scan when `root` itself changes (picked once in create mode, fixed in edit mode)
    // — `existing`/`client` are stable identities for the component's lifetime.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root]);

  const handleChooseRoot = () => {
    client
      .pickRepoFolder()
      .then((picked) => {
        if (picked === null) return;
        setRoot(picked);
        setName(basename(picked));
      })
      .catch((err: unknown) => setError(String(err)));
  };

  const toggle = (path: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const handleSave = () => {
    if (root === null) return;
    onSave(name, root, Array.from(selected)).catch((err: unknown) => setError(String(err)));
  };

  return (
    <Panel title={existing === undefined ? "New Workspace" : "Edit Workspace"}>
      {error !== null && <p role="alert">{error}</p>}
      {root === null ? (
        <button type="button" onClick={handleChooseRoot}>
          Choose Root Folder
        </button>
      ) : (
        <>
          <p title={root}>{root}</p>
          {candidates !== null && (
            <ul className={styles.list}>
              {candidates.map((path) => (
                <li key={path} className={styles.row}>
                  <input
                    type="checkbox"
                    id={`member-${path}`}
                    aria-label={path}
                    checked={selected.has(path)}
                    onChange={() => toggle(path)}
                  />
                  <label htmlFor={`member-${path}`} title={path}>
                    {basename(path)}
                  </label>
                </li>
              ))}
            </ul>
          )}
          <label>
            Workspace name
            <input
              aria-label="Workspace name"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <div className={styles.actions}>
            <button type="button" disabled={selected.size === 0 || name.trim() === ""} onClick={handleSave}>
              Save
            </button>
            <button type="button" onClick={onCancel}>
              Cancel
            </button>
          </div>
        </>
      )}
    </Panel>
  );
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx vitest run src/components/WorkspaceEditor.test.tsx`
Expected: PASS, all tests.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/WorkspaceEditor.tsx frontend/src/components/WorkspaceEditor.module.css frontend/src/components/WorkspaceEditor.test.tsx
git commit -m "feat(frontend): add WorkspaceEditor for creating and editing workspaces"
```

---

## Task 9: Frontend — wire `RepoPicker` to workspaces

**Files:**
- Modify: `frontend/src/components/RepoPicker.tsx`
- Modify: `frontend/src/components/RepoPicker.test.tsx`

**Interfaces:**
- Consumes: `Workspace` type (Task 5), `WorkspaceEditor` (Task 8).
- Produces: `RepoPicker` gains props `onOpenWorkspace: (workspace: Workspace) => void`, `workspaces: Workspace[]`, `workspacesLoading: boolean`, `workspacesError: string | null`, `onCreateWorkspace: (name: string, root: string, members: string[]) => Promise<string>`, `onEditWorkspace: (id: string, name: string, members: string[]) => Promise<void>`, `onDeleteWorkspace: (id: string) => Promise<void>`.

- [ ] **Step 1: Write the failing tests**

In `frontend/src/components/RepoPicker.test.tsx`, add the new required props to the existing `render(<RepoPicker ... />)` calls (`workspaces={[]}`, `workspacesLoading={false}`, `workspacesError={null}`, `onOpenWorkspace={vi.fn()}`, `onCreateWorkspace={vi.fn()}`, `onEditWorkspace={vi.fn()}`, `onDeleteWorkspace={vi.fn()}`), then add:

```tsx
describe("RepoPicker workspaces", () => {
  const workspace = {
    id: "ws-1",
    name: "Services",
    rootPath: "/projects",
    memberPaths: ["/projects/a", "/projects/b"],
  };

  it("renders each saved workspace by name, with its root path as a tooltip", () => {
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[workspace]}
        workspacesLoading={false}
        workspacesError={null}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );

    const row = screen.getByText("Services");
    expect(row).toBeInTheDocument();
    expect(screen.getByTitle("/projects")).toBeInTheDocument();
  });

  it("Open All calls onOpenWorkspace with the workspace", () => {
    const onOpenWorkspace = vi.fn();
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={onOpenWorkspace}
        workspaces={[workspace]}
        workspacesLoading={false}
        workspacesError={null}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Open All" }));

    expect(onOpenWorkspace).toHaveBeenCalledWith(workspace);
  });

  it("Delete asks for confirmation before calling onDeleteWorkspace", async () => {
    const onDeleteWorkspace = vi.fn().mockResolvedValue(undefined);
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[workspace]}
        workspacesLoading={false}
        workspacesError={null}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={onDeleteWorkspace}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Delete Services" }));
    expect(onDeleteWorkspace).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Delete workspace" }));
    await waitFor(() => expect(onDeleteWorkspace).toHaveBeenCalledWith("ws-1"));
  });

  it("Open Workspace Root shows the WorkspaceEditor in create mode", () => {
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[]}
        workspacesLoading={false}
        workspacesError={null}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Open Workspace Root"));

    expect(screen.getByText("New Workspace")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx vitest run src/components/RepoPicker.test.tsx`
Expected: FAIL — `RepoPicker` doesn't accept these props yet, none of the new UI exists.

- [ ] **Step 3: Implement the changes in `RepoPicker.tsx`**

```tsx
import { useEffect, useState } from "react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";
import { ListRow } from "./primitives/ListRow";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import { WorkspaceEditor } from "./WorkspaceEditor";
import styles from "./RepoPicker.module.css";

export function RepoPicker({
  client,
  onOpenRepo,
  onOpenWorkspace,
  workspaces,
  workspacesLoading,
  workspacesError,
  onCreateWorkspace,
  onEditWorkspace,
  onDeleteWorkspace,
}: {
  client: RepoClient;
  onOpenRepo: (path: string) => void;
  onOpenWorkspace: (workspace: Workspace) => void;
  workspaces: Workspace[];
  workspacesLoading: boolean;
  workspacesError: string | null;
  onCreateWorkspace: (name: string, root: string, members: string[]) => Promise<string>;
  onEditWorkspace: (id: string, name: string, members: string[]) => Promise<void>;
  onDeleteWorkspace: (id: string) => Promise<void>;
}) {
  const [recentRepos, setRecentRepos] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [creatingWorkspace, setCreatingWorkspace] = useState(false);
  const [editingWorkspace, setEditingWorkspace] = useState<Workspace | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] = useState<Workspace | null>(null);

  useEffect(() => {
    client
      .listRecentRepos()
      .then(setRecentRepos)
      .catch((err: unknown) => setError(String(err)));
  }, [client]);

  const handleOpenFolder = () => {
    client
      .pickRepoFolder()
      .then((path) => {
        if (path !== null) {
          onOpenRepo(path);
        }
      })
      .catch((err: unknown) => setError(String(err)));
  };

  if (creatingWorkspace) {
    return (
      <WorkspaceEditor
        client={client}
        onSave={(name, root, members) => onCreateWorkspace(name, root, members).then(() => setCreatingWorkspace(false))}
        onCancel={() => setCreatingWorkspace(false)}
      />
    );
  }

  if (editingWorkspace !== null) {
    return (
      <WorkspaceEditor
        client={client}
        existing={editingWorkspace}
        onSave={(name, root, members) =>
          onEditWorkspace(editingWorkspace.id, name, members).then(() => setEditingWorkspace(null))
        }
        onCancel={() => setEditingWorkspace(null)}
      />
    );
  }

  return (
    <Panel title="Open a repository">
      <Toolbar>
        <button onClick={handleOpenFolder}>Open Folder</button>
        <button onClick={() => setCreatingWorkspace(true)}>Open Workspace Root</button>
      </Toolbar>
      {error !== null && <p role="alert">{error}</p>}
      {recentRepos.length === 0 ? (
        <p>No recent repositories</p>
      ) : (
        <ul className={styles.list}>
          {recentRepos.map((path) => (
            <ListRow key={path} onClick={() => onOpenRepo(path)}>
              {path}
            </ListRow>
          ))}
        </ul>
      )}
      <Panel title="Workspaces" headingLevel={3}>
        {workspacesError !== null && <p role="alert">{workspacesError}</p>}
        {!workspacesLoading && workspaces.length === 0 ? (
          <p>No saved workspaces</p>
        ) : (
          <ul className={styles.list}>
            {workspaces.map((workspace) => (
              <ListRow key={workspace.id}>
                <span title={workspace.rootPath}>{workspace.name}</span>
                <Toolbar>
                  <button onClick={() => onOpenWorkspace(workspace)}>Open All</button>
                  <button onClick={() => setEditingWorkspace(workspace)}>Edit</button>
                  <button onClick={() => setDeleteConfirmation(workspace)}>Delete {workspace.name}</button>
                </Toolbar>
              </ListRow>
            ))}
          </ul>
        )}
      </Panel>
      {deleteConfirmation !== null && (
        <dialog open aria-label={`Delete workspace ${deleteConfirmation.name}`}>
          <p>Delete workspace {deleteConfirmation.name}? Its member repos stay open if currently open; only the saved workspace is removed.</p>
          <button
            type="button"
            onClick={() =>
              void onDeleteWorkspace(deleteConfirmation.id).then(() => setDeleteConfirmation(null))
            }
          >
            Delete workspace
          </button>
          <button type="button" onClick={() => setDeleteConfirmation(null)}>
            Cancel
          </button>
        </dialog>
      )}
    </Panel>
  );
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx vitest run src/components/RepoPicker.test.tsx`
Expected: PASS, all tests (existing and new).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/RepoPicker.tsx frontend/src/components/RepoPicker.test.tsx
git commit -m "feat(frontend): wire RepoPicker to workspace creation, edit, and delete"
```

---

## Task 10: Frontend — `RepoTabs` grouping and `App.tsx` wiring

**Files:**
- Modify: `frontend/src/components/RepoTabs.tsx`
- Modify: `frontend/src/components/RepoTabs.module.css`
- Modify: `frontend/src/components/RepoTabs.test.tsx`
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `OpenRepo.workspaceId` (Task 6), `useWorkspaces` (Task 7), `RepoPicker`'s new props (Task 9).
- Produces: `RepoTabs` gains props `workspaceNames: Record<string, string>` and `onCloseGroup: (paths: string[]) => void`.

- [ ] **Step 1: Write the failing tests**

In `frontend/src/components/RepoTabs.test.tsx`:
- Update the top-of-file `repos` constant to include the now-required `workspaceId` field: `const repos = [{ path: "/repos/widget", displayName: "widget", workspaceId: null }, { path: "/repos/gadget", displayName: "gadget", workspaceId: null }];`
- Add `workspaceNames={{}}` and `onCloseGroup={vi.fn()}` to every existing `<RepoTabs ... />` render call.

Then add:

```tsx
describe("RepoTabs grouping", () => {
  const grouped = [
    { path: "/repos/widget", displayName: "widget", workspaceId: "ws-1" },
    { path: "/repos/gadget", displayName: "gadget", workspaceId: "ws-1" },
    { path: "/repos/solo", displayName: "solo", workspaceId: null },
  ];

  it("wraps a contiguous run of same-workspace tabs in a chip labeled with the workspace name", () => {
    render(
      <RepoTabs
        openRepos={grouped}
        activePath="/repos/widget"
        busyPaths={noneBusy}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );

    expect(screen.getByText("Services")).toBeInTheDocument();
    // Still individually clickable/closable.
    expect(screen.getByRole("tab", { name: /widget/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /gadget/i })).toBeInTheDocument();
  });

  it("a standalone tab (no workspaceId) renders with no chip", () => {
    render(
      <RepoTabs
        openRepos={grouped}
        activePath="/repos/widget"
        busyPaths={noneBusy}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );

    expect(screen.getByRole("tab", { name: /solo/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /close services/i })).toBeInTheDocument();
  });

  it("clicking the chip's close-all control calls onCloseGroup with every path in that run", () => {
    const onCloseGroup = vi.fn();
    render(
      <RepoTabs
        openRepos={grouped}
        activePath="/repos/widget"
        busyPaths={noneBusy}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={onCloseGroup}
        onAddTab={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /close services/i }));

    expect(onCloseGroup).toHaveBeenCalledWith(["/repos/widget", "/repos/gadget"]);
  });

  it("a tab whose workspaceId has no matching name in workspaceNames renders standalone", () => {
    render(
      <RepoTabs
        openRepos={[{ path: "/repos/orphan", displayName: "orphan", workspaceId: "deleted-ws" }]}
        activePath="/repos/orphan"
        busyPaths={noneBusy}
        workspaceNames={{}}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );

    expect(screen.getByRole("tab", { name: /orphan/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /close deleted-ws/i })).not.toBeInTheDocument();
  });
});
```

Note this requires `fireEvent` to already be imported in this test file — add `import { fireEvent, render, screen } from "@testing-library/react";` (replacing the existing `import { render, screen } from "@testing-library/react";`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx vitest run src/components/RepoTabs.test.tsx`
Expected: FAIL — `RepoTabs` doesn't accept `workspaceNames`/`onCloseGroup`, no grouping renders.

- [ ] **Step 3: Implement grouping in `RepoTabs.tsx`**

```tsx
import { X, Plus } from "lucide-react";
import type { OpenRepo } from "../state/useOpenRepos";
import styles from "./RepoTabs.module.css";

interface TabGroup {
  workspaceId: string | null;
  workspaceName: string | null;
  repos: OpenRepo[];
}

function groupContiguousTabs(openRepos: OpenRepo[], workspaceNames: Record<string, string>): TabGroup[] {
  const groups: TabGroup[] = [];
  for (const repo of openRepos) {
    const workspaceName = repo.workspaceId !== null ? workspaceNames[repo.workspaceId] ?? null : null;
    const last = groups[groups.length - 1];
    if (last !== undefined && last.workspaceId === repo.workspaceId && workspaceName !== null) {
      last.repos.push(repo);
      continue;
    }
    groups.push({ workspaceId: workspaceName !== null ? repo.workspaceId : null, workspaceName, repos: [repo] });
  }
  return groups;
}

export function RepoTabs({
  openRepos,
  activePath,
  busyPaths,
  workspaceNames,
  onSwitchTo,
  onClose,
  onCloseGroup,
  onAddTab,
}: {
  openRepos: OpenRepo[];
  activePath: string | null;
  busyPaths: ReadonlySet<string>;
  workspaceNames: Record<string, string>;
  onSwitchTo: (path: string) => void;
  onClose: (path: string) => void;
  onCloseGroup: (paths: string[]) => void;
  onAddTab: () => void;
}) {
  if (openRepos.length === 0) return null;

  const renderTab = (repo: OpenRepo) => {
    const selected = repo.path === activePath;
    const busy = busyPaths.has(repo.path);
    return (
      <div key={repo.path} className={selected ? `${styles.tab} ${styles.active}` : styles.tab}>
        <button
          type="button"
          role="tab"
          aria-selected={selected}
          title={repo.path}
          className={styles.tabLabel}
          onClick={() => onSwitchTo(repo.path)}
        >
          {repo.displayName}
        </button>
        <button
          type="button"
          className={styles.closeButton}
          aria-label={`Close ${repo.displayName}`}
          title={busy ? "This repo has an operation in progress" : undefined}
          disabled={busy}
          onClick={() => onClose(repo.path)}
        >
          <X size={12} aria-hidden="true" />
        </button>
      </div>
    );
  };

  return (
    <div className={styles.tabs} role="tablist" aria-label="Open repositories">
      {groupContiguousTabs(openRepos, workspaceNames).map((group, index) =>
        group.workspaceName !== null ? (
          <div key={`group-${index}`} className={styles.group}>
            <div className={styles.groupHeader}>
              <span className={styles.groupLabel}>{group.workspaceName}</span>
              <button
                type="button"
                className={styles.closeButton}
                aria-label={`Close ${group.workspaceName}`}
                onClick={() => onCloseGroup(group.repos.map((r) => r.path))}
              >
                <X size={12} aria-hidden="true" />
              </button>
            </div>
            <div className={styles.groupTabs}>{group.repos.map(renderTab)}</div>
          </div>
        ) : (
          renderTab(group.repos[0])
        ),
      )}
      <button type="button" className={styles.addButton} aria-label="Open another repository" onClick={onAddTab}>
        <Plus size={14} aria-hidden="true" />
      </button>
    </div>
  );
}
```

Add to `RepoTabs.module.css`:

```css
.group {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 2px var(--space-1);
  border-right: 1px solid var(--color-border);
  flex: 0 0 auto;
}

.groupHeader {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  font-size: 0.75rem;
  color: var(--color-text-muted);
}

.groupLabel {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.groupTabs {
  display: flex;
  gap: var(--space-1);
}
```

- [ ] **Step 4: Run the `RepoTabs` tests to verify they pass**

Run: `cd frontend && npx vitest run src/components/RepoTabs.test.tsx`
Expected: PASS, all tests.

- [ ] **Step 5: Wire `App.tsx`**

In `frontend/src/App.tsx`:

Add `useWorkspaces` to the imports: `import { useWorkspaces } from "./state/useWorkspaces";`.

Inside `App()`, after `const openRepos = useOpenRepos(tauriRepoClient);`, add:

```tsx
  const workspaces = useWorkspaces(tauriRepoClient);
  const workspaceNames = useMemo(
    () => Object.fromEntries(workspaces.workspaces.map((w) => [w.id, w.name])),
    [workspaces.workspaces],
  );
```

(`useMemo` is already imported in this file for `busyPaths`.)

Update the `<RepoTabs ... />` element to pass the new props:

```tsx
        <RepoTabs
          openRepos={openRepos.openRepos}
          activePath={openRepos.activePath}
          busyPaths={busyPaths}
          workspaceNames={workspaceNames}
          onSwitchTo={openRepos.switchTo}
          onClose={openRepos.closeRepo}
          onCloseGroup={(paths) => paths.forEach((path) => openRepos.closeRepo(path))}
          onAddTab={() => setPickingRepo(true)}
        />
```

Update both `<RepoPicker ... />` elements (the overlay one and the empty-state one) to pass the new workspace props:

```tsx
          <RepoPicker
            client={tauriRepoClient}
            onOpenRepo={(path) => {
              void openRepoTab(path);
              setPickingRepo(false);
            }}
            onOpenWorkspace={(workspace) => {
              void openRepos.openWorkspace(workspace);
              setPickingRepo(false);
            }}
            workspaces={workspaces.workspaces}
            workspacesLoading={workspaces.loading}
            workspacesError={workspaces.error}
            onCreateWorkspace={workspaces.createWorkspace}
            onEditWorkspace={workspaces.editWorkspace}
            onDeleteWorkspace={workspaces.deleteWorkspace}
          />
```

for the overlay instance, and:

```tsx
        <RepoPicker
          client={tauriRepoClient}
          onOpenRepo={openRepoTab}
          onOpenWorkspace={(workspace) => void openRepos.openWorkspace(workspace)}
          workspaces={workspaces.workspaces}
          workspacesLoading={workspaces.loading}
          workspacesError={workspaces.error}
          onCreateWorkspace={workspaces.createWorkspace}
          onEditWorkspace={workspaces.editWorkspace}
          onDeleteWorkspace={workspaces.deleteWorkspace}
        />
```

for the empty-state instance.

- [ ] **Step 6: Type-check and run the full frontend unit suite**

Run: `cd frontend && npx tsc -b && npx eslint . && npx vitest run`
Expected: all pass — this is the point where the shape change from Task 5 is fully threaded through every call site, so `tsc -b` should now be clean.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/RepoTabs.tsx frontend/src/components/RepoTabs.module.css frontend/src/components/RepoTabs.test.tsx frontend/src/App.tsx
git commit -m "feat(frontend): group workspace tabs in RepoTabs, wire workspaces through App"
```

---

## Task 11: E2E — `workspaces.spec.ts`

**Files:**
- Modify: `e2e/wdio.conf.ts`
- Create: `e2e/specs/workspaces.spec.ts`

**Interfaces:**
- Consumes: the full stack from Tasks 1-10.

`RepoPicker`'s native-dialog flows (`pickRepoFolder`, and therefore `WorkspaceEditor`'s create-mode "Choose Root Folder" step) can't be driven through WebDriver — this is the same limitation `multi-repo.spec.ts` works around today by seeding `recent_repos` directly into the E2E `config.toml` rather than clicking "Open Folder". This spec applies the same workaround: seed a `[[workspaces]]` entry directly, so the suite exercises Open All / tab-strip grouping / close-all / Edit (which re-scans an *already-known* root and needs no dialog) / Delete — everything except the initial root-picking click, matching the precedent's own scope.

- [ ] **Step 1: Extend `wdio.conf.ts`'s fixture setup**

In `e2e/wdio.conf.ts`, add near the top with the other path constants:

```ts
const E2E_WORKSPACE_ROOT = path.join(os.tmpdir(), "browsitory-e2e-workspace-root");
const E2E_WORKSPACE_REPO_A = path.join(E2E_WORKSPACE_ROOT, "repo-a");
const E2E_WORKSPACE_REPO_B = path.join(E2E_WORKSPACE_ROOT, "repo-b");
const E2E_WORKSPACE_REPO_C = path.join(E2E_WORKSPACE_ROOT, "repo-c");
```

In `onPrepare`, after the `setupFixtureRepo(E2E_SECOND_REPO_PATH)` block and before the `BROWSITORY_CONFIG_DIR` seeding block, add:

```ts
    // Fixture root for workspaces.spec.ts: three member repos under one root. The saved
    // workspace below deliberately references only repo-a and repo-b as members — repo-c
    // exists on disk but isn't a member yet, so the Edit flow's re-scan has something new to
    // discover (matching production behavior: editing re-scans the root, doesn't just replay
    // the old member list).
    fs.rmSync(E2E_WORKSPACE_ROOT, { recursive: true, force: true });
    fs.mkdirSync(E2E_WORKSPACE_ROOT, { recursive: true });
    for (const repoPath of [E2E_WORKSPACE_REPO_A, E2E_WORKSPACE_REPO_B, E2E_WORKSPACE_REPO_C]) {
      setupFixtureRepo(repoPath);
      execFileSync("git", ["add", "README.md"], { cwd: repoPath, stdio: "inherit" });
      execFileSync("git", ["commit", "-m", "e2e: workspace member base commit"], { cwd: repoPath, stdio: "inherit" });
    }
```

Update the `fs.writeFileSync(path.join(E2E_CONFIG_DIR, "config.toml"), ...)` call to also seed a workspace. Replace it with:

```ts
    fs.writeFileSync(
      path.join(E2E_CONFIG_DIR, "config.toml"),
      `recent_repos = ["${E2E_SECOND_REPO_PATH.replace(/\\/g, "\\\\")}"]\n\n` +
        `[[workspaces]]\n` +
        `id = "e2e-workspace-1"\n` +
        `name = "E2E Workspace"\n` +
        `root_path = "${E2E_WORKSPACE_ROOT.replace(/\\/g, "\\\\")}"\n` +
        `member_paths = ["${E2E_WORKSPACE_REPO_A.replace(/\\/g, "\\\\")}", "${E2E_WORKSPACE_REPO_B.replace(/\\/g, "\\\\")}"]\n`,
    );
```

- [ ] **Step 2: Write `e2e/specs/workspaces.spec.ts`**

```ts
import path from "node:path";
import os from "node:os";
import { expect } from "@wdio/globals";

const E2E_WORKSPACE_ROOT = path.join(os.tmpdir(), "browsitory-e2e-workspace-root");
const E2E_WORKSPACE_REPO_A = path.join(E2E_WORKSPACE_ROOT, "repo-a");
const E2E_WORKSPACE_REPO_B = path.join(E2E_WORKSPACE_ROOT, "repo-b");
const E2E_WORKSPACE_REPO_C = path.join(E2E_WORKSPACE_ROOT, "repo-c");

async function openPickerOverlay(): Promise<void> {
  await browser.execute((el) => (el as HTMLElement).click(), await $('button[aria-label="Open another repository"]'));
}

describe("Browsitory multi-repo workspaces", () => {
  it("opens all workspace members grouped under a chip, then closes the whole group at once", async () => {
    await openPickerOverlay();

    const openAllButton = await $('button=Open All');
    await openAllButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), openAllButton);

    await browser.waitUntil(
      async () => (await $$('[role="tab"]')).length >= 2,
      { timeout: 10000, timeoutMsg: "expected the workspace's two members to open as tabs" },
    );

    const groupLabel = await $('span*=E2E Workspace');
    await groupLabel.waitForExist({ timeout: 10000 });

    const repoATab = await $(`button[title="${E2E_WORKSPACE_REPO_A}"]`);
    const repoBTab = await $(`button[title="${E2E_WORKSPACE_REPO_B}"]`);
    await repoATab.waitForExist({ timeout: 10000 });
    await repoBTab.waitForExist({ timeout: 10000 });

    const closeGroupButton = await $('button[aria-label="Close E2E Workspace"]');
    await browser.execute((el) => (el as HTMLElement).click(), closeGroupButton);

    await browser.waitUntil(
      async () => !(await $(`button[title="${E2E_WORKSPACE_REPO_A}"]`).catch(() => null))?.isExisting?.(),
      { timeout: 10000, timeoutMsg: "expected both workspace member tabs to close" },
    );
    expect(await $(`button[title="${E2E_WORKSPACE_REPO_A}"]`).isExisting()).toBe(false);
    expect(await $(`button[title="${E2E_WORKSPACE_REPO_B}"]`).isExisting()).toBe(false);
  });

  it("Edit re-scans the root, pre-checking current members and offering the newly-found repo unchecked", async () => {
    await openPickerOverlay();

    const editButton = await $('button=Edit');
    await editButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), editButton);

    const repoCCheckbox = await $(`input[aria-label="${E2E_WORKSPACE_REPO_C}"]`);
    await repoCCheckbox.waitForExist({ timeout: 10000 });
    expect(await repoCCheckbox.isSelected()).toBe(false);

    const repoACheckbox = await $(`input[aria-label="${E2E_WORKSPACE_REPO_A}"]`);
    expect(await repoACheckbox.isSelected()).toBe(true);

    await browser.execute((el) => (el as HTMLElement).click(), repoCCheckbox);
    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Save'));

    // Re-open the picker and Edit again: the saved membership should now include repo-c.
    await openPickerOverlay();
    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Edit'));
    const repoCCheckboxAfterSave = await $(`input[aria-label="${E2E_WORKSPACE_REPO_C}"]`);
    await repoCCheckboxAfterSave.waitForExist({ timeout: 10000 });
    expect(await repoCCheckboxAfterSave.isSelected()).toBe(true);
    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Cancel'));
  });

  it("Delete removes the workspace from the list after confirmation", async () => {
    await openPickerOverlay();

    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Delete E2E Workspace'));
    const confirmDialog = await $('dialog[aria-label="Delete workspace E2E Workspace"]');
    await confirmDialog.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Delete workspace'));

    await browser.waitUntil(
      async () => !(await $('span*=E2E Workspace').isExisting()),
      { timeout: 10000, timeoutMsg: "expected the workspace to be removed from the list" },
    );
  });
});
```

- [ ] **Step 3: Build and run the full test suite**

Run: `cargo build --workspace`
Run: `cd frontend && VITE_E2E_REPO_PATH=$(node -e "console.log(require('node:path').join(require('node:os').tmpdir(),'browsitory-e2e-repo'))") npx vite build`
Run: `cargo build --workspace` (again, to pick up the freshly-built `frontend/dist` — matches the existing E2E build order documented in `wdio.conf.ts`'s header comment)
Run: `cd e2e && xvfb-run --auto-servernum npx wdio run wdio.conf.ts`
Expected: PASS — the full suite (`multi-repo.spec.ts`'s existing tests plus the three new `workspaces.spec.ts` tests) passes.

- [ ] **Step 4: Commit**

```bash
git add e2e/wdio.conf.ts e2e/specs/workspaces.spec.ts
git commit -m "test(e2e): add workspaces spec covering open-all, grouping, edit, and delete"
```

---

## Task 12: Whole-branch verification

**Files:** none (verification only).

- [ ] **Step 1: Run the full Rust test suite**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 2: Run the full frontend suite**

Run: `cd frontend && npx tsc -b && npx eslint . && npx vitest run`
Expected: PASS.

- [ ] **Step 3: Run the full E2E suite**

Run: `cd e2e && xvfb-run --auto-servernum npx wdio run wdio.conf.ts`
Expected: PASS (15+ existing specs plus the 3 new ones).

- [ ] **Step 4: Review the diff against the spec**

Read through `git diff main --stat` and re-check each spec section in
`docs/superpowers/specs/2026-08-21-multi-repo-workspaces-design.md` against the implementation.
Confirm every Goal is met and every Non-goal was left alone (no drag-reorder was added, no
recursive scan, no auto re-sync).

- [ ] **Step 5: Commit if any fixes were needed**

If Steps 1-4 required any fixes, commit them:

```bash
git add -A
git commit -m "fix: address whole-branch verification findings for multi-repo workspaces"
```
