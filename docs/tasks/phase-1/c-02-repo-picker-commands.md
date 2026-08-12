# Task 1.C.02: Tauri commands for repo picking + recent repos

## Goal

Add `pick_repo_folder` (native OS folder picker) and `list_recent_repos` Tauri commands, and make
the existing `open_repo` command record every successfully opened repo into the recent-repos
list. These don't go through `Worker` — no repo needs to be open yet to pick one or list recents.

## Depends on

1.B.01 (`config::list_recent_repos`/`add_recent_repo`).

## Interfaces produced

Two new `#[tauri::command]` functions in `crates/tauri-app/src/commands.rs`:
`pick_repo_folder(app: tauri::AppHandle) -> Option<String>`,
`list_recent_repos() -> Result<Vec<String>, String>`. The existing `open_repo` command's
behavior gains a side effect (still the same signature: `open_repo(path: String, state:
State<AppState>) -> Result<(), String>`). Task 1.D.01 (`RepoClient`) calls all three.

## Implementation notes

**`crates/tauri-app/Cargo.toml`** gains a dependency:
```toml
tauri-plugin-dialog = "2"
```

**`crates/tauri-app/capabilities/default.json`** (new file — Tauri v2 requires an explicit
capability grant for plugin commands; core app commands like `open_repo`/`get_status` don't need
this, but `tauri-plugin-dialog`'s commands do). Tauri auto-discovers `capabilities/*.json` next
to `tauri.conf.json`, no registration step needed beyond creating the file:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default permissions for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:default"
  ]
}
```

**`crates/tauri-app/src/main.rs`** registers the plugin — add `.plugin(tauri_plugin_dialog::init())`
to the builder chain, before `.invoke_handler(...)`:
```rust
tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .manage(AppState::default())
    .invoke_handler(tauri::generate_handler![
        open_repo,
        get_status,
        get_log,
        get_working_diff,
        get_commit_diff,
        get_commit_files,
        stage_file,
        unstage_file,
        commit,
        pick_repo_folder,
        list_recent_repos,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```
(This task assumes 1.C.01 already landed the six git-operation commands listed above; if this
task is implemented before 1.C.01, only add the entries this task introduces —
`pick_repo_folder`, `list_recent_repos` — to whatever `generate_handler!` list currently exists.)

**`crates/tauri-app/src/commands.rs`** — its existing `use std::path::PathBuf;` becomes
`use std::path::{Path, PathBuf};` (the modified `open_repo` below needs `Path::new`):
```rust
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn pick_repo_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .map(|path| path.to_string())
}

#[tauri::command]
pub fn list_recent_repos() -> Result<Vec<String>, String> {
    config::list_recent_repos()
        .map(|paths| {
            paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_repo(path: String, state: State<AppState>) -> Result<(), String> {
    let worker = Worker::spawn(PathBuf::from(&path))?;
    *state.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(worker);
    // Best-effort: a repo that opened successfully should count as "recent" even if we can't
    // persist that fact (e.g. an unwritable config dir) — don't fail the whole open_repo call
    // over it.
    let _ = config::add_recent_repo(Path::new(&path));
    Ok(())
}
```
`crates/tauri-app/Cargo.toml` also needs `config = { path = "../config" }` added under
`[dependencies]` if it isn't there already (it isn't — `tauri-app` has never depended on
`config` before this task). `commands.rs` calls it as `config::list_recent_repos()` /
`config::add_recent_repo(...)` — the crate name `config` is already in scope for any Rust 2018+
edition crate once it's a `Cargo.toml` dependency, no `use` needed for the crate root itself.

`app.dialog().file().blocking_pick_folder()` is `tauri-plugin-dialog`'s synchronous
folder-picker convenience API, meant for exactly this use inside a `#[tauri::command]` handler
(the async/callback-based `pick_folder(callback)` variant is for contexts outside a command,
where blocking isn't an option). It returns `Option<FilePath>`; `FilePath` implements
`Display`, so `.to_string()` gives the path. **Verify this exact method name/return type
against the installed crate version** (`cargo doc -p tauri-plugin-dialog --open`, or
docs.rs/tauri-plugin-dialog) once it's added to `Cargo.lock` — plugin APIs shift between minor
versions more than core `tauri`/`git2`, and this is the one piece of this task not cross-checked
against a locally vendored source copy the way the rest of this plan's `git2` code was.

## TDD requirement

No new automated tests this task — `pick_repo_folder` opens a real native OS dialog, which
can't run in `cargo test`'s headless environment, and `open_repo`'s new recent-repos side effect
writes to the real OS config directory (not a tempdir), so it isn't safely testable in the
existing unit-test setup either. This is consistent with `open_repo`'s existing coverage: it has
no dedicated test today (Phase 0's `worker.rs` tests exercise `Worker::spawn` directly, not the
Tauri command wrapper), and 1.C.02 doesn't change that.

Two things this task's implementer must still verify manually, and note in their report:
- `cargo build --workspace` succeeds with `tauri-plugin-dialog` and `config` as new
  dependencies of `tauri-app`.
- `cargo tauri dev`, clicking whatever currently triggers `pick_repo_folder` (nothing does yet —
  Task 1.E.01 wires up the `RepoPicker` UI; for this task, a temporary manual check is calling
  it via the Tauri devtools console: `window.__TAURI__.core.invoke('pick_repo_folder')`) opens a
  real native folder picker and returns the chosen path.

## Acceptance criteria

- [ ] `cargo build --workspace` succeeds.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo fmt --all -- --check` clean.
- [ ] Manual check above (native folder picker opens via `cargo tauri dev` + devtools `invoke`)
      performed and noted in the task report.
- [ ] `docs/LICENSE_COMPLIANCE.md` gains a row for `tauri-plugin-dialog` (verify its license via
      `cargo info tauri-plugin-dialog` against the installed version — expected MIT/Apache-2.0,
      confirm rather than assume).
- [ ] Commit: `git add crates/tauri-app/Cargo.toml crates/tauri-app/capabilities crates/tauri-app/src/main.rs crates/tauri-app/src/commands.rs docs/LICENSE_COMPLIANCE.md && git commit -m "feat(tauri-app): add repo-picker dialog and recent-repos commands"`.

## Out of scope

Removing a repo from the recent list (no "remove" UI this phase — a stale/deleted path just
fails to open if picked again, handled as an ordinary `open_repo` error). Any other native
dialog (save-file, message boxes) — folder-picker only.
