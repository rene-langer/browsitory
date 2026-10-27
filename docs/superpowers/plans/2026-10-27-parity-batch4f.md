# Feature Parity Batch 4f (External Tools) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open the user's editor, file manager, terminal, git diff tool and git merge tool from file rows, commit file sections, repo tabs and the palette, offer "Mark resolved" after a merge tool, and add a Preferences dialog with an "External tools" section. Both hosts are covered: the desktop app spawns programs, and the VSCode extension uses VSCode APIs.

**Architecture:**
- **`git-core`** gets three new modules:
  - `process`: command-line splitting, program lookup, detached spawn. Shared with 4e.
  - `launch`: pure editor, terminal and reveal resolution.
  - `tools`: git `diff.tool`/`merge.tool` resolution with presets, file versions from `git2`, temp side files, launch and wait.

  `merge::mark_resolved` is new too.
- **`config`** gets a `[tools]` table.
- **`repo-service`** prepares tool launches on the repo worker (config + temp files). The Tauri command spawns, and for the merge tool waits on `spawn_blocking`, so the worker is never held while a tool window is open.
- **VSCode:** the extension host answers the five launch methods natively. It reads repository file contents through a `browsitory-file:` content provider backed by an internal sidecar method.
- **Frontend:** `useExternalTools` + menus + `PreferencesDialog`.

**Tech Stack:** Rust (`git2` 0.21, `tempfile` 3, `thiserror`, `tauri-plugin-opener` 2.5 already present), React + TypeScript, Vitest + Testing Library, VSCode extension API.

**Spec:** `docs/superpowers/specs/2026-10-27-parity-batch4f-design.md`

## Global Constraints

- No new Rust or npm dependency. `docs/LICENSE_COMPLIANCE.md` is untouched. Reveal and open-with-default come from `tauri_plugin_opener::{reveal_item_in_dir, open_path}`, which are already in the dependency tree.
- The editor and terminal commands are never run through a shell: `process::split_command_line`, then placeholders per word. Placeholders are exactly `{file}`, `{line}` and `{dir}`.
- A git tool `cmd` runs as `sh -c <cmd>` with env vars `LOCAL`, `REMOTE`, `MERGED` and `BASE`, with cwd = work tree root. On Windows without Git's `sh.exe`, fall back to split + `expand_tool_vars` + direct spawn.
- Presets, exactly: `meld`, `kdiff3`, `bc`, `bc3`, `bc4`, `p4merge`, `vscode`, `winmerge` (argv in the spec's table). Terminal tools, exactly: `vimdiff`, `vimdiff1`, `vimdiff2`, `vimdiff3`, `nvimdiff`, `nvimdiff1`, `nvimdiff2`, `nvimdiff3`, `emerge`.
- Temp root: `std::env::temp_dir().join("browsitory-tools")`. One `tempfile` dir per launch. Side names are `<stem>_LOCAL.<ext>`, `<stem>_REMOTE.<ext>` and `<stem>_BASE.<ext>`. Sweep age 24 h. Diff tool grace: 1 s. Stderr tail: 4096 bytes.
- `MAX_FILE_VERSION_BYTES = 20 * 1024 * 1024`. It applies only to `read_file_version` (VSCode content provider).
- Error copy, exactly as in the spec's `ToolError`/`OpenError`/`SplitError`/`LaunchError` blocks.
- UI copy, exactly:
  - menu items: `Open in editor`, `Reveal in file manager`, `Open in diff tool`, `Open in merge tool`
  - disabled title: `The file was deleted`
  - tab menu: `Open terminal here`, `Reveal repository`
  - palette: `Preferences…`, `Open terminal here`, `Reveal repository`, `Open current file in editor`, `Open current file in diff tool`
  - header gear aria-label: `Preferences`
  - dialog aria-label and heading: `Preferences`
  - section legend: `External tools`
  - field labels: `Editor command`, `Terminal command`
  - placeholders: `System default app`, `System default terminal`
  - hint: `Placeholders: {file}, {line}, {dir}`
  - buttons: `Save`, `Close`
  - rows: `Diff tool`, `Merge tool`
  - not configured: `Not configured`
  - VSCode note: `In VSCode, files, diffs, merges and terminals open in VSCode itself.`
  - notices: `The merge tool finished.`, `The merge tool closed without resolving the conflict.`
  - notice buttons: `Mark resolved`, `Dismiss`
  - confirm: `Conflict markers remain in PATH. Mark it resolved anyway?` with confirm label `Mark resolved`
- Wire format (camelCase params, Rust variant names for enums):
  - `open_in_editor {repoPath, path, line}` → `null`
  - `reveal_in_file_manager {repoPath, path}` → `null`
  - `open_terminal {repoPath}` → `null`
  - `open_diff_tool {repoPath, path, source}` → `null`
  - `open_merge_tool {repoPath, path}` → `"Resolved"|"Unresolved"` (VSCode adds `"HandedOff"`)
  - `mark_resolved {repoPath, path, force}` → `"Resolved"|"MarkersRemain"`
  - `get_external_tool_settings {}` → `{editor, terminal}`
  - `set_external_tool_settings {settings}` → `null`
  - `get_git_tools {repoPath}` → `{diffTool, mergeTool}`
  - internal sidecar methods: `read_file_version {repoPath, path, version}` → `{text}` and `repo_workdir {repoPath}` → `string`
- No undo entries. Do not add arms to `undo::plan_for` or `undo::hold_reply`.
- Tests: `git-core`/`repo-service`/`config` use real temp dirs and repos, and git config through real `git2::Config` files. The frontend mocks `RepoClient` only. Process spawning is tested only behind `#[cfg(unix)]` with `sh` builtins. No E2E.
- Every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook). The changelog lands in Task 15, so push only after Task 15.

## Review Focus

1. **A file in the repo that would execute when "opened" with no editor configured** (`setup.exe`, `x.desktop`, a `chmod +x` script) must be refused, never handed to the OS default handler. Pinned in Task 2 (`may_execute_*`, `editor_target_refuses_an_executable_without_an_editor`).
2. **Paths from IPC** (`../outside`, `/etc/passwd`, `C:\x`) must never escape the work tree in open, reveal or tool launches. Pinned in Task 2 (`resolve_in_workdir_rejects_escapes`) and Task 4 (`prepare_rejects_an_escaping_path`).
3. **Quoting.** `"C:\Program Files\Sublime Text\subl.exe" {file}:{line}` and an editor path with spaces must reach the program as single arguments. git `cmd`s with `"$LOCAL"` must keep git's quoting because the shell does the expansion. Pinned in Task 1 and Task 3.
4. **The repo worker is never blocked by an open merge tool.** The worker only prepares. Tauri waits in `spawn_blocking`. VSCode never waits, because it hands off to the merge editor. Pinned by review of Task 8's command body.
5. **Mark resolved never silently stages conflict markers.** Pinned in Task 5 (`mark_resolved_reports_markers_and_keeps_the_conflict`).

---

## Pre-flight (before Task 1)

Batches 4b–4e land before this one, so names in this plan may have moved.

- [ ] **Step 1: Rebase on main**

```bash
git fetch origin && git rebase origin/main
```

- [ ] **Step 2: Re-check named code and record any drift at the top of this plan as a "Pre-flight notes" list**

Run each and compare with the expectation:

```bash
graft skeleton crates/git-core/src/lib.rs            # module list; is there a process/hooks module from 4e?
grep -rn "fn find_sh\|fn find_git_sh\|process_group\|CREATE_NO_WINDOW" crates/git-core/src   # 4e spawn helpers
grep -n "pub(crate) fn run_program\|fn stderr_tail\|fn config_string" crates/git-core/src/signing.rs
grep -n "fn find_conflict\|pub fn resolve_conflict\|pub enum MergeError" crates/git-core/src/merge.rs
grep -rn "mark_resolved\|MarkResolved\|DiffSource\|Preferences" crates frontend/src extension/src
grep -n "pub struct WorkerHandle" -A8 crates/repo-service/src/worker/mod.rs
grep -n "struct ConfigFile" -A25 crates/config/src/lib.rs
grep -n "NATIVE_METHODS" -A10 extension/src/sidecarBridge.ts
grep -n "export function buildCommands" -A20 frontend/src/lib/commands.ts
grep -n "function CommitFileSection\|function UncommittedFileSection" frontend/src/components/DiffPane.tsx
grep -rln "getRangeDiff\|RangeFileSection\|Compare these commits" frontend/src   # 4d compare view
```

Act on what you find:
- **4e added spawn helpers** (for example `hooks::find_sh`, a `process.rs`): put Task 1's functions in that module and reuse its names. Change the paths in Tasks 1–4 consistently.
- **4d added a type named `DiffSource`**: name this batch's frontend/Rust type `ToolDiffSource` everywhere.
- **4d's compare pane renders its own file sections**: note the component name. Task 13 Step 5 adds the commit-section menu to it with a `Range` source.
- **Someone already added `mark_resolved`**: stop and reconcile with the spec's "Overlaps" section before continuing.
- **`buildCommands` gained trailing parameters**: `toolCommands` goes after the last one.

- [ ] **Step 3: Baseline is green**

```bash
cargo test --workspace && (cd frontend && pnpm test -- --run) && (cd extension && pnpm test -- --run)
```
Expected: PASS. If not, stop and report. Do not build on a red baseline.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/git-core/src/process.rs` (new) | `CommandStyle`, `split_command_line`, `find_program(_in)`, `find_git_sh`, `git_sh_candidates`, `SpawnSpec`, `spawn`, `spawn_detached`, `drain_tail`, `tail_text`, `LaunchError`, `SplitError` + inline tests |
| `crates/git-core/src/launch.rs` (new) | `Os`, `OpenError`, `Invocation`, `EditorTarget`, `resolve_in_workdir`, `nearest_existing`, `expand_placeholders`, `may_execute`, `has_exec_bit`, `editor_invocation`, `editor_target`, `default_terminal`, `terminal_invocation`, `prepare_editor`, `prepare_terminal`, `reveal_target`, `run_invocation` + inline tests |
| `crates/git-core/src/tools.rs` (new) | `ToolKind`, `ConfigSource`, `ToolCommand`, `ResolvedTool`, `ToolVars`, presets, `resolve_tool`, `expand_tool_vars`, `git_tools`, `DiffSource`, `FileVersion`, `file_version_contents`, `PreparedDiffTool`, `PreparedMergeTool`, `prepare_*`, `sweep_stale_tool_dirs`, `launch_diff_tool`, `run_merge_tool`, `ToolError` |
| `crates/git-core/src/merge.rs` | `MarkResolvedOutcome`, `mark_resolved`, `has_conflict_markers` |
| `crates/git-core/src/lib.rs` | `pub mod launch; pub mod process; pub mod tools;` |
| `crates/git-core/tests/tools.rs` (new), `tests/merge.rs` | real-repo tests |
| `crates/config/src/lib.rs`, `crates/config/tests/external_tool_settings.rs` (new) | `[tools]` table |
| `crates/repo-service/src/worker/tools.rs` (new), `worker/merge.rs`, `worker/mod.rs` | worker commands, `WorkerHandle::workdir` |
| `crates/tauri-app/src/commands/tools.rs` (new), `commands/merge.rs`, `commands/mod.rs`, `main.rs` | commands + DTOs + DTO tests |
| `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs` | methods + round trips |
| `extension/src/toolUris.ts` (+ test) (new), `sidecarBridge.ts` (+ test), `extension.ts` | native launch methods, content provider |
| `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts` (+ tests, fixtures) | types + 9 methods |
| `frontend/src/state/useExternalTools.ts` (+ test) (new), `useExternalToolSettings.ts` (+ test) (new), `useMergeRebaseActions.ts`, `useAppState.ts` | tool actions, settings, `markResolved` |
| `frontend/src/components/FileActionsMenu.tsx` (+ test), `DiffPane.tsx` (+ test), `DiffPane.module.css`, `RepoTabs.tsx` (+ test) | menus, merge-tool notice |
| `frontend/src/components/PreferencesDialog.tsx`, `.module.css`, `.test.tsx` (new) | dialog shell, `PreferencesSection`, `ExternalToolsSection` |
| `frontend/src/lib/commands.ts` (+ test), `App.tsx` (+ test), `vscode-main.tsx` | palette, wiring, `toolHost` |
| `CHANGELOG.md`, `docs/USER_GUIDE.md`, `docs/ARCHITECTURE.md` | docs |

---

### Task 1: `git-core::process`: splitting, lookup, spawn

**Files:**
- Create: `crates/git-core/src/process.rs`
- Modify: `crates/git-core/src/lib.rs` (add `pub mod process;` between `pub mod pick;` and `pub mod rebase;`)

**Interfaces:**
- Produces (Tasks 2–4, 8): everything in the spec's `process.rs` block. `SpawnSpec` derives `Debug, Clone`.

- [ ] **Step 1: Write the failing tests** (bottom of `process.rs`; the module starts with the type definitions and `todo!()` bodies so it compiles)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use CommandStyle::{Posix, Windows};

    fn split(input: &str, style: CommandStyle) -> Vec<String> {
        split_command_line(input, style).unwrap()
    }

    #[test]
    fn posix_splits_on_whitespace_and_honours_quotes_and_escapes() {
        assert_eq!(split("code  --goto \"{file}:{line}\"", Posix), ["code", "--goto", "{file}:{line}"]);
        assert_eq!(
            split(r#"'a b' c\ d "e\"f" "g\h" '\n'"#, Posix),
            ["a b", "c d", "e\"f", "g\\h", "\\n"]
        );
        assert_eq!(split(r#"x "" y"#, Posix), ["x", "", "y"]);
    }

    #[test]
    fn posix_rejects_empty_and_unclosed_quotes() {
        assert_eq!(split_command_line("   ", Posix), Err(SplitError::Empty));
        assert_eq!(split_command_line("\"abc", Posix), Err(SplitError::UnclosedQuote));
        assert_eq!(split_command_line("'abc", Posix), Err(SplitError::UnclosedQuote));
    }

    #[test]
    fn windows_keeps_backslashes_literal_except_before_quotes() {
        assert_eq!(
            split(r#""C:\Program Files\Sublime Text\subl.exe" {file}:{line}"#, Windows),
            [r"C:\Program Files\Sublime Text\subl.exe", "{file}:{line}"]
        );
        assert_eq!(split(r#"a\\\"b"#, Windows), [r#"a\"b"#]);
        assert_eq!(split(r#""a\\" b"#, Windows), [r"a\", "b"]);
        assert_eq!(split(r"C:\dir\ x", Windows), [r"C:\dir\", "x"]);
        assert_eq!(split(r#""say ""hi""""#, Windows), [r#"say "hi""#]);
        assert_eq!(split_command_line("\"abc", Windows), Err(SplitError::UnclosedQuote));
    }

    #[test]
    fn find_program_in_searches_dirs_and_tries_extensions() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("tool"), "").unwrap();
        std::fs::write(dir.path().join("code.cmd"), "").unwrap();
        let dirs = vec![PathBuf::from("/definitely/missing"), dir.path().to_path_buf()];
        assert_eq!(find_program_in("tool", &dirs, &[]), Some(dir.path().join("tool")));
        let exts = vec![".EXE".to_string(), ".cmd".to_string()];
        assert_eq!(find_program_in("code", &dirs, &exts), Some(dir.path().join("code.cmd")));
        assert_eq!(find_program_in("nope", &dirs, &exts), None);
        let explicit = dir.path().join("tool");
        assert_eq!(find_program_in(explicit.to_str().unwrap(), &[], &[]), Some(explicit));
    }

    #[test]
    fn git_sh_candidates_look_next_to_git_for_windows() {
        let git = Path::new("C:/Program Files/Git/cmd/git.exe");
        assert_eq!(
            git_sh_candidates(git),
            vec![
                PathBuf::from("C:/Program Files/Git/bin/sh.exe"),
                PathBuf::from("C:/Program Files/Git/usr/bin/sh.exe"),
                PathBuf::from("C:/Program Files/Git/cmd/sh.exe"),
            ]
        );
    }

    #[test]
    fn drain_tail_keeps_only_the_last_bytes() {
        let input: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let (tail, reader) = drain_tail(std::io::Cursor::new(input.clone()), 4096);
        reader.join().unwrap();
        assert_eq!(*tail.lock().unwrap(), input[input.len() - 4096..].to_vec());
    }

    #[test]
    fn tail_text_keeps_the_last_three_non_empty_lines() {
        assert_eq!(tail_text(b"a\n\nb\nc\n  d  \n\n"), "b\nc\nd");
        assert_eq!(tail_text(b""), "no output");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --lib process::`
Expected: FAIL (panics at `todo!()`).

- [ ] **Step 3: Implement**

```rust
//! Spawning external programs that are not git: editors, terminals, diff/merge tools. See
//! `docs/superpowers/specs/2026-10-27-parity-batch4f-design.md`. Nothing here goes through a
//! shell unless the caller asks for `sh` explicitly.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStyle {
    Posix,
    Windows,
}

impl CommandStyle {
    pub fn native() -> Self {
        if cfg!(windows) { Self::Windows } else { Self::Posix }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SplitError {
    #[error("The command is empty.")]
    Empty,
    #[error("The command has an unclosed quote.")]
    UnclosedQuote,
}

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("Couldn't find the program '{0}'.")]
    NotFound(String),
    #[error(transparent)]
    Split(#[from] SplitError),
    #[error("Couldn't start '{program}': {source}")]
    Io { program: String, source: std::io::Error },
}

pub fn split_command_line(input: &str, style: CommandStyle) -> Result<Vec<String>, SplitError> {
    let words = match style {
        CommandStyle::Posix => split_posix(input)?,
        CommandStyle::Windows => split_windows(input)?,
    };
    if words.is_empty() {
        return Err(SplitError::Empty);
    }
    Ok(words)
}

fn split_posix(input: &str) -> Result<Vec<String>, SplitError> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => current.push(c),
                        None => return Err(SplitError::UnclosedQuote),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(c @ ('"' | '\\' | '$' | '`')) => current.push(c),
                            Some(c) => {
                                current.push('\\');
                                current.push(c);
                            }
                            None => return Err(SplitError::UnclosedQuote),
                        },
                        Some(c) => current.push(c),
                        None => return Err(SplitError::UnclosedQuote),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(c) = chars.next() {
                    current.push(c);
                }
            }
            c => {
                in_word = true;
                current.push(c);
            }
        }
    }
    if in_word {
        words.push(current);
    }
    Ok(words)
}

/// `CommandLineToArgvW` rules: 2n backslashes + `"` → n backslashes and a quote toggle; 2n+1
/// backslashes + `"` → n backslashes and a literal `"`; other backslashes are literal; `""`
/// inside quotes is a literal `"`. An unclosed quote is an error here (Windows tolerates it).
fn split_windows(input: &str) -> Result<Vec<String>, SplitError> {
    let chars: Vec<char> = input.chars().collect();
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quoted = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let start = i;
            while i < chars.len() && chars[i] == '\\' {
                i += 1;
            }
            let n = i - start;
            in_word = true;
            if i < chars.len() && chars[i] == '"' {
                current.extend(std::iter::repeat('\\').take(n / 2));
                if n % 2 == 1 {
                    current.push('"');
                    i += 1;
                }
            } else {
                current.extend(std::iter::repeat('\\').take(n));
            }
            continue;
        }
        if c == '"' {
            in_word = true;
            if quoted && chars.get(i + 1) == Some(&'"') {
                current.push('"');
                i += 2;
                continue;
            }
            quoted = !quoted;
            i += 1;
            continue;
        }
        if c.is_whitespace() && !quoted {
            if in_word {
                words.push(std::mem::take(&mut current));
                in_word = false;
            }
            i += 1;
            continue;
        }
        in_word = true;
        current.push(c);
        i += 1;
    }
    if quoted {
        return Err(SplitError::UnclosedQuote);
    }
    if in_word {
        words.push(current);
    }
    Ok(words)
}

pub fn find_program(name: &str) -> Option<PathBuf> {
    let dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };
    find_program_in(name, &dirs, &exts)
}

pub fn find_program_in(name: &str, dirs: &[PathBuf], exts: &[String]) -> Option<PathBuf> {
    let candidates = |base: PathBuf| -> Vec<PathBuf> {
        let mut out = Vec::new();
        if exts.is_empty() || base.extension().is_some() {
            out.push(base.clone());
        }
        for ext in exts {
            let mut with_ext = base.clone().into_os_string();
            with_ext.push(ext);
            out.push(PathBuf::from(with_ext));
        }
        out
    };
    if name.contains('/') || name.contains('\\') {
        return candidates(PathBuf::from(name)).into_iter().find(|p| p.is_file());
    }
    dirs.iter()
        .flat_map(|dir| candidates(dir.join(name)))
        .find(|p| p.is_file())
}

pub fn git_sh_candidates(git_exe: &Path) -> Vec<PathBuf> {
    let Some(bin) = git_exe.parent() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(root) = bin.parent() {
        out.push(root.join("bin").join("sh.exe"));
        out.push(root.join("usr").join("bin").join("sh.exe"));
    }
    out.push(bin.join("sh.exe"));
    out
}

pub fn find_git_sh() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let git = find_program("git")?;
    git_sh_candidates(&git).into_iter().find(|p| p.is_file())
}

#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, OsString)>,
    pub new_console: bool,
    pub no_window: bool,
    pub capture_stderr: bool,
}

pub fn spawn(spec: &SpawnSpec) -> Result<Child, LaunchError> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(if spec.capture_stderr { Stdio::piped() } else { Stdio::null() });
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut flags = CREATE_NEW_PROCESS_GROUP;
        if spec.new_console {
            flags |= CREATE_NEW_CONSOLE;
        } else if spec.no_window {
            flags |= CREATE_NO_WINDOW;
        }
        command.creation_flags(flags);
    }
    command.spawn().map_err(|source| {
        let program = spec.program.display().to_string();
        if source.kind() == std::io::ErrorKind::NotFound {
            LaunchError::NotFound(program)
        } else {
            LaunchError::Io { program, source }
        }
    })
}

/// Spawns and reaps on a background thread, so no zombie is left behind.
pub fn spawn_detached(spec: &SpawnSpec) -> Result<(), LaunchError> {
    let mut child = spawn(spec)?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub fn drain_tail(
    mut source: impl Read + Send + 'static,
    cap: usize,
) -> (Arc<Mutex<Vec<u8>>>, JoinHandle<()>) {
    let tail = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&tail);
    let handle = std::thread::spawn(move || {
        let mut chunk = [0u8; 1024];
        loop {
            match source.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut buf = sink.lock().unwrap_or_else(|e| e.into_inner());
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() > cap {
                        let excess = buf.len() - cap;
                        buf.drain(..excess);
                    }
                }
            }
        }
    });
    (tail, handle)
}

pub fn tail_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let start = lines.len().saturating_sub(3);
    let tail = lines[start..].join("\n");
    if tail.is_empty() { "no output".to_string() } else { tail }
}
```

- [ ] **Step 4: Run to verify pass, plus clippy**

Run: `cargo test -p git-core --lib process:: && cargo clippy -p git-core --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/process.rs crates/git-core/src/lib.rs
git commit -m "feat(git-core): process helpers for launching external programs"
```

---

### Task 2: `git-core::launch`: editor, terminal, reveal

**Files:**
- Create: `crates/git-core/src/launch.rs`
- Modify: `crates/git-core/src/lib.rs` (add `pub mod launch;` between `mod line_patch;` and `pub mod merge;`)

**Interfaces:**
- Consumes: `process::{split_command_line, CommandStyle, SplitError, LaunchError, find_program, SpawnSpec, spawn_detached}` (Task 1).
- Produces (Task 8):

```rust
pub enum Os { Linux, MacOs, Windows }            // Debug, Clone, Copy, PartialEq, Eq; Os::current()
pub enum OpenError { InvalidPath, Missing(String), MayExecute(String), NoTerminal, Launch(LaunchError) }
pub struct Invocation { pub argv: Vec<String>, pub new_console: bool }   // Debug, Clone, PartialEq, Eq
pub enum EditorTarget { Command(Invocation), SystemDefault(PathBuf) }   // Debug, PartialEq, Eq
pub fn resolve_in_workdir(workdir: &Path, rel: &str) -> Result<PathBuf, OpenError>;
pub fn nearest_existing(workdir: &Path, target: &Path) -> PathBuf;
pub fn expand_placeholders(word: &str, file: Option<&Path>, line: Option<u32>, dir: &Path) -> String;
pub fn may_execute(path: &Path, os: Os, has_exec_bit: bool) -> bool;
pub fn has_exec_bit(path: &Path) -> bool;
pub fn editor_invocation(command: &str, style: CommandStyle, file: &Path, line: Option<u32>, dir: &Path) -> Result<Invocation, SplitError>;
pub fn editor_target(configured: Option<&str>, os: Os, style: CommandStyle, file: &Path, line: Option<u32>, dir: &Path, has_exec_bit: bool) -> Result<EditorTarget, OpenError>;
pub fn default_terminal(os: Os, env_terminal: Option<&str>, has_program: &dyn Fn(&str) -> bool) -> Option<Invocation>;
pub fn terminal_invocation(configured: Option<&str>, os: Os, style: CommandStyle, env_terminal: Option<&str>, has_program: &dyn Fn(&str) -> bool, dir: &Path) -> Result<Invocation, OpenError>;
/// Real-environment wrappers used by tauri-app:
pub fn prepare_editor(workdir: &Path, rel: &str, line: Option<u32>, configured: Option<&str>) -> Result<EditorTarget, OpenError>;
pub fn prepare_terminal(workdir: &Path, configured: Option<&str>) -> Result<Invocation, OpenError>;
pub fn reveal_target(workdir: &Path, rel: Option<&str>) -> Result<PathBuf, OpenError>;
pub fn run_invocation(invocation: &Invocation, cwd: &Path) -> Result<(), OpenError>;
```

- [ ] **Step 1: Write the failing tests** (inline, bottom of `launch.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::CommandStyle::{Posix, Windows};

    fn argv(inv: &Invocation) -> Vec<&str> {
        inv.argv.iter().map(String::as_str).collect()
    }

    #[test]
    fn resolve_in_workdir_rejects_escapes() {
        let root = Path::new("/repo");
        assert_eq!(resolve_in_workdir(root, "src/a b.rs").unwrap(), root.join("src/a b.rs"));
        for bad in ["", "../outside", "src/../../x", "/etc/passwd", "./a"] {
            assert!(matches!(resolve_in_workdir(root, bad), Err(OpenError::InvalidPath)), "{bad}");
        }
    }

    #[test]
    fn nearest_existing_walks_up_but_never_out_of_the_workdir() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        assert_eq!(nearest_existing(dir.path(), &dir.path().join("src/gone/x.rs")), dir.path().join("src"));
        assert_eq!(nearest_existing(dir.path(), &dir.path().join("gone.rs")), dir.path());
    }

    #[test]
    fn placeholders_expand_inside_words() {
        let file = Path::new("/r/my file.rs");
        let dir = Path::new("/r");
        assert_eq!(expand_placeholders("{file}:{line}", Some(file), Some(12), dir), "/r/my file.rs:12");
        assert_eq!(expand_placeholders("--line={line}", Some(file), None, dir), "--line=1");
        assert_eq!(expand_placeholders("--cwd={dir}", None, None, dir), "--cwd=/r");
        assert_eq!(expand_placeholders("{other}", Some(file), None, dir), "{other}");
    }

    #[test]
    fn editor_invocation_keeps_a_spaced_path_as_one_argument() {
        let file = Path::new("/r/my file.rs");
        let inv = editor_invocation("code --goto {file}:{line}", Posix, file, Some(3), Path::new("/r")).unwrap();
        assert_eq!(argv(&inv), ["code", "--goto", "/r/my file.rs:3"]);
        let inv = editor_invocation(r#""C:\Program Files\Sublime Text\subl.exe" {file}:{line}"#, Windows, file, None, Path::new("/r")).unwrap();
        assert_eq!(argv(&inv), [r"C:\Program Files\Sublime Text\subl.exe", "/r/my file.rs:1"]);
    }

    #[test]
    fn editor_invocation_appends_the_file_without_a_placeholder() {
        let inv = editor_invocation("gedit --new-window", Posix, Path::new("/r/a.rs"), None, Path::new("/r")).unwrap();
        assert_eq!(argv(&inv), ["gedit", "--new-window", "/r/a.rs"]);
    }

    #[test]
    fn may_execute_per_os() {
        assert!(may_execute(Path::new("setup.EXE"), Os::Windows, false));
        assert!(may_execute(Path::new("run.bat"), Os::Windows, false));
        assert!(may_execute(Path::new("x.lnk"), Os::Windows, false));
        assert!(may_execute(Path::new("app.js"), Os::Windows, false));
        assert!(!may_execute(Path::new("app.js"), Os::Linux, false));
        assert!(!may_execute(Path::new("README.md"), Os::Windows, false));
        assert!(may_execute(Path::new("build"), Os::Linux, true));
        assert!(may_execute(Path::new("x.desktop"), Os::Linux, false));
        assert!(may_execute(Path::new("x.command"), Os::MacOs, false));
        assert!(may_execute(Path::new("Tool.app"), Os::MacOs, false));
        assert!(!may_execute(Path::new("notes.txt"), Os::MacOs, false));
    }

    #[test]
    fn editor_target_uses_the_default_app_only_for_safe_files() {
        let dir = Path::new("/r");
        assert_eq!(
            editor_target(None, Os::Linux, Posix, Path::new("/r/a.md"), None, dir, false).unwrap(),
            EditorTarget::SystemDefault(PathBuf::from("/r/a.md"))
        );
        assert_eq!(
            editor_target(Some("  "), Os::Linux, Posix, Path::new("/r/a.md"), None, dir, false).unwrap(),
            EditorTarget::SystemDefault(PathBuf::from("/r/a.md"))
        );
    }

    #[test]
    fn editor_target_refuses_an_executable_without_an_editor() {
        let err = editor_target(None, Os::Windows, Windows, Path::new("/r/setup.exe"), None, Path::new("/r"), false).unwrap_err();
        assert_eq!(
            err.to_string(),
            "No editor is configured, and opening setup.exe with the system default app could run it. Set an editor command in Preferences."
        );
        // A configured editor opens anything.
        assert!(matches!(
            editor_target(Some("code"), Os::Windows, Windows, Path::new("/r/setup.exe"), None, Path::new("/r"), false),
            Ok(EditorTarget::Command(_))
        ));
    }

    #[test]
    fn default_terminal_per_os() {
        let none = |_: &str| false;
        let only = |name: &'static str| move |p: &str| p == name;
        assert_eq!(argv(&default_terminal(Os::MacOs, None, &none).unwrap()), ["open", "-a", "Terminal", "{dir}"]);
        assert_eq!(argv(&default_terminal(Os::Windows, None, &only("wt")).unwrap()), ["wt", "-d", "{dir}"]);
        let cmd = default_terminal(Os::Windows, None, &none).unwrap();
        assert_eq!((argv(&cmd), cmd.new_console), (vec!["cmd", "/K"], true));
        assert_eq!(argv(&default_terminal(Os::Linux, Some("foot --app-id x"), &none).unwrap()), ["foot", "--app-id", "x"]);
        assert_eq!(argv(&default_terminal(Os::Linux, Some(""), &only("xterm")).unwrap()), ["xterm"]);
        assert_eq!(
            argv(&default_terminal(Os::Linux, None, &only("gnome-terminal")).unwrap()),
            ["gnome-terminal", "--working-directory={dir}"]
        );
        let both = |p: &str| p == "x-terminal-emulator" || p == "konsole";
        assert_eq!(argv(&default_terminal(Os::Linux, None, &both).unwrap()), ["x-terminal-emulator"]);
        assert_eq!(default_terminal(Os::Linux, None, &none), None);
    }

    #[test]
    fn terminal_invocation_prefers_the_configured_command_and_expands_dir() {
        let none = |_: &str| false;
        let dir = Path::new("/r w");
        let inv = terminal_invocation(Some("kitty --directory {dir}"), Os::Linux, Posix, None, &none, dir).unwrap();
        assert_eq!(argv(&inv), ["kitty", "--directory", "/r w"]);
        let inv = terminal_invocation(None, Os::MacOs, Posix, None, &none, dir).unwrap();
        assert_eq!(argv(&inv), ["open", "-a", "Terminal", "/r w"]);
        assert_eq!(
            terminal_invocation(None, Os::Linux, Posix, None, &none, dir).unwrap_err().to_string(),
            "No terminal found. Set a terminal command in Preferences."
        );
    }

    #[test]
    fn prepare_editor_reports_a_missing_file_and_reveal_target_falls_back() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        assert_eq!(
            prepare_editor(dir.path(), "gone.txt", None, Some("code")).unwrap_err().to_string(),
            "gone.txt no longer exists in the working tree."
        );
        assert!(matches!(prepare_editor(dir.path(), "a.txt", Some(2), Some("code")), Ok(EditorTarget::Command(_))));
        assert_eq!(reveal_target(dir.path(), None).unwrap(), dir.path());
        assert_eq!(reveal_target(dir.path(), Some("gone/x.txt")).unwrap(), dir.path());
        assert_eq!(reveal_target(dir.path(), Some("a.txt")).unwrap(), dir.path().join("a.txt"));
        assert!(matches!(reveal_target(dir.path(), Some("../x")), Err(OpenError::InvalidPath)));
    }

    #[cfg(unix)]
    #[test]
    fn has_exec_bit_reads_the_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("s");
        std::fs::write(&file, "").unwrap();
        assert!(!has_exec_bit(&file));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(has_exec_bit(&file));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --lib launch::`
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
//! Pure resolution of "open in editor", "open terminal" and "reveal" for the desktop app. The
//! OS, environment and PATH lookups are parameters so every rule is unit tested on every OS;
//! `prepare_*` bind them to the real environment.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::process::{self, CommandStyle, LaunchError, SpawnSpec, SplitError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os { Linux, MacOs, Windows }

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") { Os::MacOs } else if cfg!(windows) { Os::Windows } else { Os::Linux }
    }
}

#[derive(Debug, Error)]
pub enum OpenError {
    #[error("Invalid path.")]
    InvalidPath,
    #[error("{0} no longer exists in the working tree.")]
    Missing(String),
    #[error("No editor is configured, and opening {0} with the system default app could run it. Set an editor command in Preferences.")]
    MayExecute(String),
    #[error("No terminal found. Set a terminal command in Preferences.")]
    NoTerminal,
    #[error(transparent)]
    Launch(#[from] LaunchError),
}

impl From<SplitError> for OpenError {
    fn from(error: SplitError) -> Self {
        OpenError::Launch(LaunchError::Split(error))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation { pub argv: Vec<String>, pub new_console: bool }

#[derive(Debug, PartialEq, Eq)]
pub enum EditorTarget { Command(Invocation), SystemDefault(PathBuf) }

pub fn resolve_in_workdir(workdir: &Path, rel: &str) -> Result<PathBuf, OpenError> {
    let path = Path::new(rel);
    if rel.is_empty() || path.is_absolute() || path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(OpenError::InvalidPath);
    }
    Ok(workdir.join(path))
}

pub fn nearest_existing(workdir: &Path, target: &Path) -> PathBuf {
    target
        .ancestors()
        .find(|p| p.starts_with(workdir) && p.exists())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| workdir.to_path_buf())
}

pub fn expand_placeholders(word: &str, file: Option<&Path>, line: Option<u32>, dir: &Path) -> String {
    let mut out = word.replace("{line}", &line.unwrap_or(1).to_string());
    out = out.replace("{dir}", &dir.display().to_string());
    if let Some(file) = file {
        out = out.replace("{file}", &file.display().to_string());
    }
    out
}

const WINDOWS_RUNNABLE: &[&str] = &[
    "exe", "com", "bat", "cmd", "ps1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "msi", "msc", "scr",
    "lnk", "url", "hta", "cpl", "jar",
];
const UNIX_RUNNABLE: &[&str] = &["app", "command", "tool", "desktop", "jar", "sh", "appimage", "run"];

pub fn may_execute(path: &Path, os: Os, has_exec_bit: bool) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    match os {
        Os::Windows => WINDOWS_RUNNABLE.contains(&ext.as_str()),
        Os::Linux | Os::MacOs => has_exec_bit || UNIX_RUNNABLE.contains(&ext.as_str()),
    }
}

#[cfg(unix)]
pub fn has_exec_bit(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(not(unix))]
pub fn has_exec_bit(_path: &Path) -> bool {
    false
}

pub fn editor_invocation(command: &str, style: CommandStyle, file: &Path, line: Option<u32>, dir: &Path) -> Result<Invocation, SplitError> {
    let words = process::split_command_line(command, style)?;
    let has_file = words.iter().any(|w| w.contains("{file}"));
    let mut argv: Vec<String> = words.iter().map(|w| expand_placeholders(w, Some(file), line, dir)).collect();
    if !has_file {
        argv.push(file.display().to_string());
    }
    Ok(Invocation { argv, new_console: false })
}

pub fn editor_target(configured: Option<&str>, os: Os, style: CommandStyle, file: &Path, line: Option<u32>, dir: &Path, has_exec_bit: bool) -> Result<EditorTarget, OpenError> {
    match configured.map(str::trim).filter(|c| !c.is_empty()) {
        Some(command) => Ok(EditorTarget::Command(editor_invocation(command, style, file, line, dir)?)),
        None if may_execute(file, os, has_exec_bit) => Err(OpenError::MayExecute(
            file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| file.display().to_string()),
        )),
        None => Ok(EditorTarget::SystemDefault(file.to_path_buf())),
    }
}

fn inv(words: &[&str], new_console: bool) -> Invocation {
    Invocation { argv: words.iter().map(|w| w.to_string()).collect(), new_console }
}

const LINUX_TERMINALS: &[&[&str]] = &[
    &["x-terminal-emulator"],
    &["gnome-terminal", "--working-directory={dir}"],
    &["konsole", "--workdir", "{dir}"],
    &["xfce4-terminal", "--working-directory={dir}"],
    &["alacritty", "--working-directory", "{dir}"],
    &["kitty", "--directory", "{dir}"],
    &["xterm"],
];

pub fn default_terminal(os: Os, env_terminal: Option<&str>, has_program: &dyn Fn(&str) -> bool) -> Option<Invocation> {
    match os {
        Os::MacOs => Some(inv(&["open", "-a", "Terminal", "{dir}"], false)),
        Os::Windows if has_program("wt") => Some(inv(&["wt", "-d", "{dir}"], false)),
        Os::Windows => Some(inv(&["cmd", "/K"], true)),
        Os::Linux => {
            if let Some(words) = env_terminal
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .and_then(|t| process::split_command_line(t, CommandStyle::Posix).ok())
            {
                return Some(Invocation { argv: words, new_console: false });
            }
            LINUX_TERMINALS.iter().find(|c| has_program(c[0])).map(|c| inv(c, false))
        }
    }
}

pub fn terminal_invocation(configured: Option<&str>, os: Os, style: CommandStyle, env_terminal: Option<&str>, has_program: &dyn Fn(&str) -> bool, dir: &Path) -> Result<Invocation, OpenError> {
    let base = match configured.map(str::trim).filter(|c| !c.is_empty()) {
        Some(command) => Invocation { argv: process::split_command_line(command, style)?, new_console: false },
        None => default_terminal(os, env_terminal, has_program).ok_or(OpenError::NoTerminal)?,
    };
    Ok(Invocation {
        argv: base.argv.iter().map(|w| expand_placeholders(w, None, None, dir)).collect(),
        new_console: base.new_console,
    })
}

pub fn prepare_editor(workdir: &Path, rel: &str, line: Option<u32>, configured: Option<&str>) -> Result<EditorTarget, OpenError> {
    let file = resolve_in_workdir(workdir, rel)?;
    if !file.exists() {
        return Err(OpenError::Missing(rel.to_string()));
    }
    editor_target(configured, Os::current(), CommandStyle::native(), &file, line, workdir, has_exec_bit(&file))
}

pub fn prepare_terminal(workdir: &Path, configured: Option<&str>) -> Result<Invocation, OpenError> {
    let env_terminal = std::env::var("TERMINAL").ok();
    terminal_invocation(configured, Os::current(), CommandStyle::native(), env_terminal.as_deref(), &|p| process::find_program(p).is_some(), workdir)
}

pub fn reveal_target(workdir: &Path, rel: Option<&str>) -> Result<PathBuf, OpenError> {
    match rel {
        None => Ok(workdir.to_path_buf()),
        Some(rel) => Ok(nearest_existing(workdir, &resolve_in_workdir(workdir, rel)?)),
    }
}

pub fn run_invocation(invocation: &Invocation, cwd: &Path) -> Result<(), OpenError> {
    let (program, args) = invocation.argv.split_first().ok_or(OpenError::Launch(LaunchError::Split(SplitError::Empty)))?;
    let program = process::find_program(program).ok_or_else(|| LaunchError::NotFound(program.clone()))?;
    process::spawn_detached(&SpawnSpec {
        program,
        args: args.to_vec(),
        cwd: cwd.to_path_buf(),
        env: Vec::new(),
        new_console: invocation.new_console,
        no_window: false,
        capture_stderr: false,
    })?;
    Ok(())
}
```

Note on `resolve_in_workdir_rejects_escapes`: `"./a"` has a `CurDir` component and is rejected on purpose. Status paths never contain it.

- [ ] **Step 4: Run to verify pass, plus clippy** (`#[allow(clippy::too_many_arguments)]` on `editor_target`/`terminal_invocation` if clippy asks)

Run: `cargo test -p git-core --lib launch:: && cargo clippy -p git-core --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/launch.rs crates/git-core/src/lib.rs
git commit -m "feat(git-core): editor, terminal and reveal resolution"
```

---

### Task 3: `git-core::tools`: tool resolution and presets

**Files:**
- Create: `crates/git-core/src/tools.rs` (resolution part), `crates/git-core/tests/tools.rs`
- Modify: `crates/git-core/src/lib.rs` (add `pub mod tools;` after `pub mod status;`, before `pub mod submodule;`, alphabetical)

**Interfaces:**
- Consumes: `process::{split_command_line, find_program, find_git_sh, CommandStyle, LaunchError, SplitError}`.
- Produces (Tasks 4, 7, 8): `ToolKind`, `ConfigSource`, `ToolCommand`, `ResolvedTool`, `ToolVars`, `ToolError` (all variants from the spec), `resolve_tool`, `preset`, `is_terminal_tool`, `expand_tool_vars`, `GitToolInfo`, `GitTools`, `git_tools`, and the crate-private `ToolInvocation` + `build_invocation(command: &ToolCommand, vars: &ToolVars, workdir: &Path) -> Result<ToolInvocation, ToolError>`. Make `ToolInvocation` `pub`, because `PreparedDiffTool` exposes it.

- [ ] **Step 1: Write the failing tests**

Inline in `tools.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> ToolVars {
        ToolVars {
            local: "/t/a_LOCAL.rs".into(),
            remote: "/t/a_REMOTE.rs".into(),
            merged: "/r/a.rs".into(),
            base: "/t/a_BASE.rs".into(),
        }
    }

    #[test]
    fn expand_tool_vars_replaces_only_the_four_names() {
        let v = vars();
        assert_eq!(expand_tool_vars("$LOCAL", &v), "/t/a_LOCAL.rs");
        assert_eq!(expand_tool_vars("--output=$MERGED", &v), "--output=/r/a.rs");
        assert_eq!(expand_tool_vars("${REMOTE}x", &v), "/t/a_REMOTE.rsx");
        assert_eq!(expand_tool_vars("$BASEX $HOME $", &v), "$BASEX $HOME $");
        assert_eq!(expand_tool_vars("-mergeoutput=$MERGED", &v), "-mergeoutput=/r/a.rs");
    }

    #[test]
    fn presets_and_terminal_tools() {
        assert_eq!(preset(ToolKind::Diff, "meld"), Some(("meld", &["$LOCAL", "$REMOTE"][..])));
        assert_eq!(preset(ToolKind::Merge, "bc4").unwrap().0, "bcompare");
        assert_eq!(preset(ToolKind::Merge, "vscode").unwrap().1, &["--wait", "--merge", "$REMOTE", "$LOCAL", "$BASE", "$MERGED"][..]);
        assert_eq!(preset(ToolKind::Diff, "vimdiff"), None);
        assert!(is_terminal_tool("nvimdiff2"));
        assert!(!is_terminal_tool("gvimdiff"));
    }
}
```

`crates/git-core/tests/tools.rs` (new). It uses isolated config files, so the developer's global `diff.tool` cannot leak in:

```rust
mod common;

use git2::{Config, ConfigLevel};
use git_core::tools::{git_tools, resolve_tool, ConfigSource, ToolCommand, ToolKind};

fn config_with(entries: &[(&str, &str)]) -> (tempfile::TempDir, Config) {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config");
    std::fs::write(&file, "").unwrap();
    let mut config = Config::new().unwrap();
    config.add_file(&file, ConfigLevel::Local, false).unwrap();
    for (key, value) in entries {
        config.set_str(key, value).unwrap();
    }
    (dir, config)
}

#[test]
fn a_cmd_beats_a_preset_and_runs_in_the_shell() {
    let (_d, config) = config_with(&[("diff.tool", "meld"), ("difftool.meld.cmd", "meld --newtab \"$LOCAL\" \"$REMOTE\"")]);
    let tool = resolve_tool(&config, ToolKind::Diff).unwrap();
    assert_eq!(tool.name, "meld");
    assert_eq!(tool.source, ConfigSource::Local);
    assert!(!tool.via_merge_tool);
    assert_eq!(tool.command, ToolCommand::Shell("meld --newtab \"$LOCAL\" \"$REMOTE\"".into()));
}

#[test]
fn a_preset_honours_the_path_setting() {
    let (_d, config) = config_with(&[("merge.tool", "kdiff3"), ("mergetool.kdiff3.path", "/opt/kdiff3/bin/kdiff3")]);
    let tool = resolve_tool(&config, ToolKind::Merge).unwrap();
    assert_eq!(
        tool.command,
        ToolCommand::Preset {
            program: "/opt/kdiff3/bin/kdiff3".into(),
            args: ["--auto", "-o", "$MERGED", "$BASE", "$LOCAL", "$REMOTE"].map(String::from).to_vec(),
        }
    );
    assert!(!tool.trust_exit_code);
}

#[test]
fn the_diff_tool_falls_back_to_merge_tool() {
    let (_d, config) = config_with(&[("merge.tool", "p4merge")]);
    let tool = resolve_tool(&config, ToolKind::Diff).unwrap();
    assert!(tool.via_merge_tool);
    assert_eq!(tool.name, "p4merge");
}

#[test]
fn trust_exit_code_is_read_for_merge_tools() {
    let (_d, mut config) = config_with(&[("merge.tool", "t"), ("mergetool.t.cmd", "true")]);
    config.set_bool("mergetool.t.trustExitCode", true).unwrap();
    assert!(resolve_tool(&config, ToolKind::Merge).unwrap().trust_exit_code);
}

#[test]
fn unusable_configurations_have_specific_messages() {
    let (_d, empty) = config_with(&[]);
    assert_eq!(
        resolve_tool(&empty, ToolKind::Diff).unwrap_err().to_string(),
        "No diff tool is configured. Set diff.tool in your git config, for example: git config --global diff.tool meld"
    );
    assert_eq!(
        resolve_tool(&empty, ToolKind::Merge).unwrap_err().to_string(),
        "No merge tool is configured. Set merge.tool in your git config, for example: git config --global merge.tool meld"
    );
    let (_d, vim) = config_with(&[("diff.tool", "vimdiff")]);
    assert_eq!(
        resolve_tool(&vim, ToolKind::Diff).unwrap_err().to_string(),
        "vimdiff runs in a terminal, which Browsitory can't host. Choose a graphical tool, or set difftool.vimdiff.cmd to run it in a terminal window."
    );
    let (_d, unknown) = config_with(&[("merge.tool", "frobmerge")]);
    assert_eq!(
        resolve_tool(&unknown, ToolKind::Merge).unwrap_err().to_string(),
        "merge.tool is set to \"frobmerge\", but mergetool.frobmerge.cmd is not set and Browsitory has no built-in command for it."
    );
}

#[test]
fn git_tools_reports_both_tools_with_problems_as_text() {
    let (_dir, repo) = common::init_repo();
    {
        let mut config = repo.config().unwrap().open_level(ConfigLevel::Local).unwrap();
        config.set_str("diff.tool", "meld").unwrap();
        config.set_str("merge.tool", "emerge").unwrap();
    }
    let tools = git_tools(&repo).unwrap();
    let diff = tools.diff_tool.unwrap();
    assert_eq!((diff.name.as_str(), diff.command.as_deref(), diff.problem), ("meld", Some("meld $LOCAL $REMOTE"), None));
    let merge = tools.merge_tool.unwrap();
    assert_eq!(merge.name, "emerge");
    assert!(merge.problem.unwrap().starts_with("emerge runs in a terminal"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --lib tools:: && cargo test -p git-core --test tools`
Expected: FAIL (compile errors: `tools` missing).

- [ ] **Step 3: Implement the resolution part of `tools.rs`**

```rust
//! git's diff.tool / merge.tool, resolved and launched the way `git difftool`/`git mergetool`
//! do. See `docs/superpowers/specs/2026-10-27-parity-batch4f-design.md`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use git2::{Config, ConfigLevel, Repository};
use thiserror::Error;

use crate::process::{self, CommandStyle, LaunchError, SplitError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind { Diff, Merge }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource { System, Xdg, Global, Local, Worktree, Other }

impl From<ConfigLevel> for ConfigSource {
    fn from(level: ConfigLevel) -> Self {
        match level {
            ConfigLevel::ProgramData | ConfigLevel::System => ConfigSource::System,
            ConfigLevel::XDG => ConfigSource::Xdg,
            ConfigLevel::Global => ConfigSource::Global,
            ConfigLevel::Local => ConfigSource::Local,
            ConfigLevel::Worktree => ConfigSource::Worktree,
            _ => ConfigSource::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCommand {
    Shell(String),
    Preset { program: String, args: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTool {
    pub name: String,
    pub source: ConfigSource,
    pub via_merge_tool: bool,
    pub command: ToolCommand,
    pub trust_exit_code: bool,
}

#[derive(Debug, Error)]
pub enum ToolError { /* exactly the spec's variants and messages */ }

impl From<SplitError> for ToolError {
    fn from(error: SplitError) -> Self { ToolError::Launch(LaunchError::Split(error)) }
}

type Preset = (&'static str, &'static str, &'static [&'static str], &'static [&'static str]);
const PRESETS: &[Preset] = &[
    ("meld", "meld", &["$LOCAL", "$REMOTE"], &["--output=$MERGED", "$LOCAL", "$BASE", "$REMOTE"]),
    ("kdiff3", "kdiff3", &["$LOCAL", "$REMOTE"], &["--auto", "-o", "$MERGED", "$BASE", "$LOCAL", "$REMOTE"]),
    ("bc", "bcompare", &["$LOCAL", "$REMOTE"], &["$LOCAL", "$REMOTE", "$BASE", "-mergeoutput=$MERGED"]),
    ("bc3", "bcompare", &["$LOCAL", "$REMOTE"], &["$LOCAL", "$REMOTE", "$BASE", "-mergeoutput=$MERGED"]),
    ("bc4", "bcompare", &["$LOCAL", "$REMOTE"], &["$LOCAL", "$REMOTE", "$BASE", "-mergeoutput=$MERGED"]),
    ("p4merge", "p4merge", &["$LOCAL", "$REMOTE"], &["$BASE", "$LOCAL", "$REMOTE", "$MERGED"]),
    ("vscode", "code", &["--wait", "--diff", "$LOCAL", "$REMOTE"], &["--wait", "--merge", "$REMOTE", "$LOCAL", "$BASE", "$MERGED"]),
    ("winmerge", "WinMergeU", &["-u", "-e", "$LOCAL", "$REMOTE"], &["-u", "-e", "-wl", "-wr", "-fm", "$LOCAL", "$BASE", "$REMOTE", "-o", "$MERGED"]),
];
const TERMINAL_TOOLS: &[&str] = &["vimdiff", "vimdiff1", "vimdiff2", "vimdiff3", "nvimdiff", "nvimdiff1", "nvimdiff2", "nvimdiff3", "emerge"];

pub fn preset(kind: ToolKind, name: &str) -> Option<(&'static str, &'static [&'static str])> {
    PRESETS.iter().find(|p| p.0 == name).map(|p| (p.1, if kind == ToolKind::Diff { p.2 } else { p.3 }))
}

pub fn is_terminal_tool(name: &str) -> bool {
    TERMINAL_TOOLS.contains(&name)
}

fn config_string(config: &Config, key: &str) -> Option<String> {
    config.get_string(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn tool_entry(config: &Config, key: &str) -> Option<(String, ConfigSource)> {
    let entry = config.get_entry(key).ok()?;
    let value = entry.value()?.trim().to_string();
    (!value.is_empty()).then(|| (value, entry.level().into()))
}

pub fn resolve_tool(config: &Config, kind: ToolKind) -> Result<ResolvedTool, ToolError> {
    let (setting, section) = match kind {
        ToolKind::Diff => ("diff.tool", "difftool"),
        ToolKind::Merge => ("merge.tool", "mergetool"),
    };
    let (name, source, via_merge_tool) = match tool_entry(config, setting) {
        Some((name, source)) => (name, source, false),
        None if kind == ToolKind::Diff => match tool_entry(config, "merge.tool") {
            Some((name, source)) => (name, source, true),
            None => return Err(ToolError::NoDiffTool),
        },
        None => return Err(ToolError::NoMergeTool),
    };
    let key = format!("{section}.{name}.cmd");
    let trust_exit_code = kind == ToolKind::Merge
        && config.get_bool(&format!("{section}.{name}.trustExitCode")).unwrap_or(false);
    let command = if let Some(cmd) = config_string(config, &key) {
        ToolCommand::Shell(cmd)
    } else if is_terminal_tool(&name) {
        return Err(ToolError::TerminalTool { name, key });
    } else if let Some((program, args)) = preset(kind, &name) {
        ToolCommand::Preset {
            program: config_string(config, &format!("{section}.{name}.path")).unwrap_or_else(|| program.to_string()),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    } else {
        let setting = if via_merge_tool { "merge.tool" } else { setting };
        return Err(ToolError::UnknownTool { setting: setting.to_string(), name, key });
    };
    Ok(ResolvedTool { name, source, via_merge_tool, command, trust_exit_code })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolVars { pub local: PathBuf, pub remote: PathBuf, pub merged: PathBuf, pub base: PathBuf }

impl ToolVars {
    fn get(&self, name: &str) -> Option<&Path> {
        match name {
            "LOCAL" => Some(&self.local),
            "REMOTE" => Some(&self.remote),
            "MERGED" => Some(&self.merged),
            "BASE" => Some(&self.base),
            _ => None,
        }
    }
    fn env(&self) -> Vec<(String, OsString)> {
        ["LOCAL", "REMOTE", "MERGED", "BASE"]
            .iter()
            .map(|n| (n.to_string(), self.get(n).expect("known name").as_os_str().to_owned()))
            .collect()
    }
}

/// Expands `$LOCAL`/`${LOCAL}` (and REMOTE, MERGED, BASE) inside one argv word. Any other `$…`
/// is kept literally; there is no shell here.
pub fn expand_tool_vars(word: &str, vars: &ToolVars) -> String {
    let mut out = String::new();
    let mut rest = word;
    while let Some(pos) = rest.find('$') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        let (name, consumed) = if let Some(inner) = after.strip_prefix('{') {
            match inner.find('}') {
                Some(end) => (&inner[..end], end + 2),
                None => ("", 0),
            }
        } else {
            let end = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(after.len());
            (&after[..end], end)
        };
        match vars.get(name) {
            Some(path) if consumed > 0 => {
                out.push_str(&path.display().to_string());
                rest = &after[consumed..];
            }
            _ => {
                out.push('$');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Clone)]
pub struct ToolInvocation { pub program: PathBuf, pub args: Vec<String>, pub env: Vec<(String, OsString)>, pub cwd: PathBuf, pub no_window: bool }

fn program_path(name: &str) -> Result<PathBuf, ToolError> {
    process::find_program(name).ok_or_else(|| ToolError::Launch(LaunchError::NotFound(name.to_string())))
}

fn shell() -> Option<PathBuf> {
    if cfg!(windows) { process::find_git_sh() } else { Some(PathBuf::from("/bin/sh")) }
}

pub(crate) fn build_invocation(command: &ToolCommand, vars: &ToolVars, workdir: &Path) -> Result<ToolInvocation, ToolError> {
    let cwd = workdir.to_path_buf();
    match command {
        ToolCommand::Preset { program, args } => Ok(ToolInvocation {
            program: program_path(program)?,
            args: args.iter().map(|a| expand_tool_vars(a, vars)).collect(),
            env: vars.env(),
            cwd,
            no_window: false,
        }),
        ToolCommand::Shell(cmd) => match shell() {
            Some(sh) => Ok(ToolInvocation { program: sh, args: vec!["-c".into(), cmd.clone()], env: vars.env(), cwd, no_window: cfg!(windows) }),
            None => {
                let words: Vec<String> = process::split_command_line(cmd, CommandStyle::Posix)?
                    .iter()
                    .map(|w| expand_tool_vars(w, vars))
                    .collect();
                let (program, args) = words.split_first().ok_or(ToolError::from(SplitError::Empty))?;
                Ok(ToolInvocation { program: program_path(program)?, args: args.to_vec(), env: vars.env(), cwd, no_window: false })
            }
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitToolInfo { pub name: String, pub command: Option<String>, pub source: ConfigSource, pub via_merge_tool: bool, pub problem: Option<String> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTools { pub diff_tool: Option<GitToolInfo>, pub merge_tool: Option<GitToolInfo> }

fn info(config: &Config, kind: ToolKind) -> Option<GitToolInfo> {
    match resolve_tool(config, kind) {
        Ok(tool) => Some(GitToolInfo {
            command: Some(match &tool.command {
                ToolCommand::Shell(cmd) => cmd.clone(),
                ToolCommand::Preset { program, args } => std::iter::once(program.clone()).chain(args.iter().cloned()).collect::<Vec<_>>().join(" "),
            }),
            name: tool.name,
            source: tool.source,
            via_merge_tool: tool.via_merge_tool,
            problem: None,
        }),
        Err(ToolError::NoDiffTool | ToolError::NoMergeTool) => None,
        Err(error) => {
            let (setting, fallback) = if kind == ToolKind::Diff { ("diff.tool", true) } else { ("merge.tool", false) };
            let (name, source, via) = tool_entry(config, setting)
                .map(|(n, s)| (n, s, false))
                .or_else(|| fallback.then(|| tool_entry(config, "merge.tool").map(|(n, s)| (n, s, true))).flatten())?;
            Some(GitToolInfo { name, command: None, source, via_merge_tool: via, problem: Some(error.to_string()) })
        }
    }
}

pub fn git_tools(repo: &Repository) -> Result<GitTools, ToolError> {
    let config = repo.config()?;
    Ok(GitTools { diff_tool: info(&config, ToolKind::Diff), merge_tool: info(&config, ToolKind::Merge) })
}
```

Fill in `ToolError` with the spec's exact variants. `ToolFailed`'s `tool` is `&'static str`, either `"diff tool"` or `"merge tool"`.

- [ ] **Step 4: Run to verify pass, plus clippy**

Run: `cargo test -p git-core --lib tools:: && cargo test -p git-core --test tools && cargo clippy -p git-core --all-targets -- -D warnings`
Expected: PASS. `build_invocation` is unused until Task 4. Add `#[allow(dead_code)]` with a `// used by prepare_* (Task 4)` comment if clippy fails, and remove it in Task 4.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/tools.rs crates/git-core/src/lib.rs crates/git-core/tests/tools.rs
git commit -m "feat(git-core): resolve diff.tool and merge.tool with built-in presets"
```

---

### Task 4: `git-core::tools`: file versions, temp sides, launch and wait

**Files:**
- Modify: `crates/git-core/src/tools.rs`, `crates/git-core/tests/tools.rs`

**Interfaces:**
- Consumes: Task 3, `process::{spawn, drain_tail, tail_text, SpawnSpec}`.
- Produces (Tasks 7, 8, 9):

```rust
pub enum DiffSource { Unstaged, Staged, Commit { commit_id: String, old_path: Option<String> }, Range { from_id: String, to_id: String, old_path: Option<String> } }
pub enum FileVersion { Head, Index, Stage(u8), Commit(String), FirstParent(String) }
pub const MAX_FILE_VERSION_BYTES: usize = 20 * 1024 * 1024;
pub fn file_version_contents(repo: &Repository, path: &str, version: &FileVersion, limit: usize) -> Result<Option<Vec<u8>>, ToolError>;
pub struct PreparedDiffTool { pub invocation: ToolInvocation, pub temp_dir: PathBuf }
pub struct PreparedMergeTool { pub invocation: ToolInvocation, pub temp_dir: PathBuf, pub merged: PathBuf, pub merged_before: Vec<u8>, pub trust_exit_code: bool }
pub fn tool_temp_root() -> PathBuf;
pub fn prepare_diff_tool(repo: &Repository, path: &str, source: &DiffSource) -> Result<PreparedDiffTool, ToolError>;
pub fn prepare_diff_tool_in(repo: &Repository, path: &str, source: &DiffSource, root: &Path) -> Result<PreparedDiffTool, ToolError>;
pub fn prepare_merge_tool(repo: &Repository, path: &str) -> Result<PreparedMergeTool, ToolError>;
pub fn prepare_merge_tool_in(repo: &Repository, path: &str, root: &Path) -> Result<PreparedMergeTool, ToolError>;
pub fn sweep_stale_tool_dirs(root: &Path, max_age: Duration);
pub const DIFF_TOOL_GRACE: Duration = Duration::from_secs(1);
pub fn launch_diff_tool(prepared: PreparedDiffTool, grace: Duration) -> Result<(), ToolError>;
pub enum MergeToolOutcome { Resolved, Unresolved }
pub fn run_merge_tool(prepared: PreparedMergeTool) -> Result<MergeToolOutcome, ToolError>;
```

Derives: `DiffSource`/`FileVersion` `Debug, Clone, PartialEq, Eq`. `MergeToolOutcome` also `Copy`. `PreparedDiffTool`/`PreparedMergeTool` `Debug`.

- [ ] **Step 1: Write the failing tests** (append to `tests/tools.rs`)

```rust
use std::path::Path;
use std::time::Duration;

use common::{commit_all, write_file};
use git_core::tools::{
    file_version_contents, prepare_diff_tool_in, prepare_merge_tool_in, sweep_stale_tool_dirs, DiffSource, FileVersion,
    ToolError,
};

fn set_local(repo: &git2::Repository, entries: &[(&str, &str)]) {
    let mut config = repo.config().unwrap().open_level(ConfigLevel::Local).unwrap();
    for (k, v) in entries {
        config.set_str(k, v).unwrap();
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// LOCAL/REMOTE/BASE paths as the env vars the tool would see.
fn env(prepared_env: &[(String, std::ffi::OsString)], name: &str) -> std::path::PathBuf {
    prepared_env.iter().find(|(k, _)| k == name).map(|(_, v)| v.into()).unwrap()
}

#[test]
fn file_versions_come_from_head_index_commits_and_parents() {
    let (dir, repo) = common::init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "first");
    let first = repo.head().unwrap().target().unwrap().to_string();
    write_file(dir.path(), "a.txt", "two\n");
    commit_all(&repo, "second");
    let second = repo.head().unwrap().target().unwrap().to_string();
    write_file(dir.path(), "a.txt", "three\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("a.txt")).unwrap();
    index.write().unwrap();

    let get = |v: FileVersion| file_version_contents(&repo, "a.txt", &v, usize::MAX).unwrap().map(|b| String::from_utf8(b).unwrap());
    assert_eq!(get(FileVersion::Head).as_deref(), Some("two\n"));
    assert_eq!(get(FileVersion::Index).as_deref(), Some("three\n"));
    assert_eq!(get(FileVersion::Commit(first.clone())).as_deref(), Some("one\n"));
    assert_eq!(get(FileVersion::FirstParent(second)).as_deref(), Some("one\n"));
    assert_eq!(get(FileVersion::FirstParent(first)), None);
    assert_eq!(file_version_contents(&repo, "missing.txt", &FileVersion::Head, usize::MAX).unwrap(), None);
    assert!(matches!(
        file_version_contents(&repo, "a.txt", &FileVersion::Head, 2),
        Err(ToolError::TooLarge(p)) if p == "a.txt"
    ));
    assert!(matches!(file_version_contents(&repo, "../x", &FileVersion::Head, usize::MAX), Err(ToolError::InvalidPath)));
}

#[test]
fn unstaged_diff_compares_the_index_with_the_real_work_tree_file() {
    let (dir, repo) = common::init_repo();
    write_file(dir.path(), "src/main.rs", "old\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "src/main.rs", "new\n");
    set_local(&repo, &[("diff.tool", "t"), ("difftool.t.cmd", "true")]);
    let root = tempfile::TempDir::new().unwrap();

    let prepared = prepare_diff_tool_in(&repo, "src/main.rs", &DiffSource::Unstaged, root.path()).unwrap();

    let local = env(&prepared.invocation.env, "LOCAL");
    assert_eq!(local.file_name().unwrap(), "main_LOCAL.rs");
    assert!(local.starts_with(root.path()));
    assert_eq!(read(&local), "old\n");
    assert_eq!(env(&prepared.invocation.env, "REMOTE"), dir.path().join("src/main.rs"));
    assert_eq!(env(&prepared.invocation.env, "MERGED"), dir.path().join("src/main.rs"));
    assert_eq!(prepared.invocation.cwd, dir.path());
}

#[test]
fn staged_commit_and_range_diffs_write_both_sides() {
    let (dir, repo) = common::init_repo();
    write_file(dir.path(), "old.txt", "v1\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap().to_string();
    std::fs::rename(dir.path().join("old.txt"), dir.path().join("new.txt")).unwrap();
    write_file(dir.path(), "new.txt", "v2\n");
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("old.txt")).unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();
    commit_all(&repo, "rename");
    let renamed = repo.head().unwrap().target().unwrap().to_string();
    write_file(dir.path(), "new.txt", "v3\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();
    set_local(&repo, &[("diff.tool", "meld")]);
    let root = tempfile::TempDir::new().unwrap();
    let sides = |source: DiffSource| {
        let p = prepare_diff_tool_in(&repo, "new.txt", &source, root.path()).unwrap();
        (read(&env(&p.invocation.env, "LOCAL")), read(&env(&p.invocation.env, "REMOTE")))
    };

    assert_eq!(sides(DiffSource::Staged), ("v2\n".into(), "v3\n".into()));
    assert_eq!(
        sides(DiffSource::Commit { commit_id: renamed.clone(), old_path: Some("old.txt".into()) }),
        ("v1\n".into(), "v2\n".into())
    );
    assert_eq!(
        sides(DiffSource::Range { from_id: base, to_id: renamed, old_path: Some("old.txt".into()) }),
        ("v1\n".into(), "v2\n".into())
    );
}

#[test]
fn a_missing_side_is_an_empty_file_and_presets_run_without_a_shell() {
    let (dir, repo) = common::init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "new.txt", "fresh\n");
    set_local(&repo, &[("diff.tool", "meld"), ("difftool.meld.path", "/bin/echo")]);
    let root = tempfile::TempDir::new().unwrap();

    let prepared = prepare_diff_tool_in(&repo, "new.txt", &DiffSource::Unstaged, root.path()).unwrap();

    assert_eq!(read(&env(&prepared.invocation.env, "LOCAL")), "");
    assert_eq!(prepared.invocation.program, Path::new("/bin/echo"));
    assert_eq!(prepared.invocation.args[1], dir.path().join("new.txt").display().to_string());
}

#[test]
fn prepare_rejects_an_escaping_path() {
    let (dir, repo) = common::init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    set_local(&repo, &[("diff.tool", "meld")]);
    let root = tempfile::TempDir::new().unwrap();
    assert!(matches!(
        prepare_diff_tool_in(&repo, "../a.txt", &DiffSource::Unstaged, root.path()),
        Err(ToolError::InvalidPath)
    ));
}

fn conflicted_repo() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    write_file(dir.path(), "shared.txt", "base\n");
    commit_all(&repo, "base");
    let main = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "shared.txt", "theirs\n");
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    write_file(dir.path(), "shared.txt", "ours\n");
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature").unwrap();
    (dir, repo)
}

#[test]
fn merge_tool_sides_come_from_the_conflict_stages() {
    let (dir, repo) = conflicted_repo();
    set_local(&repo, &[("merge.tool", "t"), ("mergetool.t.cmd", "true")]);
    let root = tempfile::TempDir::new().unwrap();

    let prepared = prepare_merge_tool_in(&repo, "shared.txt", root.path()).unwrap();

    let e = &prepared.invocation.env;
    assert_eq!(read(&env(e, "LOCAL")), "ours\n");
    assert_eq!(read(&env(e, "REMOTE")), "theirs\n");
    assert_eq!(read(&env(e, "BASE")), "base\n");
    assert_eq!(env(e, "MERGED"), dir.path().join("shared.txt"));
    assert_eq!(prepared.merged_before, std::fs::read(dir.path().join("shared.txt")).unwrap());
    assert!(!prepared.trust_exit_code);
}

#[test]
fn merge_tool_refuses_paths_without_a_text_conflict() {
    let (dir, repo) = conflicted_repo();
    write_file(dir.path(), "other.txt", "x\n");
    set_local(&repo, &[("merge.tool", "meld")]);
    let root = tempfile::TempDir::new().unwrap();
    assert_eq!(
        prepare_merge_tool_in(&repo, "other.txt", root.path()).unwrap_err().to_string(),
        "no conflict found for path 'other.txt'"
    );
}

#[test]
fn sweep_removes_only_old_tool_dirs() {
    let root = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(root.path().join("t-old")).unwrap();
    sweep_stale_tool_dirs(root.path(), Duration::from_secs(3600));
    assert!(root.path().join("t-old").exists());
    sweep_stale_tool_dirs(root.path(), Duration::ZERO);
    assert!(!root.path().join("t-old").exists());
}

#[cfg(unix)]
mod spawning {
    use super::*;
    use git_core::tools::{launch_diff_tool, run_merge_tool, MergeToolOutcome};

    fn merge_with(cmd: &str, trust: bool) -> (tempfile::TempDir, Result<MergeToolOutcome, ToolError>, std::path::PathBuf) {
        let (dir, repo) = conflicted_repo();
        set_local(&repo, &[("merge.tool", "t"), ("mergetool.t.cmd", cmd)]);
        repo.config().unwrap().open_level(ConfigLevel::Local).unwrap().set_bool("mergetool.t.trustExitCode", trust).unwrap();
        let root = tempfile::TempDir::new().unwrap();
        let prepared = prepare_merge_tool_in(&repo, "shared.txt", root.path()).unwrap();
        let temp_dir = prepared.temp_dir.clone();
        (dir, run_merge_tool(prepared), temp_dir)
    }

    #[test]
    fn a_changed_merged_file_is_resolved_and_the_temp_dir_is_removed() {
        let (dir, outcome, temp_dir) = merge_with("printf done > \"$MERGED\"", false);
        assert_eq!(outcome.unwrap(), MergeToolOutcome::Resolved);
        assert_eq!(read(&dir.path().join("shared.txt")), "done");
        assert!(!temp_dir.exists());
    }

    #[test]
    fn an_unchanged_file_or_a_trusted_failure_is_unresolved() {
        assert_eq!(merge_with("true", false).1.unwrap(), MergeToolOutcome::Unresolved);
        assert_eq!(merge_with("printf x > \"$MERGED\"; exit 3", true).1.unwrap(), MergeToolOutcome::Unresolved);
        assert_eq!(merge_with("exit 0", true).1.unwrap(), MergeToolOutcome::Resolved);
    }

    #[test]
    fn a_missing_program_is_an_error_not_an_outcome() {
        let err = merge_with("definitely-not-a-merge-tool-4f", false).1.unwrap_err();
        assert!(matches!(err, ToolError::ToolFailed { tool: "merge tool", code: 127, .. }), "{err}");
    }

    #[test]
    fn a_diff_tool_that_fails_at_once_reports_its_exit_code_and_stderr() {
        let (dir, repo) = common::init_repo();
        write_file(dir.path(), "a.txt", "a\n");
        commit_all(&repo, "base");
        set_local(&repo, &[("diff.tool", "t"), ("difftool.t.cmd", "echo broken >&2; exit 7")]);
        let root = tempfile::TempDir::new().unwrap();
        let prepared = prepare_diff_tool_in(&repo, "a.txt", &DiffSource::Unstaged, root.path()).unwrap();
        let err = launch_diff_tool(prepared, Duration::from_secs(5)).unwrap_err();
        assert_eq!(err.to_string(), "The diff tool failed (exit 7): broken");
    }

    #[test]
    fn a_long_running_diff_tool_returns_after_the_grace_period() {
        let (dir, repo) = common::init_repo();
        write_file(dir.path(), "a.txt", "a\n");
        commit_all(&repo, "base");
        set_local(&repo, &[("diff.tool", "t"), ("difftool.t.cmd", "sleep 2")]);
        let root = tempfile::TempDir::new().unwrap();
        let prepared = prepare_diff_tool_in(&repo, "a.txt", &DiffSource::Unstaged, root.path()).unwrap();
        let started = std::time::Instant::now();
        launch_diff_tool(prepared, Duration::from_millis(200)).unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
```

`a_missing_side_is_an_empty_file_and_presets_run_without_a_shell` relies on `/bin/echo` existing, so gate it with `#[cfg(unix)]`. `repo.workdir()` may differ from `TempDir::path()` by a trailing separator (harmless for `Path` equality) or, on macOS, by a `/private` prefix. If an equality on a work-tree path fails on macOS, compare `canonicalize()`d paths, as Task 7's `workdir` test does.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --test tools`
Expected: FAIL (unresolved imports).

- [ ] **Step 3: Implement** (append to `tools.rs`)

```rust
use std::time::{Duration, Instant, SystemTime};

use git2::{ErrorCode, ObjectType, Oid, Tree};

use crate::process::{drain_tail, spawn, tail_text, SpawnSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSource {
    Unstaged,
    Staged,
    Commit { commit_id: String, old_path: Option<String> },
    Range { from_id: String, to_id: String, old_path: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileVersion { Head, Index, Stage(u8), Commit(String), FirstParent(String) }

pub const MAX_FILE_VERSION_BYTES: usize = 20 * 1024 * 1024;
const GITLINK: u32 = 0o160000;
const STDERR_TAIL_BYTES: usize = 4096;
const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
pub const DIFF_TOOL_GRACE: Duration = Duration::from_secs(1);

fn checked(path: &str) -> Result<&Path, ToolError> {
    let p = Path::new(path);
    if path.is_empty() || p.is_absolute() || p.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return Err(ToolError::InvalidPath);
    }
    Ok(p)
}

fn tree_blob(tree: &Tree, rel: &Path, path: &str) -> Result<Option<Oid>, ToolError> {
    match tree.get_path(rel) {
        Ok(entry) if entry.filemode() as u32 == GITLINK => Err(ToolError::NotAFile(path.to_string())),
        Ok(entry) if entry.kind() == Some(ObjectType::Blob) => Ok(Some(entry.id())),
        Ok(_) => Ok(None),
        Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn index_blob(repo: &Repository, rel: &Path, stage: i32, path: &str) -> Result<Option<Oid>, ToolError> {
    match repo.index()?.get_path(rel, stage) {
        Some(entry) if entry.mode == GITLINK => Err(ToolError::NotAFile(path.to_string())),
        Some(entry) => Ok(Some(entry.id)),
        None => Ok(None),
    }
}

pub fn file_version_contents(repo: &Repository, path: &str, version: &FileVersion, limit: usize) -> Result<Option<Vec<u8>>, ToolError> {
    let rel = checked(path)?;
    let blob = match version {
        FileVersion::Head => match repo.head().ok().and_then(|h| h.peel_to_tree().ok()) {
            Some(tree) => tree_blob(&tree, rel, path)?,
            None => None,
        },
        FileVersion::Index => index_blob(repo, rel, 0, path)?,
        FileVersion::Stage(stage) => index_blob(repo, rel, i32::from(*stage), path)?,
        FileVersion::Commit(id) => tree_blob(&repo.find_commit(Oid::from_str(id)?)?.tree()?, rel, path)?,
        FileVersion::FirstParent(id) => match repo.find_commit(Oid::from_str(id)?)?.parents().next() {
            Some(parent) => tree_blob(&parent.tree()?, rel, path)?,
            None => None,
        },
    };
    let Some(id) = blob else { return Ok(None) };
    let blob = repo.find_blob(id)?;
    if blob.size() > limit {
        return Err(ToolError::TooLarge(path.to_string()));
    }
    Ok(Some(blob.content().to_vec()))
}

pub fn tool_temp_root() -> PathBuf {
    std::env::temp_dir().join("browsitory-tools")
}

pub fn sweep_stale_tool_dirs(root: &Path, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .map(|modified| now.duration_since(modified).unwrap_or_default() >= max_age)
            .unwrap_or(false);
        if old && entry.path().is_dir() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn new_tool_dir(root: &Path) -> Result<PathBuf, ToolError> {
    std::fs::create_dir_all(root)?;
    sweep_stale_tool_dirs(root, STALE_AFTER);
    Ok(tempfile::Builder::new().prefix("t-").tempdir_in(root)?.keep())
}

fn side_name(path: &str, side: &str) -> String {
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let p = Path::new(&name);
    match (p.file_stem(), p.extension()) {
        (Some(stem), Some(ext)) => format!("{}_{side}.{}", stem.to_string_lossy(), ext.to_string_lossy()),
        _ => format!("{name}_{side}"),
    }
}

fn write_side(dir: &Path, path: &str, side: &str, contents: Option<Vec<u8>>) -> Result<PathBuf, ToolError> {
    let file = dir.join(side_name(path, side));
    std::fs::write(&file, contents.unwrap_or_default())?;
    Ok(file)
}

#[derive(Debug)]
pub struct PreparedDiffTool { pub invocation: ToolInvocation, pub temp_dir: PathBuf }

#[derive(Debug)]
pub struct PreparedMergeTool { pub invocation: ToolInvocation, pub temp_dir: PathBuf, pub merged: PathBuf, pub merged_before: Vec<u8>, pub trust_exit_code: bool }

pub fn prepare_diff_tool(repo: &Repository, path: &str, source: &DiffSource) -> Result<PreparedDiffTool, ToolError> {
    prepare_diff_tool_in(repo, path, source, &tool_temp_root())
}

pub fn prepare_diff_tool_in(repo: &Repository, path: &str, source: &DiffSource, root: &Path) -> Result<PreparedDiffTool, ToolError> {
    let rel = checked(path)?;
    let workdir = repo.workdir().ok_or(ToolError::NoWorkdir)?.to_path_buf();
    let tool = resolve_tool(&repo.config()?, ToolKind::Diff)?;
    let (local_version, remote_version, local_path) = match source {
        DiffSource::Unstaged => (FileVersion::Index, None, path),
        DiffSource::Staged => (FileVersion::Head, Some(FileVersion::Index), path),
        DiffSource::Commit { commit_id, old_path } => (
            FileVersion::FirstParent(commit_id.clone()),
            Some(FileVersion::Commit(commit_id.clone())),
            old_path.as_deref().unwrap_or(path),
        ),
        DiffSource::Range { from_id, to_id, old_path } => (
            FileVersion::Commit(from_id.clone()),
            Some(FileVersion::Commit(to_id.clone())),
            old_path.as_deref().unwrap_or(path),
        ),
    };
    let local_contents = file_version_contents(repo, local_path, &local_version, usize::MAX)?;
    let remote_contents = match &remote_version {
        Some(v) => Some(file_version_contents(repo, path, v, usize::MAX)?),
        None => None,
    };
    let temp_dir = new_tool_dir(root)?;
    let local = write_side(&temp_dir, path, "LOCAL", local_contents)?;
    let work_file = workdir.join(rel);
    let remote = match remote_contents {
        Some(contents) => write_side(&temp_dir, path, "REMOTE", contents)?,
        None if work_file.is_file() => work_file.clone(),
        None => write_side(&temp_dir, path, "REMOTE", None)?,
    };
    // git difftool: MERGED is the path being compared and BASE equals MERGED.
    let vars = ToolVars { local, remote, merged: work_file.clone(), base: work_file };
    let invocation = build_invocation(&tool.command, &vars, &workdir)?;
    Ok(PreparedDiffTool { invocation, temp_dir })
}

pub fn prepare_merge_tool(repo: &Repository, path: &str) -> Result<PreparedMergeTool, ToolError> {
    prepare_merge_tool_in(repo, path, &tool_temp_root())
}

pub fn prepare_merge_tool_in(repo: &Repository, path: &str, root: &Path) -> Result<PreparedMergeTool, ToolError> {
    let rel = checked(path)?;
    let workdir = repo.workdir().ok_or(ToolError::NoWorkdir)?.to_path_buf();
    let index = repo.index()?;
    let conflict = index
        .conflicts()?
        .filter_map(Result::ok)
        .find(|c| [&c.ancestor, &c.our, &c.their].iter().any(|e| e.as_ref().is_some_and(|e| e.path == path.as_bytes())))
        .ok_or_else(|| ToolError::NoConflict(path.to_string()))?;
    let (Some(ours), Some(theirs)) = (&conflict.our, &conflict.their) else {
        return Err(ToolError::NotATextConflict(path.to_string()));
    };
    if [Some(ours), Some(theirs), conflict.ancestor.as_ref()].iter().flatten().any(|e| e.mode == GITLINK) {
        return Err(ToolError::NotAFile(path.to_string()));
    }
    let tool = resolve_tool(&repo.config()?, ToolKind::Merge)?;
    let blob = |id: Oid| -> Result<Vec<u8>, ToolError> { Ok(repo.find_blob(id)?.content().to_vec()) };
    let temp_dir = new_tool_dir(root)?;
    let local = write_side(&temp_dir, path, "LOCAL", Some(blob(ours.id)?))?;
    let remote = write_side(&temp_dir, path, "REMOTE", Some(blob(theirs.id)?))?;
    let base = write_side(&temp_dir, path, "BASE", conflict.ancestor.as_ref().map(|a| blob(a.id)).transpose()?)?;
    let merged = workdir.join(rel);
    let merged_before = std::fs::read(&merged).unwrap_or_default();
    let vars = ToolVars { local, remote, merged: merged.clone(), base };
    let invocation = build_invocation(&tool.command, &vars, &workdir)?;
    Ok(PreparedMergeTool { invocation, temp_dir, merged, merged_before, trust_exit_code: tool.trust_exit_code })
}

fn spawn_tool(invocation: &ToolInvocation) -> Result<std::process::Child, ToolError> {
    Ok(spawn(&SpawnSpec {
        program: invocation.program.clone(),
        args: invocation.args.clone(),
        cwd: invocation.cwd.clone(),
        env: invocation.env.clone(),
        new_console: false,
        no_window: invocation.no_window,
        capture_stderr: true,
    })?)
}

/// Returns `Err` when the tool exits non-zero within `grace` (typically "command not found").
/// A quick exit 0 leaves the temp dir for the 24 h sweep, because a launcher that hands off to
/// a running instance still needs the files. Otherwise a background thread removes it when the
/// tool exits.
pub fn launch_diff_tool(prepared: PreparedDiffTool, grace: Duration) -> Result<(), ToolError> {
    let mut child = spawn_tool(&prepared.invocation)?;
    let (tail, reader) = drain_tail(child.stderr.take().expect("stderr is piped"), STDERR_TAIL_BYTES);
    let deadline = Instant::now() + grace;
    loop {
        if let Some(status) = child.try_wait()? {
            let _ = reader.join();
            if status.success() {
                return Ok(());
            }
            let _ = std::fs::remove_dir_all(&prepared.temp_dir);
            let output = tail_text(&tail.lock().unwrap_or_else(|e| e.into_inner()));
            return Err(ToolError::ToolFailed { tool: "diff tool", code: status.code().unwrap_or(-1), output });
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let temp_dir = prepared.temp_dir;
    std::thread::spawn(move || {
        let _ = child.wait();
        let _ = reader.join();
        let _ = std::fs::remove_dir_all(temp_dir);
    });
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeToolOutcome { Resolved, Unresolved }

/// Blocks until the merge tool exits; never call it on the repo worker thread.
pub fn run_merge_tool(prepared: PreparedMergeTool) -> Result<MergeToolOutcome, ToolError> {
    let result = (|| {
        let mut child = spawn_tool(&prepared.invocation)?;
        let (tail, reader) = drain_tail(child.stderr.take().expect("stderr is piped"), STDERR_TAIL_BYTES);
        let status = child.wait()?;
        let _ = reader.join();
        let code = status.code().unwrap_or(-1);
        if code == 126 || code == 127 {
            let output = tail_text(&tail.lock().unwrap_or_else(|e| e.into_inner()));
            return Err(ToolError::ToolFailed { tool: "merge tool", code, output });
        }
        let resolved = if prepared.trust_exit_code {
            status.success()
        } else {
            std::fs::read(&prepared.merged).unwrap_or_default() != prepared.merged_before
        };
        Ok(if resolved { MergeToolOutcome::Resolved } else { MergeToolOutcome::Unresolved })
    })();
    let _ = std::fs::remove_dir_all(&prepared.temp_dir);
    result
}
```

Remove Task 3's `#[allow(dead_code)]` on `build_invocation`.

- [ ] **Step 4: Run to verify pass, plus clippy**

Run: `cargo test -p git-core --test tools && cargo test -p git-core --lib && cargo clippy -p git-core --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/tools.rs crates/git-core/tests/tools.rs
git commit -m "feat(git-core): diff and merge tool temp sides, launch and wait"
```

---

### Task 5: `merge::mark_resolved`

**Files:**
- Modify: `crates/git-core/src/merge.rs` (after `resolve_conflict`), `crates/git-core/tests/merge.rs` (after `resolve_conflict_rejects_a_git_relative_path_even_though_it_resolves_under_the_workdir`)

**Interfaces:**
- Produces (Tasks 7–9; reused by 4g): `pub enum MarkResolvedOutcome { Resolved, MarkersRemain }` (`Debug, Clone, Copy, PartialEq, Eq`), `pub fn mark_resolved(repo: &Repository, path: &str, force: bool) -> Result<MarkResolvedOutcome, MergeError>`, `pub(crate) fn has_conflict_markers(content: &[u8]) -> bool`.

- [ ] **Step 1: Write the failing tests**

Inline at the bottom of `merge.rs` (add a `#[cfg(test)] mod tests` if none exists):

```rust
#[test]
fn conflict_markers_are_seven_angle_brackets_at_a_line_start() {
    assert!(has_conflict_markers(b"a\n<<<<<<< HEAD\nb\n"));
    assert!(has_conflict_markers(b"a\r\n>>>>>>>\r\n"));
    assert!(!has_conflict_markers(b"Title\n=======\n"));
    assert!(!has_conflict_markers(b"x <<<<<<< y\n"));
    assert!(!has_conflict_markers(b"<<<<<<<<< nine\n"));
    assert!(!has_conflict_markers(b"\0binary\n<<<<<<< x\n"));
}
```

`tests/merge.rs`:

```rust
#[test]
fn mark_resolved_stages_a_clean_file_and_clears_the_conflict() {
    let (dir, repo) = make_conflicted_repo();
    write_file(dir.path(), "shared.txt", "line one\nmerged two\nline three\n");
    let outcome = git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
    assert!(!repo.index().unwrap().has_conflicts());
}

#[test]
fn mark_resolved_reports_markers_and_keeps_the_conflict() {
    let (_dir, repo) = make_conflicted_repo();
    let outcome = git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::MarkersRemain);
    assert!(repo.index().unwrap().has_conflicts());
}

#[test]
fn mark_resolved_with_force_stages_despite_markers() {
    let (_dir, repo) = make_conflicted_repo();
    let outcome = git_core::merge::mark_resolved(&repo, "shared.txt", true).unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
    assert!(!repo.index().unwrap().has_conflicts());
}

#[test]
fn mark_resolved_rejects_a_path_without_a_conflict() {
    let (dir, repo) = make_conflicted_repo();
    write_file(dir.path(), "other.txt", "x\n");
    let err = git_core::merge::mark_resolved(&repo, "other.txt", false).unwrap_err();
    assert_eq!(err.to_string(), "no conflict found for path 'other.txt'");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --test merge mark_resolved && cargo test -p git-core --lib merge::`
Expected: FAIL (compile error).

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkResolvedOutcome {
    Resolved,
    MarkersRemain,
}

/// Stages the work tree file of a conflicted path, which clears its conflict entries. Without
/// `force`, a file still containing conflict markers is left alone and reported. Not recorded
/// for undo, like `resolve_conflict`.
pub fn mark_resolved(repo: &Repository, path: &str, force: bool) -> Result<MarkResolvedOutcome, MergeError> {
    find_conflict(repo, path)?;
    let workdir = repo.workdir().ok_or(MergeError::NoWorkdir)?;
    let full = workdir.join(path);
    if !force {
        if let Ok(content) = std::fs::read(&full) {
            if has_conflict_markers(&content) {
                return Ok(MarkResolvedOutcome::MarkersRemain);
            }
        }
    }
    let mut index = repo.index()?;
    if full.exists() {
        index.add_path(Path::new(path))?;
    } else {
        index.remove_path(Path::new(path))?;
    }
    index.write()?;
    Ok(MarkResolvedOutcome::Resolved)
}

pub(crate) fn has_conflict_markers(content: &[u8]) -> bool {
    if content.iter().take(8000).any(|b| *b == 0) {
        return false;
    }
    content.split(|b| *b == b'\n').any(|line| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        [&b"<<<<<<<"[..], &b">>>>>>>"[..]]
            .iter()
            .any(|marker| line.strip_prefix(*marker).is_some_and(|rest| rest.is_empty() || rest[0] == b' '))
    })
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p git-core --test merge && cargo test -p git-core --lib merge:: && cargo clippy -p git-core --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/merge.rs crates/git-core/tests/merge.rs
git commit -m "feat(git-core): mark a conflicted file resolved, refusing leftover markers"
```

---

### Task 6: `config`: `[tools]` table

**Files:**
- Modify: `crates/config/src/lib.rs` (struct after `DiffSettings`'s `impl Default`; `ConfigFile.tools` right after `diff`; functions after `set_diff_settings_at`)
- Create: `crates/config/tests/external_tool_settings.rs`

**Interfaces:**
- Produces (Tasks 8, 9): `ExternalToolSettings { editor: Option<String>, terminal: Option<String> }` (`Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize`, `#[serde(default)]`), `get_external_tool_settings()`, `set_external_tool_settings(&ExternalToolSettings)`, and the `_at` variants.

- [ ] **Step 1: Write the failing tests**

```rust
use config::{get_diff_settings_at, get_external_tool_settings_at, set_diff_settings_at, set_external_tool_settings_at, DiffSettings, ExternalToolSettings};

#[test]
fn defaults_to_nothing_configured() {
    let dir = tempfile::TempDir::new().unwrap();
    assert_eq!(get_external_tool_settings_at(&dir.path().join("config.toml")).unwrap(), ExternalToolSettings::default());
}

#[test]
fn round_trips_next_to_the_diff_table() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    let diff = DiffSettings { ignore_whitespace: true, context_lines: 5, syntax_highlight: true };
    set_diff_settings_at(&file, &diff).unwrap();
    let wanted = ExternalToolSettings { editor: Some("code --goto {file}:{line}".into()), terminal: Some("kitty".into()) };
    set_external_tool_settings_at(&file, &wanted).unwrap();
    assert_eq!(get_external_tool_settings_at(&file).unwrap(), wanted);
    assert_eq!(get_diff_settings_at(&file).unwrap(), diff);
    assert!(std::fs::read_to_string(&file).unwrap().contains("[tools]"));
}

#[test]
fn blank_commands_are_stored_as_unset() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_external_tool_settings_at(&file, &ExternalToolSettings { editor: Some("  ".into()), terminal: Some(" xterm ".into()) }).unwrap();
    assert_eq!(
        get_external_tool_settings_at(&file).unwrap(),
        ExternalToolSettings { editor: None, terminal: Some("xterm".into()) }
    );
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p config --test external_tool_settings`
Expected: FAIL (unresolved imports).

- [ ] **Step 3: Implement**

```rust
/// External editor/terminal commands, persisted as the `[tools]` TOML table. `None` means "use
/// the system default". Diff/merge tools are not here; they come from git config.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExternalToolSettings {
    pub editor: Option<String>,
    pub terminal: Option<String>,
}

// in ConfigFile, directly after `diff: DiffSettings,`:
    #[serde(default)]
    tools: ExternalToolSettings,

pub fn get_external_tool_settings() -> Result<ExternalToolSettings, ConfigError> {
    get_external_tool_settings_at(&config_file_path()?)
}

pub fn set_external_tool_settings(settings: &ExternalToolSettings) -> Result<(), ConfigError> {
    set_external_tool_settings_at(&config_file_path()?, settings)
}

pub fn get_external_tool_settings_at(config_file: &Path) -> Result<ExternalToolSettings, ConfigError> {
    Ok(read_config(config_file)?.tools)
}

/// Trims both commands; blank ones are stored as unset.
pub fn set_external_tool_settings_at(config_file: &Path, settings: &ExternalToolSettings) -> Result<(), ConfigError> {
    let clean = |value: &Option<String>| value.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    let mut config = read_config(config_file)?;
    config.tools = ExternalToolSettings { editor: clean(&settings.editor), terminal: clean(&settings.terminal) };
    write_config(config_file, &config)
}
```

Update the `ConfigFile` comment to say "`diff` and `tools` serialize as TOML tables".

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p config && cargo clippy -p config --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/config
git commit -m "feat(config): persist external editor and terminal commands"
```

---

### Task 7: `repo-service` worker commands and `WorkerHandle::workdir`

**Files:**
- Create: `crates/repo-service/src/worker/tools.rs`
- Modify: `crates/repo-service/src/worker/mod.rs` (`mod tools;` after `mod tag;` or alphabetically; `Command` variants after `ResolveAddDeleteConflict`; dispatch arms next to `Command::ResolveAddDeleteConflict { … }`; `workdir` field on `Worker` and `WorkerHandle`; tests), `crates/repo-service/src/worker/merge.rs` (`mark_resolved` handler + `WorkerHandle::mark_resolved`)

**Interfaces:**
- Consumes: Tasks 4, 5.
- Produces (Tasks 8, 9):

```rust
impl WorkerHandle {
    pub fn workdir(&self) -> Result<PathBuf, String>;   // "repository has no working directory (bare repository)"
    pub fn prepare_diff_tool(&self, path: String, source: DiffSource) -> Result<PreparedDiffTool, String>;
    pub fn prepare_merge_tool(&self, path: String) -> Result<PreparedMergeTool, String>;
    pub fn get_git_tools(&self) -> Result<GitTools, String>;
    pub fn read_file_version(&self, path: String, version: FileVersion) -> Result<Option<Vec<u8>>, String>;  // MAX_FILE_VERSION_BYTES
    pub fn mark_resolved(&self, path: String, force: bool) -> Result<MarkResolvedOutcome, String>;
}
```

- [ ] **Step 1: Write the failing worker tests** (in `worker/mod.rs`'s `#[cfg(test)] mod tests`, after `get_commit_signature_reports_unsigned_for_a_plain_commit`)

```rust
    #[test]
    fn workdir_is_the_work_tree_root_even_when_opened_from_a_subdirectory() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "sub/a.txt", "a");
        commit_all(&repo, "initial");
        let worker = Worker::spawn(dir.path().join("sub")).unwrap();
        assert_eq!(
            worker.handle().workdir().unwrap().canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn prepare_diff_tool_and_read_file_version_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "old");
        commit_all(&repo, "initial");
        write_file(dir.path(), "a.txt", "new");
        {
            let mut config = repo.config().unwrap();
            config.set_str("diff.tool", "t").unwrap();
            config.set_str("difftool.t.cmd", "true").unwrap();
        }
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let prepared = handle.prepare_diff_tool("a.txt".into(), git_core::tools::DiffSource::Unstaged).unwrap();
        let local = prepared.invocation.env.iter().find(|(k, _)| k == "LOCAL").unwrap().1.clone();
        assert_eq!(std::fs::read_to_string(local).unwrap(), "old");
        let _ = std::fs::remove_dir_all(&prepared.temp_dir);

        assert_eq!(
            handle.read_file_version("a.txt".into(), git_core::tools::FileVersion::Index).unwrap(),
            Some(b"old".to_vec())
        );
        assert!(handle.get_git_tools().unwrap().diff_tool.is_some());
    }

    #[test]
    fn mark_resolved_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "base\n");
        commit_all(&repo, "base");
        let main = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();
        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "a.txt", "theirs\n");
        commit_all(&repo, "feature");
        git_core::branch::switch_branch(&repo, &main).unwrap();
        write_file(dir.path(), "a.txt", "ours\n");
        commit_all(&repo, "main");
        git_core::merge::start_merge(&repo, "feature").unwrap();

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        assert_eq!(handle.mark_resolved("a.txt".into(), false).unwrap(), git_core::merge::MarkResolvedOutcome::MarkersRemain);
        assert_eq!(handle.mark_resolved("a.txt".into(), true).unwrap(), git_core::merge::MarkResolvedOutcome::Resolved);
    }
```

Use the module's existing test helpers (`init_repo`, `write_file`, `commit_all`). If their names differ, use the ones `stage_then_commit_round_trips_through_the_worker` uses.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p repo-service worker::tests::`
Expected: FAIL (no method `workdir`).

- [ ] **Step 3: Implement**

`Command` variants (after `ResolveAddDeleteConflict`):

```rust
    PrepareDiffTool { path: String, source: git_core::tools::DiffSource, reply: Sender<Result<git_core::tools::PreparedDiffTool, String>> },
    PrepareMergeTool { path: String, reply: Sender<Result<git_core::tools::PreparedMergeTool, String>> },
    GetGitTools { reply: Sender<Result<git_core::tools::GitTools, String>> },
    ReadFileVersion { path: String, version: git_core::tools::FileVersion, reply: Sender<Result<Option<Vec<u8>>, String>> },
    MarkResolved { path: String, force: bool, reply: Sender<Result<git_core::merge::MarkResolvedOutcome, String>> },
```

Dispatch arms:

```rust
                    Command::PrepareDiffTool { path, source, reply } => tools::prepare_diff_tool(&repo, path, source, reply),
                    Command::PrepareMergeTool { path, reply } => tools::prepare_merge_tool(&repo, path, reply),
                    Command::GetGitTools { reply } => tools::git_tools(&repo, reply),
                    Command::ReadFileVersion { path, version, reply } => tools::read_file_version(&repo, path, version, reply),
                    Command::MarkResolved { path, force, reply } => merge::mark_resolved(&repo, path, force, reply),
```

`worker/tools.rs`:

```rust
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use git_core::tools::{DiffSource, FileVersion, GitTools, PreparedDiffTool, PreparedMergeTool, MAX_FILE_VERSION_BYTES};

use super::{Command, WorkerHandle};

pub(super) fn prepare_diff_tool(repo: &git2::Repository, path: String, source: DiffSource, reply: Sender<Result<PreparedDiffTool, String>>) {
    let _ = reply.send(git_core::tools::prepare_diff_tool(repo, &path, &source).map_err(|e| e.to_string()));
}
pub(super) fn prepare_merge_tool(repo: &git2::Repository, path: String, reply: Sender<Result<PreparedMergeTool, String>>) {
    let _ = reply.send(git_core::tools::prepare_merge_tool(repo, &path).map_err(|e| e.to_string()));
}
pub(super) fn git_tools(repo: &git2::Repository, reply: Sender<Result<GitTools, String>>) {
    let _ = reply.send(git_core::tools::git_tools(repo).map_err(|e| e.to_string()));
}
pub(super) fn read_file_version(repo: &git2::Repository, path: String, version: FileVersion, reply: Sender<Result<Option<Vec<u8>>, String>>) {
    let _ = reply.send(
        git_core::tools::file_version_contents(repo, &path, &version, MAX_FILE_VERSION_BYTES).map_err(|e| e.to_string()),
    );
}

impl WorkerHandle {
    /// Work tree root captured at spawn. No round trip to the worker.
    pub fn workdir(&self) -> Result<PathBuf, String> {
        self.workdir.clone().ok_or_else(|| "repository has no working directory (bare repository)".to_string())
    }

    /// Reads config and writes the temp side files on the worker. The caller spawns the tool,
    /// so the worker never waits on it.
    pub fn prepare_diff_tool(&self, path: String, source: DiffSource) -> Result<PreparedDiffTool, String> {
        self.request(|reply| Command::PrepareDiffTool { path, source, reply })
    }
    pub fn prepare_merge_tool(&self, path: String) -> Result<PreparedMergeTool, String> {
        self.request(|reply| Command::PrepareMergeTool { path, reply })
    }
    pub fn get_git_tools(&self) -> Result<GitTools, String> {
        self.request(|reply| Command::GetGitTools { reply })
    }
    pub fn read_file_version(&self, path: String, version: FileVersion) -> Result<Option<Vec<u8>>, String> {
        self.request(|reply| Command::ReadFileVersion { path, version, reply })
    }
}
```

If `WorkerHandle` has no generic `request` helper (check `grep -n "fn request" worker/mod.rs`), write each body in the same shape as `search_commits` (channel, `self.tx.send(...)`, the two error strings). Do not add a helper only for this batch.

`worker/merge.rs`: `mark_resolved` handler and `WorkerHandle::mark_resolved`, written exactly like `resolve_add_delete`/`resolve_add_delete_conflict` there.

`workdir`:
- In `spawn_with`, before the repo moves into the thread, add `let workdir = repo.workdir().map(std::path::Path::to_path_buf);`.
- Store it on `Worker`.
- Copy it into every `WorkerHandle { … }` literal. Find them with `grep -n "WorkerHandle {" crates/repo-service/src/worker/mod.rs`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p repo-service && cargo clippy -p repo-service --all-targets -- -D warnings`
Expected: PASS. In particular, `every_recordable_command_has_its_reply_held` still passes, because no new command is recordable.

- [ ] **Step 5: Commit**

```bash
git add crates/repo-service
git commit -m "feat(repo-service): worker commands for external tools and mark resolved"
```

---

### Task 8: Tauri commands and DTOs

**Files:**
- Create: `crates/tauri-app/src/commands/tools.rs`
- Modify:
  - `crates/tauri-app/src/commands/merge.rs`: `mark_resolved`.
  - `crates/tauri-app/src/commands/mod.rs`:
    - `mod tools;` + `pub use tools::{…};`.
    - DTOs after `ResetModeDto`'s `impl From`.
    - Tests after `diff_options_dto_parses_camel_case`.
  - `crates/tauri-app/src/main.rs`: the `use` list and `generate_handler!` after `open_external_url`.

**Interfaces:**
- Consumes: Tasks 2, 4, 6, 7.
- Produces: the Tauri side of the wire format in Global Constraints.

- [ ] **Step 1: Write the failing DTO tests** (add the new DTO names to the test module's `use super::{…}`)

```rust
    #[test]
    fn diff_source_dto_parses_every_variant() {
        use git_core::tools::DiffSource;
        let parse = |v: serde_json::Value| DiffSource::from(serde_json::from_value::<DiffSourceDto>(v).unwrap());
        assert_eq!(parse(serde_json::json!({"kind": "Unstaged"})), DiffSource::Unstaged);
        assert_eq!(parse(serde_json::json!({"kind": "Staged"})), DiffSource::Staged);
        assert_eq!(
            parse(serde_json::json!({"kind": "Commit", "commitId": "abc", "oldPath": null})),
            DiffSource::Commit { commit_id: "abc".into(), old_path: None }
        );
        assert_eq!(
            parse(serde_json::json!({"kind": "Range", "fromId": "a", "toId": "b", "oldPath": "old.rs"})),
            DiffSource::Range { from_id: "a".into(), to_id: "b".into(), old_path: Some("old.rs".into()) }
        );
    }

    #[test]
    fn tool_outcome_and_settings_dtos_serialize_for_the_frontend() {
        assert_eq!(serde_json::to_value(MergeToolOutcomeDto::from(git_core::tools::MergeToolOutcome::Resolved)).unwrap(), "Resolved");
        assert_eq!(
            serde_json::to_value(MarkResolvedOutcomeDto::from(git_core::merge::MarkResolvedOutcome::MarkersRemain)).unwrap(),
            "MarkersRemain"
        );
        let settings: ExternalToolSettingsDto = serde_json::from_value(serde_json::json!({"editor": "code", "terminal": null})).unwrap();
        assert_eq!(config::ExternalToolSettings::from(settings), config::ExternalToolSettings { editor: Some("code".into()), terminal: None });
        let tools = GitToolsDto::from(git_core::tools::GitTools {
            diff_tool: Some(git_core::tools::GitToolInfo {
                name: "meld".into(),
                command: Some("meld $LOCAL $REMOTE".into()),
                source: git_core::tools::ConfigSource::Global,
                via_merge_tool: true,
                problem: None,
            }),
            merge_tool: None,
        });
        assert_eq!(
            serde_json::to_value(tools).unwrap(),
            serde_json::json!({
                "diffTool": {"name": "meld", "command": "meld $LOCAL $REMOTE", "source": "Global", "viaMergeTool": true, "problem": null},
                "mergeTool": null
            })
        );
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p tauri-app`
Expected: FAIL (unknown DTOs).

- [ ] **Step 3: Implement DTOs** (`commands/mod.rs`)

```rust
#[derive(Deserialize)]
#[serde(tag = "kind")]
pub enum DiffSourceDto {
    Unstaged,
    Staged,
    #[serde(rename_all = "camelCase")]
    Commit { commit_id: String, old_path: Option<String> },
    #[serde(rename_all = "camelCase")]
    Range { from_id: String, to_id: String, old_path: Option<String> },
}

impl From<DiffSourceDto> for git_core::tools::DiffSource {
    fn from(dto: DiffSourceDto) -> Self {
        match dto {
            DiffSourceDto::Unstaged => Self::Unstaged,
            DiffSourceDto::Staged => Self::Staged,
            DiffSourceDto::Commit { commit_id, old_path } => Self::Commit { commit_id, old_path },
            DiffSourceDto::Range { from_id, to_id, old_path } => Self::Range { from_id, to_id, old_path },
        }
    }
}

#[derive(Serialize)]
pub enum MergeToolOutcomeDto { Resolved, Unresolved }
// From<git_core::tools::MergeToolOutcome>

#[derive(Serialize)]
pub enum MarkResolvedOutcomeDto { Resolved, MarkersRemain }
// From<git_core::merge::MarkResolvedOutcome>

#[derive(Serialize, Deserialize)]
pub struct ExternalToolSettingsDto { pub editor: Option<String>, pub terminal: Option<String> }
// From both ways with config::ExternalToolSettings

#[derive(Serialize)]
pub enum GitConfigSourceDto { System, Xdg, Global, Local, Worktree, Other }
// From<git_core::tools::ConfigSource>

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitToolDto { name: String, command: Option<String>, source: GitConfigSourceDto, via_merge_tool: bool, problem: Option<String> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitToolsDto { diff_tool: Option<GitToolDto>, merge_tool: Option<GitToolDto> }
// From<git_core::tools::GitTools>
```

Write each `// From…` as a plain `impl From` with an exhaustive `match` or field copy, like `ResetModeDto`'s.

- [ ] **Step 4: Implement the commands** (`commands/tools.rs`)

```rust
use git_core::launch::{self, EditorTarget};
use tauri::State;

use super::{worker_handle, AppState, DiffSourceDto, ExternalToolSettingsDto, GitToolsDto, MergeToolOutcomeDto};

fn tool_settings() -> Result<config::ExternalToolSettings, String> {
    config::get_external_tool_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_in_editor(repo_path: String, path: String, line: Option<u32>, state: State<'_, AppState>) -> Result<(), String> {
    let workdir = worker_handle(&state, &repo_path)?.workdir()?;
    let editor = tool_settings()?.editor;
    match launch::prepare_editor(&workdir, &path, line, editor.as_deref()).map_err(|e| e.to_string())? {
        EditorTarget::Command(invocation) => launch::run_invocation(&invocation, &workdir).map_err(|e| e.to_string()),
        EditorTarget::SystemDefault(file) => tauri_plugin_opener::open_path(file, None::<&str>).map_err(|e| e.to_string()),
    }
}

#[tauri::command]
pub async fn reveal_in_file_manager(repo_path: String, path: Option<String>, state: State<'_, AppState>) -> Result<(), String> {
    let workdir = worker_handle(&state, &repo_path)?.workdir()?;
    let target = launch::reveal_target(&workdir, path.as_deref()).map_err(|e| e.to_string())?;
    tauri_plugin_opener::reveal_item_in_dir(target).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_terminal(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    let workdir = worker_handle(&state, &repo_path)?.workdir()?;
    let terminal = tool_settings()?.terminal;
    let invocation = launch::prepare_terminal(&workdir, terminal.as_deref()).map_err(|e| e.to_string())?;
    launch::run_invocation(&invocation, &workdir).map_err(|e| e.to_string())
}

/// Prepares on the repo worker, then launches here. Returns after the diff tool's 1 s grace
/// period at the latest.
#[tauri::command]
pub async fn open_diff_tool(repo_path: String, path: String, source: DiffSourceDto, state: State<'_, AppState>) -> Result<(), String> {
    let prepared = worker_handle(&state, &repo_path)?.prepare_diff_tool(path, source.into())?;
    tauri::async_runtime::spawn_blocking(move || git_core::tools::launch_diff_tool(prepared, git_core::tools::DIFF_TOOL_GRACE))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Waits for the merge tool on a blocking thread, never on the repo worker, which stays free
/// for other commands while the tool window is open.
#[tauri::command]
pub async fn open_merge_tool(repo_path: String, path: String, state: State<'_, AppState>) -> Result<MergeToolOutcomeDto, String> {
    let prepared = worker_handle(&state, &repo_path)?.prepare_merge_tool(path)?;
    let outcome = tauri::async_runtime::spawn_blocking(move || git_core::tools::run_merge_tool(prepared))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(outcome.into())
}

#[tauri::command]
pub fn get_external_tool_settings() -> Result<ExternalToolSettingsDto, String> {
    tool_settings().map(Into::into)
}

#[tauri::command]
pub fn set_external_tool_settings(settings: ExternalToolSettingsDto) -> Result<(), String> {
    config::set_external_tool_settings(&settings.into()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_git_tools(repo_path: String, state: State<'_, AppState>) -> Result<GitToolsDto, String> {
    Ok(worker_handle(&state, &repo_path)?.get_git_tools()?.into())
}
```

`get_external_tool_settings`/`set_external_tool_settings` are sync on purpose, like `get_diff_settings`/`set_diff_settings`. They only do a small file read or write.

`commands/merge.rs`:

```rust
#[tauri::command]
pub async fn mark_resolved(repo_path: String, path: String, force: bool, state: State<'_, AppState>) -> Result<MarkResolvedOutcomeDto, String> {
    Ok(worker_handle(&state, &repo_path)?.mark_resolved(path, force)?.into())
}
```

Register all nine in `main.rs` (the `use commands::{…}` list, alphabetical, and `generate_handler!` after `open_external_url`). No capability change is needed: `open_path`/`reveal_item_in_dir` are called from Rust.

- [ ] **Step 5: Run to verify pass and the build**

Run: `cargo test -p tauri-app && cargo build -p tauri-app && cargo clippy -p tauri-app --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/tauri-app
git commit -m "feat(tauri-app): commands for editor, file manager, terminal and git tools"
```

---

### Task 9: Sidecar methods

**Files:**
- Modify:
  - `crates/vscode-sidecar/src/dispatch.rs`:
    - Method table after `"set_diff_settings"`: `get_external_tool_settings`, `set_external_tool_settings`, `get_git_tools`, `mark_resolved`, `read_file_version`, `repo_workdir`.
    - Functions and DTOs after `fn set_diff_settings`.
  - `crates/vscode-sidecar/tests/protocol_roundtrip.rs` (new tests after `search_commits_and_cancel_search_round_trip_through_the_sidecar`).

**Interfaces:**
- Consumes: Tasks 6, 7.
- Produces: wire methods exactly as Global Constraints. `read_file_version` params `{ repoPath, path, version }`, where `version` is `{"kind":"Head"}`, `{"kind":"Index"}`, `{"kind":"Stage","stage":2}`, `{"kind":"Commit","commitId":…}` or `{"kind":"FirstParent","commitId":…}`. The result is `{ "text": string | null }`, with lossy UTF-8.

- [ ] **Step 1: Write the failing round-trip tests**

```rust
#[test]
fn external_tool_settings_and_git_tools_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    repo.config().unwrap().set_str("diff.tool", "meld").unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let set = sidecar.call(2, "set_external_tool_settings", serde_json::json!({"settings": {"editor": "code", "terminal": null}}));
    assert_eq!(set["result"], serde_json::Value::Null);
    let got = sidecar.call(3, "get_external_tool_settings", serde_json::json!({}));
    assert_eq!(got["result"], serde_json::json!({"editor": "code", "terminal": null}));

    let tools = sidecar.call(4, "get_git_tools", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(tools["result"]["diffTool"]["name"], "meld");
    assert_eq!(tools["result"]["diffTool"]["source"], "Local");
}

#[test]
fn read_file_version_and_repo_workdir_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "hello\n");
    commit_all(&repo, "base");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let head = sidecar.call(2, "read_file_version", serde_json::json!({"repoPath": repo_path, "path": "a.txt", "version": {"kind": "Head"}}));
    assert_eq!(head["result"], serde_json::json!({"text": "hello\n"}));
    let missing = sidecar.call(3, "read_file_version", serde_json::json!({"repoPath": repo_path, "path": "nope.txt", "version": {"kind": "Index"}}));
    assert_eq!(missing["result"], serde_json::json!({"text": null}));
    let workdir = sidecar.call(4, "repo_workdir", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(
        std::path::Path::new(workdir["result"].as_str().unwrap()).canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
}

#[test]
fn mark_resolved_round_trips_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    // git-core is a regular dependency of this crate, so the test can use it for setup.
    let main = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "a.txt", "theirs\n");
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    write_file(dir.path(), "a.txt", "ours\n");
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature").unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let blocked = sidecar.call(2, "mark_resolved", serde_json::json!({"repoPath": repo_path, "path": "a.txt", "force": false}));
    assert_eq!(blocked["result"], "MarkersRemain");
    write_file(dir.path(), "a.txt", "merged\n");
    let outcome = sidecar.call(3, "mark_resolved", serde_json::json!({"repoPath": repo_path, "path": "a.txt", "force": false}));
    assert_eq!(outcome["result"], "Resolved");
}
```

If this file's `commit_all` stages with `add_all` the same way as `git-core`'s helper, the setup above works unchanged. Otherwise stage `a.txt` explicitly before each commit.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p vscode-sidecar --test protocol_roundtrip`
Expected: FAIL (`method not found` errors in the new tests).

- [ ] **Step 3: Implement**

The DTOs are copies of Task 8's, by the sidecar's convention (`GitToolsDto`, `GitToolDto`, `GitConfigSourceDto`, `ExternalToolSettingsDto`, `MarkResolvedOutcomeDto`). Add:

```rust
#[derive(Deserialize)]
#[serde(tag = "kind")]
enum FileVersionDto {
    Head,
    Index,
    Stage { stage: u8 },
    #[serde(rename_all = "camelCase")]
    Commit { commit_id: String },
    #[serde(rename_all = "camelCase")]
    FirstParent { commit_id: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadFileVersionParams { repo_path: String, path: String, version: FileVersionDto }

fn read_file_version(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: ReadFileVersionParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
    let version = match params.version {
        FileVersionDto::Head => FileVersion::Head,
        FileVersionDto::Index => FileVersion::Index,
        FileVersionDto::Stage { stage } if (1..=3).contains(&stage) => FileVersion::Stage(stage),
        FileVersionDto::Stage { .. } => return Err("stage must be 1, 2 or 3".into()),
        FileVersionDto::Commit { commit_id } => FileVersion::Commit(commit_id),
        FileVersionDto::FirstParent { commit_id } => FileVersion::FirstParent(commit_id),
    };
    let bytes = worker_handle(repos, &params.repo_path)?.read_file_version(params.path, version)?;
    Ok(serde_json::json!({ "text": bytes.map(|b| String::from_utf8_lossy(&b).into_owned()) }))
}

fn repo_workdir(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
    let workdir = worker_handle(repos, &params.repo_path)?.workdir()?;
    Ok(Value::String(workdir.to_string_lossy().into_owned()))
}
```

Use the file's existing repo-path-only params struct for `repo_workdir`/`get_git_tools` (find it with `grep -n "struct .*RepoPath" dispatch.rs`, e.g. the one `cancel_search` uses). `get_external_tool_settings` takes no params. `set_external_tool_settings` takes `{ settings }`. `mark_resolved` takes `{ repoPath, path, force }`. Write all four in the style of `get_diff_settings`/`set_diff_settings`/`resolve_add_delete_conflict`.

Add a doc comment above `read_file_version`/`repo_workdir`: internal, called by the extension host's `browsitory-file:` content provider and native tool methods, not part of `RepoClient`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p vscode-sidecar && cargo clippy -p vscode-sidecar --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vscode-sidecar
git commit -m "feat(vscode-sidecar): tool settings, git tools, mark resolved and file versions"
```

---

### Task 10: `RepoClient` types and both clients

**Files:**
- Modify:
  - `frontend/src/ipc/RepoClient.ts`: types after `FileConflictChoice`; methods after `openExternalUrl`.
  - `frontend/src/ipc/tauriRepoClient.ts`: after `openExternalUrl`.
  - `frontend/src/ipc/vscodeRepoClient.ts`: after `openExternalUrl`.
- Test: `frontend/src/ipc/tauriRepoClient.test.ts`, `frontend/src/ipc/vscodeRepoClient.test.ts`
- Fixtures: every full `RepoClient` literal that `tsc` reports: `App.test.tsx` `fakeClient`, `useAppState.test.ts`, `DiffPane.test.tsx`, `ConflictResolutionPane.test.tsx`, `RebasePlanner.test.tsx`, `RepoPicker.test.tsx`, `lib/commands.test.ts`, and any others.

**Interfaces:**
- Produces (Tasks 11–14): exactly the spec's "`RepoClient`" block.

- [ ] **Step 1: Write the failing client tests**

`tauriRepoClient.test.ts`:

```ts
  it("invokes the external tool commands with camelCase args", async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    await tauriRepoClient.openInEditor("/repo", "src/a.ts", 12);
    expect(invoke).toHaveBeenLastCalledWith("open_in_editor", { repoPath: "/repo", path: "src/a.ts", line: 12 });
    await tauriRepoClient.revealInFileManager("/repo", null);
    expect(invoke).toHaveBeenLastCalledWith("reveal_in_file_manager", { repoPath: "/repo", path: null });
    await tauriRepoClient.openTerminal("/repo");
    expect(invoke).toHaveBeenLastCalledWith("open_terminal", { repoPath: "/repo" });
    await tauriRepoClient.openDiffTool("/repo", "a.ts", { kind: "Commit", commitId: "abc", oldPath: null });
    expect(invoke).toHaveBeenLastCalledWith("open_diff_tool", {
      repoPath: "/repo",
      path: "a.ts",
      source: { kind: "Commit", commitId: "abc", oldPath: null },
    });
    vi.mocked(invoke).mockResolvedValueOnce("Resolved");
    await expect(tauriRepoClient.openMergeTool("/repo", "a.ts")).resolves.toBe("Resolved");
    vi.mocked(invoke).mockResolvedValueOnce("MarkersRemain");
    await expect(tauriRepoClient.markResolved("/repo", "a.ts", false)).resolves.toBe("MarkersRemain");
    expect(invoke).toHaveBeenLastCalledWith("mark_resolved", { repoPath: "/repo", path: "a.ts", force: false });
    await tauriRepoClient.setExternalToolSettings({ editor: "code", terminal: null });
    expect(invoke).toHaveBeenLastCalledWith("set_external_tool_settings", { settings: { editor: "code", terminal: null } });
    await tauriRepoClient.getGitTools("/repo");
    expect(invoke).toHaveBeenLastCalledWith("get_git_tools", { repoPath: "/repo" });
  });
```

`vscodeRepoClient.test.ts`: extend the existing table test that holds the `[() => vscodeRepoClient.openExternalUrl(…), "open_external_url", …]` row with:

```ts
      [() => vscodeRepoClient.openInEditor("/r", "a.ts", null), "open_in_editor", { repoPath: "/r", path: "a.ts", line: null }, null],
      [() => vscodeRepoClient.revealInFileManager("/r", "a.ts"), "reveal_in_file_manager", { repoPath: "/r", path: "a.ts" }, null],
      [() => vscodeRepoClient.openTerminal("/r"), "open_terminal", { repoPath: "/r" }, null],
      [() => vscodeRepoClient.openDiffTool("/r", "a.ts", { kind: "Staged" }), "open_diff_tool", { repoPath: "/r", path: "a.ts", source: { kind: "Staged" } }, null],
      [() => vscodeRepoClient.openMergeTool("/r", "a.ts"), "open_merge_tool", { repoPath: "/r", path: "a.ts" }, "HandedOff"],
      [() => vscodeRepoClient.markResolved("/r", "a.ts", true), "mark_resolved", { repoPath: "/r", path: "a.ts", force: true }, "Resolved"],
      [() => vscodeRepoClient.getExternalToolSettings(), "get_external_tool_settings", {}, { editor: null, terminal: null }],
      [() => vscodeRepoClient.setExternalToolSettings({ editor: null, terminal: "kitty" }), "set_external_tool_settings", { settings: { editor: null, terminal: "kitty" } }, null],
      [() => vscodeRepoClient.getGitTools("/r"), "get_git_tools", { repoPath: "/r" }, { diffTool: null, mergeTool: null }],
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/ipc`
Expected: FAIL (`openInEditor is not a function`).

- [ ] **Step 3: Implement**

`RepoClient.ts`: add the types, and the methods with doc comments:
- `openMergeTool` resolves when the tool exits on desktop and at once with `"HandedOff"` in VSCode.
- `markResolved` with `force: false` never stages a file that still has conflict markers.
- `getGitTools` reads `diff.tool`/`merge.tool` from the repo's effective git config.
- The five launch methods are answered by the VSCode extension host itself (see `sidecarBridge.ts`'s `NATIVE_METHODS`).

`tauriRepoClient.ts`: `loggedInvoke` for all nine. For example: `openInEditor: (repoPath, path, line) => loggedInvoke("open_in_editor", { repoPath, path, line })`, `openMergeTool: (repoPath, path) => loggedInvoke<MergeToolOutcome>("open_merge_tool", { repoPath, path })`.

`vscodeRepoClient.ts`: `call<…>(method, params)` for all nine, with a comment above the five launch methods: "Handled natively by the extension host (see `sidecarBridge.ts`'s `NATIVE_METHODS`)".

- [ ] **Step 4: Fix full `RepoClient` fixtures**

Run: `cd frontend && pnpm exec tsc --noEmit -p .`
Add to every literal that errors:

```ts
    openInEditor: async () => {},
    revealInFileManager: async () => {},
    openTerminal: async () => {},
    openDiffTool: async () => {},
    openMergeTool: async () => "Unresolved" as const,
    markResolved: async () => "Resolved" as const,
    getExternalToolSettings: async () => ({ editor: null, terminal: null }),
    setExternalToolSettings: async () => {},
    getGitTools: async () => ({ diffTool: null, mergeTool: null }),
```

In `App.test.tsx`'s `fakeClient`, use `unused` for the five launch methods and `markResolved`, and the async defaults above for the three settings/info methods, because `PreferencesDialog` loads them. Repeat until `tsc` is clean.

- [ ] **Step 5: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): RepoClient methods for external tools in both clients"
```

---

### Task 11: VSCode extension host: native launch methods and content provider

**Files:**
- Create: `extension/src/toolUris.ts`, `extension/src/toolUris.test.ts`
- Modify: `extension/src/sidecarBridge.ts`, `extension/src/sidecarBridge.test.ts`, `extension/src/extension.ts`

**Interfaces:**
- Consumes: sidecar `read_file_version` / `repo_workdir` (Task 9); wire format from Task 10.
- Produces:

```ts
// toolUris.ts
export const FILE_VERSION_SCHEME = "browsitory-file";
export type FileVersion =
  | { kind: "Head" } | { kind: "Index" } | { kind: "Stage"; stage: 1 | 2 | 3 }
  | { kind: "Commit"; commitId: string } | { kind: "FirstParent"; commitId: string };
export type DiffSource =
  | { kind: "Unstaged" } | { kind: "Staged" }
  | { kind: "Commit"; commitId: string; oldPath: string | null }
  | { kind: "Range"; fromId: string; toId: string; oldPath: string | null };
export interface FileVersionRef { repoPath: string; path: string; version: FileVersion }
export function resolveRepoFile(workdir: string, path: string): string;               // throws "Invalid path."
export function fileVersionUri(ref: FileVersionRef): string;                          // "browsitory-file:/<path>?<encoded json>"
export function parseFileVersionUri(uri: { path: string; query: string }): FileVersionRef;
export function diffSides(repoPath: string, workdir: string, path: string, source: DiffSource):
  { left: string; right: string; title: string };                                     // URI strings; "file://…" for the work tree
export function mergeSides(repoPath: string, workdir: string, path: string):
  { base: string; ours: string; theirs: string; output: string };

// SidecarBridgeDependencies additions
readonly openFile: (path: string, line: number | null) => PromiseLike<unknown>;
readonly revealInOS: (path: string) => PromiseLike<unknown>;
readonly createTerminal: (cwd: string, name: string) => void;
readonly openDiff: (left: string, right: string, title: string) => PromiseLike<unknown>;
readonly openMergeEditor: (sides: { base: string; ours: string; theirs: string; output: string }) => PromiseLike<boolean>; // false = fell back to plain open

// SidecarBridge
readFileVersion(ref: FileVersionRef): Promise<string>;  // "" for a missing version
```

The frontend's `RepoClient.DiffSource` is duplicated here on purpose. The extension never imports from `frontend/`, the same as the sidecar keeps its own DTO copies.

- [ ] **Step 1: Write the failing `toolUris` tests**

```ts
import { describe, expect, it } from "vitest";
import { diffSides, fileVersionUri, mergeSides, parseFileVersionUri, resolveRepoFile } from "./toolUris";

const split = (uri: string) => {
  const url = new URL(uri);
  return { path: decodeURIComponent(url.pathname), query: url.search.slice(1) };
};

describe("toolUris", () => {
  it("round-trips a file version through its URI", () => {
    const ref = { repoPath: "/repos/a b", path: "src/x y.ts", version: { kind: "Commit", commitId: "abc" } } as const;
    const uri = fileVersionUri(ref);
    expect(uri.startsWith("browsitory-file:/src/x%20y.ts?")).toBe(true);
    expect(parseFileVersionUri(split(uri))).toEqual(ref);
  });

  it("rejects paths that escape the work tree", () => {
    expect(resolveRepoFile("/repo", "src/a.ts")).toBe("/repo/src/a.ts");
    for (const bad of ["", "../x", "a/../../x", "/etc/passwd", "C:\\x"]) {
      expect(() => resolveRepoFile("/repo", bad)).toThrow("Invalid path.");
    }
  });

  it("maps each diff source to its two sides", () => {
    const unstaged = diffSides("/r", "/r", "a.ts", { kind: "Unstaged" });
    expect(parseFileVersionUri(split(unstaged.left)).version).toEqual({ kind: "Index" });
    expect(unstaged.right).toBe("file:///r/a.ts");
    expect(unstaged.title).toBe("a.ts (Index ↔ Working Tree)");

    const staged = diffSides("/r", "/r", "a.ts", { kind: "Staged" });
    expect(parseFileVersionUri(split(staged.left)).version).toEqual({ kind: "Head" });
    expect(parseFileVersionUri(split(staged.right)).version).toEqual({ kind: "Index" });
    expect(staged.title).toBe("a.ts (HEAD ↔ Index)");

    const commit = diffSides("/r", "/r", "b.ts", { kind: "Commit", commitId: "abcdef123", oldPath: "a.ts" });
    expect(parseFileVersionUri(split(commit.left))).toEqual({ repoPath: "/r", path: "a.ts", version: { kind: "FirstParent", commitId: "abcdef123" } });
    expect(commit.title).toBe("b.ts (abcdef1 ↔ parent)");

    const range = diffSides("/r", "/r", "b.ts", { kind: "Range", fromId: "1111111aa", toId: "2222222bb", oldPath: null });
    expect(parseFileVersionUri(split(range.left)).version).toEqual({ kind: "Commit", commitId: "1111111aa" });
    expect(range.title).toBe("b.ts (1111111 ↔ 2222222)");
  });

  it("maps merge sides to the conflict stages and the work tree file", () => {
    const sides = mergeSides("/r", "/r", "a.ts");
    expect(parseFileVersionUri(split(sides.base)).version).toEqual({ kind: "Stage", stage: 1 });
    expect(parseFileVersionUri(split(sides.ours)).version).toEqual({ kind: "Stage", stage: 2 });
    expect(parseFileVersionUri(split(sides.theirs)).version).toEqual({ kind: "Stage", stage: 3 });
    expect(sides.output).toBe("file:///r/a.ts");
  });
});
```

- [ ] **Step 2: Write the failing bridge tests** (`sidecarBridge.test.ts`)

- Extend `createBridge` with `openFile`, `revealInOS`, `createTerminal`, `openDiff` and `openMergeEditor` as `vi.fn`s. `openMergeEditor` resolves `overrides.mergeEditorOpened ?? true`. Return them from `createBridge`.
- Rename the test "routes the five VSCode-native methods without spawning the sidecar" to "routes the native picker, URL and version methods without spawning the sidecar". Its body stays the same.

Add:

```ts
  it("opens files, terminals, diffs and merges through VSCode using the sidecar's work tree root", async () => {
    const { bridge, child, postToWebview, openFile, revealInOS, createTerminal, openDiff, openMergeEditor } = createBridge();
    const answerWorkdir = async (index: number) => {
      await vi.waitFor(() => expect(child.stdin.write).toHaveBeenCalledTimes(index + 1));
      const request = writtenRequest(child, index);
      expect(request).toMatchObject({ method: "repo_workdir", params: { repoPath: "/repos/a/sub" } });
      child.stdout.push(`${JSON.stringify({ jsonrpc: "2.0", id: request.id, result: "/repos/a" })}\n`);
    };

    const editor = bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 1, method: "open_in_editor", params: { repoPath: "/repos/a/sub", path: "src/x.ts", line: 4 } });
    await completePersistedRepositoryRestore(child);
    await answerWorkdir(1);
    await editor;
    expect(openFile).toHaveBeenCalledWith("/repos/a/src/x.ts", 4);

    const reveal = bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 2, method: "reveal_in_file_manager", params: { repoPath: "/repos/a/sub", path: null } });
    await answerWorkdir(2);
    await reveal;
    expect(revealInOS).toHaveBeenCalledWith("/repos/a");

    const terminal = bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 3, method: "open_terminal", params: { repoPath: "/repos/a/sub" } });
    await answerWorkdir(3);
    await terminal;
    expect(createTerminal).toHaveBeenCalledWith("/repos/a", "Browsitory: a");

    const diff = bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 4, method: "open_diff_tool", params: { repoPath: "/repos/a/sub", path: "x.ts", source: { kind: "Unstaged" } } });
    await answerWorkdir(4);
    await diff;
    expect(openDiff).toHaveBeenCalledWith(expect.stringMatching(/^browsitory-file:\/x\.ts\?/), "file:///repos/a/x.ts", "x.ts (Index ↔ Working Tree)");

    const merge = bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 5, method: "open_merge_tool", params: { repoPath: "/repos/a/sub", path: "x.ts" } });
    await answerWorkdir(5);
    await merge;
    expect(openMergeEditor).toHaveBeenCalledWith(expect.objectContaining({ output: "file:///repos/a/x.ts" }));

    expect(postToWebview.mock.calls.map(([m]) => m)).toEqual([
      { jsonrpc: "2.0", id: 1, result: null },
      { jsonrpc: "2.0", id: 2, result: null },
      { jsonrpc: "2.0", id: 3, result: null },
      { jsonrpc: "2.0", id: 4, result: null },
      { jsonrpc: "2.0", id: 5, result: "HandedOff" },
    ]);
  });

  it("rejects a launch path that escapes the work tree with invalid params", async () => {
    const { bridge, child, postToWebview, openFile } = createBridge();
    const pending = bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 9, method: "open_in_editor", params: { repoPath: "/r", path: "../x", line: null } });
    await completePersistedRepositoryRestore(child);
    await vi.waitFor(() => expect(child.stdin.write).toHaveBeenCalledTimes(2));
    const request = writtenRequest(child, 1);
    child.stdout.push(`${JSON.stringify({ jsonrpc: "2.0", id: request.id, result: "/r" })}\n`);
    await pending;
    expect(openFile).not.toHaveBeenCalled();
    expect(postToWebview).toHaveBeenCalledWith({ jsonrpc: "2.0", id: 9, error: { code: -32602, message: "Invalid path." } });
  });

  it("serves file versions to the content provider through the sidecar", async () => {
    const { bridge, child } = createBridge();
    const text = bridge.readFileVersion({ repoPath: "/r", path: "a.ts", version: { kind: "Head" } });
    await completePersistedRepositoryRestore(child);
    await vi.waitFor(() => expect(child.stdin.write).toHaveBeenCalledTimes(2));
    const request = writtenRequest(child, 1);
    expect(request).toMatchObject({ method: "read_file_version", params: { repoPath: "/r", path: "a.ts", version: { kind: "Head" } } });
    child.stdout.push(`${JSON.stringify({ jsonrpc: "2.0", id: request.id, result: { text: null } })}\n`);
    await expect(text).resolves.toBe("");
  });
```

`completePersistedRepositoryRestore` expects the first write to be `list_open_repos`. If the bridge does not restore on the very first spawn (check `restoreRepositoriesOnNextSpawn`'s initial value), drop those calls and use index 0 for the first `repo_workdir`/`read_file_version` request.

- [ ] **Step 3: Run to verify failure**

Run: `cd extension && pnpm test -- --run`
Expected: FAIL.

- [ ] **Step 4: Implement `toolUris.ts`**

```ts
import * as path from "node:path";

export const FILE_VERSION_SCHEME = "browsitory-file";
// … types as in Interfaces …

export function resolveRepoFile(workdir: string, rel: string): string {
  const parts = rel.split(/[\\/]/);
  if (
    rel === "" ||
    path.posix.isAbsolute(rel) ||
    path.win32.isAbsolute(rel) ||
    parts.some((p) => p === "" || p === "." || p === "..")
  ) {
    throw new Error("Invalid path.");
  }
  return path.join(workdir, ...parts);
}

export function fileVersionUri(ref: FileVersionRef): string {
  const encodedPath = ref.path.split("/").map(encodeURIComponent).join("/");
  return `${FILE_VERSION_SCHEME}:/${encodedPath}?${encodeURIComponent(JSON.stringify(ref))}`;
}

export function parseFileVersionUri(uri: { path: string; query: string }): FileVersionRef {
  const ref: unknown = JSON.parse(decodeURIComponent(uri.query));
  if (typeof ref !== "object" || ref === null || typeof (ref as FileVersionRef).repoPath !== "string" || typeof (ref as FileVersionRef).path !== "string") {
    throw new Error("Malformed browsitory-file URI.");
  }
  return ref as FileVersionRef;
}

function fileUri(absolute: string): string {
  return "file://" + absolute.split(path.sep).map(encodeURIComponent).join("/").replace(/^([^/])/, "/$1");
}

const short = (id: string) => id.slice(0, 7);

export function diffSides(repoPath: string, workdir: string, rel: string, source: DiffSource) {
  const name = path.posix.basename(rel);
  const at = (p: string, version: FileVersion) => fileVersionUri({ repoPath, path: p, version });
  switch (source.kind) {
    case "Unstaged":
      return { left: at(rel, { kind: "Index" }), right: fileUri(resolveRepoFile(workdir, rel)), title: `${name} (Index ↔ Working Tree)` };
    case "Staged":
      return { left: at(rel, { kind: "Head" }), right: at(rel, { kind: "Index" }), title: `${name} (HEAD ↔ Index)` };
    case "Commit":
      return {
        left: at(source.oldPath ?? rel, { kind: "FirstParent", commitId: source.commitId }),
        right: at(rel, { kind: "Commit", commitId: source.commitId }),
        title: `${name} (${short(source.commitId)} ↔ parent)`,
      };
    case "Range":
      return {
        left: at(source.oldPath ?? rel, { kind: "Commit", commitId: source.fromId }),
        right: at(rel, { kind: "Commit", commitId: source.toId }),
        title: `${name} (${short(source.fromId)} ↔ ${short(source.toId)})`,
      };
  }
}

export function mergeSides(repoPath: string, workdir: string, rel: string) {
  const stage = (s: 1 | 2 | 3) => fileVersionUri({ repoPath, path: rel, version: { kind: "Stage", stage: s } });
  return { base: stage(1), ours: stage(2), theirs: stage(3), output: fileUri(resolveRepoFile(workdir, rel)) };
}
```

`fileUri` must produce `file:///r/a.ts` on POSIX and `file:///C:/r/a.ts` on Windows. In `extension.ts`, convert with `vscode.Uri.parse` (do not rebuild URIs by hand there).

- [ ] **Step 5: Implement the bridge**

- Add the five method names to `NATIVE_METHODS`.
- Add the dependency fields from Interfaces.
- `private async workdir(repoPath: string): Promise<string>`:
  - `const sidecar = await this.ensureSidecar();`
  - throw `new Error("Browsitory sidecar is not running")` when it is undefined
  - `return requireString(await this.callSidecar(sidecar, "repo_workdir", { repoPath }))`
- `public async readFileVersion(ref)`: the same `ensureSidecar` + `callSidecar("read_file_version", ref)`. Return `result.text ?? ""`.
- New `handleNativeRequest` cases:

```ts
        case "open_in_editor": {
          const { repoPath, path: rel, line } = requireParams(request, { repoPath: "string", path: "string", line: "number|null" });
          const file = this.repoFile(await this.workdir(repoPath), rel);
          await this.dependencies.openFile(file, line);
          result = null;
          break;
        }
        case "reveal_in_file_manager": {
          const { repoPath, path: rel } = requireParams(request, { repoPath: "string", path: "string|null" });
          const workdir = await this.workdir(repoPath);
          await this.dependencies.revealInOS(rel === null ? workdir : this.repoFile(workdir, rel));
          result = null;
          break;
        }
        case "open_terminal": {
          const { repoPath } = requireParams(request, { repoPath: "string" });
          const workdir = await this.workdir(repoPath);
          this.dependencies.createTerminal(workdir, `Browsitory: ${path.basename(workdir)}`);
          result = null;
          break;
        }
        case "open_diff_tool": {
          const { repoPath, path: rel } = requireParams(request, { repoPath: "string", path: "string", source: "object" });
          const source = requireDiffSource(request.params["source"]);
          const workdir = await this.workdir(repoPath);
          this.repoFile(workdir, rel);
          const sides = diffSides(repoPath, workdir, rel, source);
          await this.dependencies.openDiff(sides.left, sides.right, sides.title);
          result = null;
          break;
        }
        case "open_merge_tool": {
          const { repoPath, path: rel } = requireParams(request, { repoPath: "string", path: "string" });
          const workdir = await this.workdir(repoPath);
          this.repoFile(workdir, rel);
          await this.dependencies.openMergeEditor(mergeSides(repoPath, workdir, rel));
          result = "HandedOff";
          break;
        }
```

- `repoFile(workdir, rel)` wraps `resolveRepoFile` and rethrows its error as `InvalidParamsError("Invalid path.")`.
- `requireParams(request, shape)` is a small validator next to `requireStringParam`. Exact keys, typed `"string" | "number|null" | "string|null" | "object"`. Throws `InvalidParamsError(method + " requires …")`.
- `requireDiffSource` checks `kind` and the per-kind string/null fields and throws `InvalidParamsError`.

- [ ] **Step 6: Implement `extension.ts`**

- On activation, before the `browsitory.open` command, register the content provider:

```ts
  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(FILE_VERSION_SCHEME, {
      provideTextDocumentContent: async (uri) => {
        if (!currentBridge) throw new Error("Browsitory is not open.");
        return currentBridge.readFileVersion(parseFileVersionUri(uri));
      },
    }),
  );
```

- Pass these new dependencies to `SidecarBridge`:

```ts
      openFile: (file, line) =>
        vscode.commands.executeCommand("vscode.open", vscode.Uri.file(file), {
          preview: false,
          selection: line === null ? undefined : new vscode.Range(line - 1, 0, line - 1, 0),
        }),
      revealInOS: (target) => vscode.commands.executeCommand("revealFileInOS", vscode.Uri.file(target)),
      createTerminal: (cwd, name) => vscode.window.createTerminal({ cwd, name }).show(),
      openDiff: (left, right, title) =>
        vscode.commands.executeCommand("vscode.diff", vscode.Uri.parse(left, true), vscode.Uri.parse(right, true), title),
      openMergeEditor: async (sides) => {
        const output = vscode.Uri.parse(sides.output, true);
        try {
          // Internal command (the one VSCode's own Git extension uses); fall back to a plain open.
          await vscode.commands.executeCommand("_open.mergeEditor", {
            base: vscode.Uri.parse(sides.base, true),
            input1: { uri: vscode.Uri.parse(sides.ours, true), title: "Ours" },
            input2: { uri: vscode.Uri.parse(sides.theirs, true), title: "Theirs" },
            output,
          });
          return true;
        } catch {
          await vscode.commands.executeCommand("vscode.open", output);
          return false;
        }
      },
```

- [ ] **Step 7: Run to verify pass and compile**

Run: `cd extension && pnpm test -- --run && pnpm run compile && pnpm run lint`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add extension/src
git commit -m "feat(extension): open files, diffs, merges and terminals in VSCode"
```

---

### Task 12: Frontend state: `useExternalTools`, `useExternalToolSettings`, `markResolved`

**Files:**
- Create:
  - `frontend/src/state/useExternalTools.ts`
  - `frontend/src/state/useExternalTools.test.ts`
  - `frontend/src/state/useExternalToolSettings.ts`
  - `frontend/src/state/useExternalToolSettings.test.ts`
- Modify:
  - `frontend/src/state/useMergeRebaseActions.ts`: `markResolved`.
  - `frontend/src/state/useAppState.ts`: expose `markResolved` in `UseAppStateResult` and the return object.
  - `frontend/src/state/useAppState.test.ts`: a `markResolved` test.

**Interfaces:**
- Consumes: Task 10's `RepoClient` methods.
- Produces (Tasks 13, 14):

```ts
export interface ExternalToolsApi {
  openInEditor(path: string, line: number | null): void;
  reveal(path: string | null): void;
  openTerminal(): void;
  openDiffTool(path: string, source: DiffSource): void;
  openMergeTool(path: string): void;
  mergeToolRunning: ReadonlySet<string>;
  mergeToolResults: ReadonlyMap<string, "Resolved" | "Unresolved">;
  dismissMergeToolResult(path: string): void;
  error: string | null;
  dismissError(): void;
}
export function useExternalTools(client: RepoClient, repoPath: string, conflictedPaths: readonly string[], refresh: () => Promise<void>): ExternalToolsApi;

export interface ExternalToolSettingsApi {
  settings: ExternalToolSettings | null;     // null while loading
  save(next: ExternalToolSettings): Promise<boolean>;
  error: string | null;
}
export function useExternalToolSettings(client: RepoClient): ExternalToolSettingsApi;

// UseAppStateResult
markResolved(path: string, force: boolean): Promise<MarkResolvedOutcome | null>;
```

- [ ] **Step 1: Write the failing tests**

`useExternalTools.test.ts`:

```ts
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { useExternalTools } from "./useExternalTools";

function client(overrides: Partial<RepoClient>): RepoClient {
  return overrides as RepoClient;
}

describe("useExternalTools", () => {
  it("forwards launches with the repo path and shows a failure", async () => {
    const openInEditor = vi.fn(async () => {
      throw new Error("No editor is configured, and opening a.exe with the system default app could run it. Set an editor command in Preferences.");
    });
    const { result } = renderHook(() => useExternalTools(client({ openInEditor }), "/r", [], async () => {}));
    act(() => result.current.openInEditor("a.exe", null));
    await waitFor(() => expect(result.current.error).toMatch(/^No editor is configured/));
    expect(openInEditor).toHaveBeenCalledWith("/r", "a.exe", null);
    act(() => result.current.dismissError());
    expect(result.current.error).toBeNull();
  });

  it("tracks a running merge tool, records its outcome and refreshes after it exits", async () => {
    let finish: (v: "Resolved") => void = () => {};
    const openMergeTool = vi.fn(() => new Promise<"Resolved">((resolve) => (finish = resolve)));
    const refresh = vi.fn(async () => {});
    const { result } = renderHook(() => useExternalTools(client({ openMergeTool }), "/r", ["a.txt"], refresh));
    act(() => result.current.openMergeTool("a.txt"));
    expect(result.current.mergeToolRunning.has("a.txt")).toBe(true);
    await act(async () => finish("Resolved"));
    expect(result.current.mergeToolRunning.has("a.txt")).toBe(false);
    expect(result.current.mergeToolResults.get("a.txt")).toBe("Resolved");
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("records nothing for a merge editor handed off to VSCode", async () => {
    const openMergeTool = vi.fn(async () => "HandedOff" as const);
    const { result } = renderHook(() => useExternalTools(client({ openMergeTool }), "/r", ["a.txt"], async () => {}));
    await act(async () => result.current.openMergeTool("a.txt"));
    expect(result.current.mergeToolResults.size).toBe(0);
  });

  it("drops a result once the path is no longer conflicted", async () => {
    const openMergeTool = vi.fn(async () => "Unresolved" as const);
    const { result, rerender } = renderHook(({ conflicted }) => useExternalTools(client({ openMergeTool }), "/r", conflicted, async () => {}), {
      initialProps: { conflicted: ["a.txt"] },
    });
    await act(async () => result.current.openMergeTool("a.txt"));
    expect(result.current.mergeToolResults.get("a.txt")).toBe("Unresolved");
    rerender({ conflicted: [] });
    expect(result.current.mergeToolResults.size).toBe(0);
  });
});
```

`useExternalToolSettings.test.ts`:

```ts
describe("useExternalToolSettings", () => {
  it("loads, saves and reports a failed save", async () => {
    const setExternalToolSettings = vi.fn().mockResolvedValueOnce(undefined).mockRejectedValueOnce(new Error("disk full"));
    const { result } = renderHook(() =>
      useExternalToolSettings({
        getExternalToolSettings: async () => ({ editor: "code", terminal: null }),
        setExternalToolSettings,
      } as unknown as RepoClient),
    );
    await waitFor(() => expect(result.current.settings).toEqual({ editor: "code", terminal: null }));
    await act(async () => expect(await result.current.save({ editor: null, terminal: "kitty" })).toBe(true));
    expect(result.current.settings).toEqual({ editor: null, terminal: "kitty" });
    await act(async () => expect(await result.current.save({ editor: "x", terminal: null })).toBe(false));
    expect(result.current.error).toBe("disk full");
    expect(result.current.settings).toEqual({ editor: null, terminal: "kitty" });
  });
});
```

`useAppState.test.ts`, a new test next to the `resolveAddDeleteConflict` one, using that test's client builder:
- `markResolved("a.txt", false)` calls `client.markResolved("/repo", "a.txt", false)`.
- It returns `"MarkersRemain"` when the client does.
- When the client throws, it returns `null` with `state.error` set.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/state`
Expected: FAIL.

- [ ] **Step 3: Implement**

`useExternalTools.ts`:

```ts
import { useCallback, useEffect, useState } from "react";
import type { DiffSource, RepoClient } from "../ipc/RepoClient";

const message = (err: unknown) => (err instanceof Error ? err.message : String(err));

/**
 * Launches the user's own tools. Never goes through `runMutation`: opening a tool changes
 * nothing in the repo, so it must not set `pending` or block other actions. A merge tool exit
 * refreshes, because the tool rewrote the work tree file.
 */
export function useExternalTools(client: RepoClient, repoPath: string, conflictedPaths: readonly string[], refresh: () => Promise<void>): ExternalToolsApi {
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState<ReadonlySet<string>>(new Set());
  const [results, setResults] = useState<ReadonlyMap<string, "Resolved" | "Unresolved">>(new Map());

  const launch = useCallback((run: () => Promise<unknown>) => {
    setError(null);
    Promise.resolve().then(run).catch((err: unknown) => setError(message(err)));
  }, []);

  const openMergeTool = useCallback(
    (path: string) => {
      setError(null);
      setRunning((prev) => new Set(prev).add(path));
      setResults((prev) => {
        const next = new Map(prev);
        next.delete(path);
        return next;
      });
      void client
        .openMergeTool(repoPath, path)
        .then((outcome) => {
          if (outcome !== "HandedOff") setResults((prev) => new Map(prev).set(path, outcome));
        })
        .catch((err: unknown) => setError(message(err)))
        .finally(() => {
          setRunning((prev) => {
            const next = new Set(prev);
            next.delete(path);
            return next;
          });
          void refresh();
        });
    },
    [client, repoPath, refresh],
  );

  const conflictedKey = conflictedPaths.join("\0");
  useEffect(() => {
    const conflicted = new Set(conflictedKey === "" ? [] : conflictedKey.split("\0"));
    setResults((prev) => {
      const kept = [...prev].filter(([path]) => conflicted.has(path));
      return kept.length === prev.size ? prev : new Map(kept);
    });
  }, [conflictedKey]);

  return {
    openInEditor: (path, line) => launch(() => client.openInEditor(repoPath, path, line)),
    reveal: (path) => launch(() => client.revealInFileManager(repoPath, path)),
    openTerminal: () => launch(() => client.openTerminal(repoPath)),
    openDiffTool: (path, source) => launch(() => client.openDiffTool(repoPath, path, source)),
    openMergeTool,
    mergeToolRunning: running,
    mergeToolResults: results,
    dismissMergeToolResult: (path) =>
      setResults((prev) => {
        const next = new Map(prev);
        next.delete(path);
        return next;
      }),
    error,
    dismissError: () => setError(null),
  };
}
```

When the "Resolved" notice comes from a merge tool, the user then clicks Mark resolved. The merge-tool exit refresh only re-reads status.

`useExternalToolSettings.ts`:
- Load once on mount. On a load failure, set `settings` to `{ editor: null, terminal: null }` and `error`.
- `save` sets `settings` only on success and returns `true`/`false`.

`useMergeRebaseActions.ts`:

```ts
  const markResolved = useCallback(
    async (path: string, force: boolean): Promise<MarkResolvedOutcome | null> => {
      let outcome: MarkResolvedOutcome | null = null;
      await runMutation(async () => {
        outcome = await client.markResolved(repoPath, path, force);
      });
      return outcome;
    },
    [client, runMutation, repoPath],
  );
```

Add it to `MergeRebaseActions`, then thread it through `useAppState` (interface + return object), next to `resolveAddDeleteConflict`.

- [ ] **Step 4: Run to verify pass**

Run: `cd frontend && pnpm test -- --run src/state && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/state
git commit -m "feat(frontend): external tool actions, tool settings and mark resolved state"
```

---

### Task 13: Menus: file rows, commit sections, conflicted notice, repo tabs

**Files:**
- Modify:
  - `frontend/src/components/FileActionsMenu.tsx`
  - `frontend/src/components/FileActionsMenu.test.tsx`
  - `frontend/src/components/DiffPane.tsx`
  - `frontend/src/components/DiffPane.module.css`
  - `frontend/src/components/DiffPane.test.tsx`
  - `frontend/src/components/RepoTabs.tsx`
  - `frontend/src/components/RepoTabs.test.tsx`
  - the 4d compare-pane file section found in Pre-flight, if there is one

**Interfaces:**
- Consumes: `ExternalToolsApi` (Task 12); `markResolved` (Task 12).
- Produces (Task 14):
  - `DiffPane` props `externalTools?: ExternalToolsApi` and `onMarkResolved?: (path: string, force: boolean) => Promise<MarkResolvedOutcome | null>`
  - `RepoTabs` prop `tabMenuItems?: (path: string) => ContextMenuItem[]`

```ts
// FileActionsMenu.tsx
export type FileAction =
  | { kind: "discard" } | { kind: "ignore"; pattern: string }
  | { kind: "openInEditor" } | { kind: "reveal" } | { kind: "diffTool" } | { kind: "mergeTool" };
export function fileMenuItems(entry: StatusEntry, onPick: (action: FileAction) => void, tools?: { mergeToolRunning: boolean }): ContextMenuItem[];
export function commitFileMenuItems(onPick: (action: FileAction) => void): ContextMenuItem[];
```

- [ ] **Step 1: Write the failing menu-builder tests** (`FileActionsMenu.test.tsx`)

```ts
  const conflicted = { path: "c.txt", staged: false, kind: "Conflicted" as const };
  const deleted = { path: "gone.txt", staged: false, kind: "Deleted" as const };

  it("adds external tool items only when tools are available", () => {
    expect(fileMenuItems(tracked, vi.fn(), { mergeToolRunning: false }).map((i) => i.label)).toEqual([
      "Discard changes…", "Open in editor", "Reveal in file manager", "Open in diff tool",
    ]);
    expect(fileMenuItems({ ...tracked, staged: true }, vi.fn(), { mergeToolRunning: false }).map((i) => i.label)).toEqual([
      "Open in editor", "Reveal in file manager", "Open in diff tool",
    ]);
  });

  it("offers the merge tool for conflicted files and disables it while running", () => {
    const items = fileMenuItems(conflicted, vi.fn(), { mergeToolRunning: true });
    expect(items.map((i) => i.label)).toEqual(["Open in editor", "Reveal in file manager", "Open in merge tool"]);
    expect(items[2].disabled).toBe(true);
  });

  it("disables open in editor for a deleted file", () => {
    const open = fileMenuItems(deleted, vi.fn(), { mergeToolRunning: false }).find((i) => i.label === "Open in editor");
    expect(open?.disabled).toBe(true);
    expect(open?.title).toBe("The file was deleted");
  });

  it("builds the commit file menu and reports picks", () => {
    const onPick = vi.fn();
    const items = commitFileMenuItems(onPick);
    expect(items.map((i) => i.label)).toEqual(["Open in editor", "Reveal in file manager", "Open in diff tool"]);
    items[2].onSelect();
    expect(onPick).toHaveBeenCalledWith({ kind: "diffTool" });
  });
```

Run: `cd frontend && pnpm test -- --run src/components/FileActionsMenu`
Expected: FAIL.

- [ ] **Step 2: Implement the builders**

```ts
export function fileMenuItems(entry: StatusEntry, onPick: (action: FileAction) => void, tools?: { mergeToolRunning: boolean }): ContextMenuItem[] {
  const items: ContextMenuItem[] = [];
  const hasFileActions = !entry.staged && entry.kind !== "Conflicted";
  if (hasFileActions) {
    items.push({ label: "Discard changes…", destructive: true, onSelect: () => onPick({ kind: "discard" }) });
    if (isUntracked(entry)) {
      for (const { label, pattern } of ignorePatterns(entry.path)) {
        items.push({ label, onSelect: () => onPick({ kind: "ignore", pattern }) });
      }
    }
  }
  if (tools !== undefined) {
    const deleted = entry.kind === "Deleted";
    items.push(
      {
        label: "Open in editor",
        disabled: deleted,
        title: deleted ? "The file was deleted" : undefined,
        onSelect: () => onPick({ kind: "openInEditor" }),
      },
      { label: "Reveal in file manager", onSelect: () => onPick({ kind: "reveal" }) },
    );
    if (entry.kind === "Conflicted") {
      items.push({ label: "Open in merge tool", disabled: tools.mergeToolRunning, onSelect: () => onPick({ kind: "mergeTool" }) });
    } else {
      items.push({ label: "Open in diff tool", onSelect: () => onPick({ kind: "diffTool" }) });
    }
  }
  return items;
}

export function commitFileMenuItems(onPick: (action: FileAction) => void): ContextMenuItem[] {
  return [
    { label: "Open in editor", onSelect: () => onPick({ kind: "openInEditor" }) },
    { label: "Reveal in file manager", onSelect: () => onPick({ kind: "reveal" }) },
    { label: "Open in diff tool", onSelect: () => onPick({ kind: "diffTool" }) },
  ];
}
```

The existing tests ("offers only discard for tracked files" etc.) pass no `tools` argument and stay green.

- [ ] **Step 3: Write the failing DiffPane tests** (`DiffPane.test.tsx`, a new `describe("DiffPane external tools")`, using `renderUncommitted` / `renderCommit`)

Build `externalTools` with `vi.fn()` actions and empty `mergeToolRunning`/`mergeToolResults`, passed through the render helper's `overrides`. Tests:
- **`opens a modified file in the editor at its first hunk line and in the diff tool`**:
  - status `[{ path: "a.ts", staged: false, kind: "Modified" }]` and `getWorkingDiff` returning one hunk with `newStart: 14`.
  - Open the "Actions for a.ts" menu and click "Open in editor": `externalTools.openInEditor` is called with `("a.ts", 14)`.
  - Click "Open in diff tool": called with `("a.ts", { kind: "Unstaged" })`.
- **`a staged entry opens the staged diff`**: `openDiffTool("a.ts", { kind: "Staged" })`.
- **`a conflicted entry has a menu with the merge tool`**:
  - Status Conflicted: the "Actions for c.txt" button exists, and "Open in merge tool" calls `openMergeTool("c.txt")`.
- **`shows the merge tool result and marks the file resolved`**:
  - `mergeToolResults = new Map([["c.txt", "Resolved"]])` and `onMarkResolved = vi.fn(async () => "Resolved")`.
  - The `role="status"` text is "The merge tool finished.".
  - Clicking "Mark resolved" calls `onMarkResolved("c.txt", false)`.
  - After resolve, `externalTools.dismissMergeToolResult` is called with `"c.txt"`.
- **`asks before marking a file with conflict markers resolved`**:
  - `onMarkResolved` resolves `"MarkersRemain"` first and then `"Resolved"`.
  - Clicking Mark resolved opens a dialog with the text "Conflict markers remain in c.txt. Mark it resolved anyway?".
  - Its "Mark resolved" button calls `onMarkResolved("c.txt", true)`.
- **`an unresolved merge tool result only offers Dismiss`**: the text "The merge tool closed without resolving the conflict.", and Dismiss calls `dismissMergeToolResult("c.txt")`.
- **`commit file sections have an actions menu with a commit diff source`**:
  - `renderCommit` with `getCommitFiles → ["b.ts"]`.
  - Open "Actions for b.ts" and click "Open in diff tool": `openDiffTool("b.ts", { kind: "Commit", commitId: "<id>", oldPath: null })`.
  - "Open in editor" passes `("b.ts", null)`.
- **`without externalTools the menus stay as before`**: an existing staged entry has no "Actions for" button (the current behavior).

Run: `cd frontend && pnpm test -- --run src/components/DiffPane`
Expected: FAIL.

- [ ] **Step 4: Implement the DiffPane changes**

- Add `externalTools?: ExternalToolsApi` and `onMarkResolved?` to the `DiffPane` props. Pass them down through `UncommittedDiffPane` → `UncommittedFileSection`, and through `CommitDiffPane` → `CommitFileSection` (`externalTools` only), the same way `onOpenFileHistory` is threaded today.
- `UncommittedFileSection`:
  - `const tools = externalTools === undefined ? undefined : { mergeToolRunning: externalTools.mergeToolRunning.has(entry.path) };`
  - `const menuItems = fileMenuItems(entry, onPickFileAction, tools);`
  - Replace `hasFileActions` as the gate for the right-click handler and the "…" button with `menuItems.length > 0`. Keep the `hasFileActions` name for what it means today, and use it inside `onPickFileAction`.
  - `onPickFileAction`:

```ts
  const onPickFileAction = (action: FileAction) => {
    switch (action.kind) {
      case "discard": return onRequestDiscardFile(entry.path);
      case "ignore": return onIgnorePath(action.pattern);
      case "openInEditor": return externalTools?.openInEditor(entry.path, hunks?.[0]?.newStart ?? null);
      case "reveal": return externalTools?.reveal(entry.path);
      case "diffTool": return externalTools?.openDiffTool(entry.path, { kind: entry.staged ? "Staged" : "Unstaged" });
      case "mergeTool": return externalTools?.openMergeTool(entry.path);
    }
  };
```

  - Notice, rendered for a conflicted entry, between the header and the body:

```tsx
      {isConflicted && externalTools?.mergeToolResults.has(entry.path) && (
        <div role="status" className={styles.mergeToolNotice}>
          {externalTools.mergeToolResults.get(entry.path) === "Resolved" ? (
            <>
              <span>The merge tool finished.</span>
              {onMarkResolved !== undefined && (
                <button type="button" onClick={() => void markResolved(false)}>Mark resolved</button>
              )}
            </>
          ) : (
            <span>The merge tool closed without resolving the conflict.</span>
          )}
          <button type="button" onClick={() => externalTools.dismissMergeToolResult(entry.path)}>Dismiss</button>
        </div>
      )}
      {confirmMarkers && (
        <ConfirmDialog … message={`Conflict markers remain in ${entry.path}. Mark it resolved anyway?`} confirmLabel="Mark resolved"
          onConfirm={() => { setConfirmMarkers(false); void markResolved(true); }} onCancel={() => setConfirmMarkers(false)} />
      )}
```

  - `markResolved(force)` calls `onMarkResolved(entry.path, force)`:
    - `"MarkersRemain"` → `setConfirmMarkers(true)`.
    - `"Resolved"` → `externalTools?.dismissMergeToolResult(entry.path)`.
    - `null` → nothing (the error is in the global banner).
  - Match `ConfirmDialog`'s real prop names. Check `primitives/ConfirmDialog.tsx` and use what `DiffPane`'s existing discard confirm uses.
- `CommitFileSection`:
  - Add `const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);`.
  - Add a header `onContextMenu` when `externalTools` is set.
  - Add a "…" button with `aria-label={`Actions for ${path}`}` and `aria-haspopup="menu"` in `rowActions` after History, matching the uncommitted section's button.
  - Picks: `openInEditor(path, null)`, `reveal(path)`, `openDiffTool(path, { kind: "Commit", commitId, oldPath: oldPath ?? null })`.
- `DiffPane.module.css`: `.mergeToolNotice { display: flex; gap: 8px; align-items: center; padding: 4px 8px; }`, using the same colour tokens as the existing conflicted styles (`grep -n "conflicted" DiffPane.module.css`).
- **4d compare pane** (only if Pre-flight found one): give its file section the same menu as `CommitFileSection`, with source `{ kind: "Range", fromId, toId, oldPath }`, and add a test that mirrors the commit one.

- [ ] **Step 5: Write the failing RepoTabs tests**

```tsx
  it("opens a tab menu from right-click and from the keyboard", async () => {
    const onPick = vi.fn();
    const tabMenuItems = vi.fn((path: string) => [{ label: "Open terminal here", onSelect: () => onPick(path) }]);
    render(<RepoTabs {...baseProps} tabMenuItems={tabMenuItems} />);
    fireEvent.contextMenu(screen.getByRole("tab", { name: "a" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Open terminal here" }));
    expect(onPick).toHaveBeenCalledWith("/repos/a");

    screen.getByRole("tab", { name: "b" }).focus();
    await userEvent.keyboard("{Shift>}{F10}{/Shift}");
    expect(screen.getByRole("menuitem", { name: "Open terminal here" })).toBeInTheDocument();
    expect(tabMenuItems).toHaveBeenLastCalledWith("/repos/b");
  });

  it("has no tab menu without tabMenuItems", () => {
    render(<RepoTabs {...baseProps} />);
    fireEvent.contextMenu(screen.getByRole("tab", { name: "a" }));
    expect(screen.queryByRole("menu")).toBeNull();
  });
```

Use the file's existing props helper and repo fixture names in place of `baseProps` and `/repos/a`. Check what role `ContextMenu` renders its items with (`menuitem` vs `button`) and use that.

Run: `cd frontend && pnpm test -- --run src/components/RepoTabs`
Expected: FAIL.

- [ ] **Step 6: Implement RepoTabs**

- Add `tabMenuItems?: (path: string) => ContextMenuItem[]` and `const [menu, setMenu] = useState<{ path: string; x: number; y: number } | null>(null)`.
- On the tab button:
  - `onContextMenu`: when `tabMenuItems` is set, `preventDefault` and `setMenu({ path, x: clientX, y: clientY })`.
  - In `handleTabKeyDown`, before the arrow-key handling: `ContextMenu`, or `F10` with Shift, opens the menu at the tab's `getBoundingClientRect()` bottom-left. Add `aria-haspopup="menu"` only when `tabMenuItems` is set.
- Render `<ContextMenu x y items={tabMenuItems(menu.path)} onClose={() => setMenu(null)} />` once, outside the tablist `div` (a tablist may only own tabs).

- [ ] **Step 7: Run all component tests and lint**

Run: `cd frontend && pnpm test -- --run src/components && pnpm lint`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/components
git commit -m "feat(frontend): external tool menus, merge tool notice and repo tab menu"
```

---

### Task 14: Preferences dialog, palette and App wiring

**Files:**
- Create:
  - `frontend/src/components/PreferencesDialog.tsx`
  - `frontend/src/components/PreferencesDialog.module.css`
  - `frontend/src/components/PreferencesDialog.test.tsx`
- Modify:
  - `frontend/src/lib/commands.ts`
  - `frontend/src/lib/commands.test.ts`
  - `frontend/src/App.tsx`
  - `frontend/src/App.test.tsx`
  - `frontend/src/vscode-main.tsx`

**Interfaces:**
- Consumes: Tasks 12, 13.
- Produces:

```tsx
export function PreferencesDialog(props: { onClose: () => void; children: ReactNode }): JSX.Element;
export function PreferencesSection(props: { title: string; children: ReactNode }): JSX.Element;  // 4g adds one of these
export function ExternalToolsSection(props: { client: RepoClient; repoPath: string | null; toolHost: "desktop" | "vscode"; onClose: () => void }): JSX.Element;
// commands.ts
export interface AppCommands { onCloneRepository(): void; onNewRepository(): void; onOpenPreferences?: () => void }
export interface ToolCommands {
  onOpenTerminal(): void;
  onRevealRepository(): void;
  onOpenCurrentFileInEditor(): void;
  onOpenCurrentFileInDiffTool(): void;
}
// buildCommands(..., searchCommands?, toolCommands?)
// App props: toolHost?: "desktop" | "vscode"
```

- [ ] **Step 1: Write the failing PreferencesDialog tests**

```tsx
function renderTools(client: Partial<RepoClient>, toolHost: "desktop" | "vscode" = "desktop") {
  const onClose = vi.fn();
  render(
    <PreferencesDialog onClose={onClose}>
      <ExternalToolsSection client={client as RepoClient} repoPath="/r" toolHost={toolHost} onClose={onClose} />
    </PreferencesDialog>,
  );
  return onClose;
}

describe("PreferencesDialog", () => {
  it("loads, edits and saves the editor and terminal commands, blank as null", async () => {
    const setExternalToolSettings = vi.fn(async () => {});
    renderTools({
      getExternalToolSettings: async () => ({ editor: "code", terminal: null }),
      setExternalToolSettings,
      getGitTools: async () => ({ diffTool: null, mergeTool: null }),
    });
    const editor = await screen.findByLabelText("Editor command");
    expect(editor).toHaveValue("code");
    expect(screen.getByLabelText("Terminal command")).toHaveAttribute("placeholder", "System default terminal");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    await userEvent.clear(editor);
    await userEvent.type(screen.getByLabelText("Terminal command"), "kitty --directory {{dir}");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(setExternalToolSettings).toHaveBeenCalledWith({ editor: null, terminal: "kitty --directory {dir}" });
  });

  it("shows git diff and merge tools read-only with their source and problems", async () => {
    renderTools({
      getExternalToolSettings: async () => ({ editor: null, terminal: null }),
      setExternalToolSettings: vi.fn(),
      getGitTools: async () => ({
        diffTool: { name: "meld", command: "meld $LOCAL $REMOTE", source: "Global", viaMergeTool: true, problem: null },
        mergeTool: { name: "vimdiff", command: null, source: "Local", viaMergeTool: false, problem: "vimdiff runs in a terminal, …" },
      }),
    });
    expect(await screen.findByText("meld $LOCAL $REMOTE")).toBeInTheDocument();
    expect(screen.getByText("from merge.tool in global git config")).toBeInTheDocument();
    expect(screen.getByText("vimdiff runs in a terminal, …")).toBeInTheDocument();
  });

  it("shows Not configured when git has no tools", async () => {
    renderTools({
      getExternalToolSettings: async () => ({ editor: null, terminal: null }),
      setExternalToolSettings: vi.fn(),
      getGitTools: async () => ({ diffTool: null, mergeTool: null }),
    });
    expect(await screen.findAllByText("Not configured")).toHaveLength(2);
  });

  it("only explains VSCode behaviour in VSCode", async () => {
    renderTools({ getExternalToolSettings: async () => ({ editor: null, terminal: null }) }, "vscode");
    expect(screen.getByText("In VSCode, files, diffs, merges and terminals open in VSCode itself.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Editor command")).toBeNull();
  });

  it("closes on Close", async () => {
    const onClose = renderTools({
      getExternalToolSettings: async () => ({ editor: null, terminal: null }),
      getGitTools: async () => ({ diffTool: null, mergeTool: null }),
    });
    await userEvent.click(await screen.findByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalled();
  });
});
```

Run: `cd frontend && pnpm test -- --run src/components/PreferencesDialog`
Expected: FAIL.

- [ ] **Step 2: Implement `PreferencesDialog.tsx`**

- `PreferencesDialog`: `<FormDialog ariaLabel="Preferences" onCancel={onClose}>`, then `<h2>Preferences</h2>`, then `{children}`. It holds no state of its own, so sections are independent. 4g adds `<PreferencesSection title="Commit message">` next to the external tools one.
- `PreferencesSection`: `<fieldset className={styles.section}><legend>{title}</legend>{children}</fieldset>`.
- `ExternalToolsSection`:
  - **VSCode:** renders a `PreferencesSection title="External tools"` with only the VSCode note paragraph and a Close button.
  - **Desktop:**
    - Uses `useExternalToolSettings(client)`. Keeps local `editor`/`terminal` strings, initialized once `settings` loads.
    - Two `Field`s (`primitives/Field.tsx`, check its props): labels "Editor command" / "Terminal command", placeholders "System default app" / "System default terminal".
    - Hint text "Placeholders: {file}, {line}, {dir}".
    - A `<dl>` with "Diff tool" / "Merge tool" rows from `client.getGitTools(repoPath)` (skipped when `repoPath === null`). Each row shows `name`, the `command` in `<code>`, a source line, or the `problem` text, or "Not configured".
    - Save is disabled until a value differs from `settings`. It calls `save({ editor: editor.trim() || null, terminal: terminal.trim() || null })`. Close calls `onClose`. The `error` shows as an `InlineError`.
  - Source line:

```ts
const SOURCE_LABEL: Record<GitConfigSource, string> = {
  System: "system git config", Xdg: "XDG git config", Global: "global git config",
  Local: "repository git config", Worktree: "worktree git config", Other: "git config",
};
const sourceText = (tool: GitToolInfo) =>
  tool.viaMergeTool ? `from merge.tool in ${SOURCE_LABEL[tool.source]}` : `from ${SOURCE_LABEL[tool.source]}`;
```

- [ ] **Step 3: Write the failing palette tests** (`commands.test.ts`)

```ts
  it("adds Preferences… with the app commands", () => {
    const onOpenPreferences = vi.fn();
    const commands = buildCommands(appState(), undefined, [], undefined, undefined, undefined, undefined, undefined, {
      onCloneRepository: vi.fn(), onNewRepository: vi.fn(), onOpenPreferences,
    });
    commands.find((c) => c.label === "Preferences…")!.run();
    expect(onOpenPreferences).toHaveBeenCalled();
  });

  it("adds tool commands, with current-file ones only when there is a current file", () => {
    const tools = { onOpenTerminal: vi.fn(), onRevealRepository: vi.fn(), onOpenCurrentFileInEditor: vi.fn(), onOpenCurrentFileInDiffTool: vi.fn() };
    const labels = (currentFile: string | null) =>
      buildCommands(appState(), undefined, [], undefined, undefined, undefined, undefined, diffCommands({ currentFile }), undefined, undefined, undefined, tools).map((c) => c.label);
    expect(labels(null)).toEqual(expect.arrayContaining(["Open terminal here", "Reveal repository"]));
    expect(labels(null)).not.toContain("Open current file in editor");
    expect(labels("a.ts")).toEqual(expect.arrayContaining(["Open current file in editor", "Open current file in diff tool"]));
  });

  it("keeps tool commands while an operation is running", () => {
    const tools = { onOpenTerminal: vi.fn(), onRevealRepository: vi.fn(), onOpenCurrentFileInEditor: vi.fn(), onOpenCurrentFileInDiffTool: vi.fn() };
    const busy = appState({ pending: true });
    const labels = buildCommands(busy, undefined, [], undefined, undefined, undefined, undefined, undefined, undefined, undefined, undefined, tools).map((c) => c.label);
    expect(labels).toContain("Open terminal here");
  });
```

Use the file's existing `appState(...)`/`diffCommands(...)` builders, or their real names. Match the positional parameter count to the signature confirmed in Pre-flight.

- [ ] **Step 4: Implement the palette**

`AppCommands.onOpenPreferences?`: when it is set, push `{ id: "preferences", label: "Preferences…", keywords: ["preferences", "settings", "options", "editor", "terminal", "tools"], run }` inside the existing `if (appCommands !== undefined)` block.

New trailing parameter `toolCommands?: ToolCommands`. Push it right after the `searchCommands` block (read-only, so it is not behind `repositoryOperationDisabled`):

```ts
  if (toolCommands !== undefined) {
    commands.push(
      { id: "open-terminal", label: "Open terminal here", keywords: ["terminal", "shell", "console", "cmd"], run: toolCommands.onOpenTerminal },
      { id: "reveal-repository", label: "Reveal repository", keywords: ["reveal", "finder", "explorer", "file manager", "folder"], run: toolCommands.onRevealRepository },
    );
    if (diffCommands?.currentFile != null) {
      commands.push(
        { id: "open-current-file-in-editor", label: "Open current file in editor", keywords: ["editor", "open", "edit", "file"], run: toolCommands.onOpenCurrentFileInEditor },
        { id: "open-current-file-in-diff-tool", label: "Open current file in diff tool", keywords: ["diff", "tool", "difftool", "compare"], run: toolCommands.onOpenCurrentFileInDiffTool },
      );
    }
  }
```

- [ ] **Step 5: Write the failing App tests** (`App.test.tsx`)

- **`the header gear opens Preferences`**:
  - `fakeClient` with one open repo.
  - Click the `Preferences` button: `dialog[aria-label="Preferences"]` is shown and `getExternalToolSettings` has been called.
  - Close hides the dialog.
- **`the palette opens Preferences…`**: open the palette (the existing helper or Ctrl/Cmd+K), type "Preferences" and press Enter. The dialog is shown.
- **`the repo tab menu opens a terminal in that repo`**: right-click the tab, click "Open terminal here", and `openTerminal` is called with the tab's path.
- **`a failed tab action shows its error`**:
  - `openTerminal` rejects with "No terminal found. Set a terminal command in Preferences."
  - The text appears in an alert or `InlineError`.

Run: `cd frontend && pnpm test -- --run src/App`
Expected: FAIL.

- [ ] **Step 6: Implement the App wiring**

- `App`/`AppContent` take `toolHost?: "desktop" | "vscode"` (default `"desktop"`) and pass it down.
- **`AppContent`:**
  - State `preferencesOpen` and `tabToolError: string | null`.
  - Header: a gear button (`Settings` icon from `lucide-react`, `aria-label="Preferences"`, `title="Preferences"`, class `styles.iconButton`) before the release-notes button.
  - `appCommands` gains `onOpenPreferences: () => setPreferencesOpen(true)`.
  - `RepoTabs tabMenuItems={(path) => [...]}`:
    - "Open terminal here" → `client.openTerminal(path).catch((e) => setTabToolError(message(e)))`.
    - "Reveal repository" → `client.revealInFileManager(path, null)`, with the same catch.
  - Render `tabToolError` as `<InlineError message={tabToolError} onDismiss={() => setTabToolError(null)} />` next to the `restoreError` one.
  - Render the dialog: `{preferencesOpen && <PreferencesDialog onClose={…}><ExternalToolsSection client={client} repoPath={openRepos.activePath} toolHost={toolHost} onClose={…} /></PreferencesDialog>}`.
- **`RepoWorkspace`:**
  - `const conflictedPaths = useMemo(() => appState.state.status.filter((e) => e.kind === "Conflicted").map((e) => e.path), [appState.state.status]);`
  - `const externalTools = useExternalTools(client, repoPath, conflictedPaths, appState.refresh);`
  - Render `externalTools.error` as an `InlineError` right below the existing `appState.state.error` one (around `App.tsx:363`).
  - `DiffPane`: `externalTools={externalTools}` and `onMarkResolved={appState.markResolved}`.
  - `buildCommands(..., { onSearchCommits: requestSearchFocus }, toolCommands)`:

```ts
{
  onOpenTerminal: externalTools.openTerminal,
  onRevealRepository: () => externalTools.reveal(null),
  onOpenCurrentFileInEditor: () => { if (currentFile !== null) externalTools.openInEditor(currentFile, null); },
  onOpenCurrentFileInDiffTool: () => { if (currentFile !== null) externalTools.openDiffTool(currentFile, { kind: "Unstaged" }); },
}
```

- `vscode-main.tsx`: `<App client={vscodeRepoClient} toolHost="vscode" />`.

- [ ] **Step 7: Run all frontend checks**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): Preferences dialog, tool palette commands and app wiring"
```

---

### Task 15: Docs and full verification

**Files:**
- Modify:
  - `CHANGELOG.md`: under `## [Unreleased]`, after the last existing subsection.
  - `docs/USER_GUIDE.md`: a new `## External tools` before `## Command palette`.
  - `docs/ARCHITECTURE.md`: the "Crate/package layout" native-method sentence, and a new `## External tools` after `## Commit search`.

- [ ] **Step 1: CHANGELOG**

```markdown
### External tools

- File menus (the "…" button or right-click on a changed file or a commit's file) open the file
  in your editor, reveal it in the file manager, or open its changes in your git diff tool.
  Conflicted files open in your git merge tool, and when it finishes Browsitory offers
  **Mark resolved** (it warns if conflict markers remain).
- Right-click a repository tab (or use the palette) for **Open terminal here** and
  **Reveal repository**.
- A new **Preferences** dialog (gear button or "Preferences…" in the palette) sets the editor
  and terminal commands, with `{file}`, `{line}` and `{dir}` placeholders. The diff and
  merge tools come from your git config (`diff.tool`, `merge.tool`, `difftool.<name>.cmd`, …)
  and are shown there read-only. meld, kdiff3, Beyond Compare, p4merge, VS Code and WinMerge
  work without a `cmd`.
- In the VSCode extension, these open in VSCode itself: editor, diff view, merge editor,
  integrated terminal and "Reveal in OS".
```

- [ ] **Step 2: USER_GUIDE**

Write a `## External tools` section with these subsections:
- **Opening files**:
  - Where the menu items are.
  - The editor setting with an example (`code --goto {file}:{line}`, `"C:\Program Files\Sublime Text\subl.exe" {file}:{line}`).
  - Without a setting, the system default app opens the file. Files that could run (programs, scripts, shortcuts) are refused, so set an editor to open them.
- **Terminal**:
  - The defaults per OS, as in the spec: Terminal.app; Windows Terminal or `cmd`; `$TERMINAL`, `x-terminal-emulator`, GNOME Terminal, Konsole, Xfce Terminal, Alacritty, kitty, xterm.
  - The terminal command setting with `{dir}`.
- **Diff and merge tools**:
  - Configure them in git: `git config --global diff.tool meld`. A custom tool uses `git config --global difftool.mytool.cmd 'mytool "$LOCAL" "$REMOTE"'`.
  - The preset list. `difftool.<name>.path` sets the program location.
  - Terminal tools such as vimdiff are not supported.
  - The tool command must wait until its window closes (for example `code --wait`), as with `git difftool`.
  - What LOCAL/REMOTE are per source (the spec's table, in words).
- **Resolving with a merge tool**:
  - After the tool closes, "The merge tool finished." with **Mark resolved**.
  - With `mergetool.<name>.trustExitCode`, the exit code decides. Otherwise any change to the file counts.
  - Mark resolved warns about leftover `<<<<<<<`/`>>>>>>>` lines.
  - No `.orig` backup is written.
- **In VSCode**: the mapping, in one short list.

Add "Preferences…", "Open terminal here" and "Reveal repository" to the `## Command palette` section if it lists command names.

- [ ] **Step 3: ARCHITECTURE**

In "Crate/package layout", replace "except five VSCode-native ones (`pickRepoFolder`, `getAppVersion`, `getLastSeenVersion`, `setLastSeenVersion`, `openExternalUrl`)" with "except the VSCode-native ones (`pickRepoFolder`, `getAppVersion`, `getLastSeenVersion`, `setLastSeenVersion`, `openExternalUrl`, `logFrontendError`, and the external-tool launchers `openInEditor`, `revealInFileManager`, `openTerminal`, `openDiffTool`, `openMergeTool`)". The old count was already wrong, because `logFrontendError` was missing. Make the same "five" → list fix in "VSCode host and sidecar lifecycle" ("The host handles the five VSCode-native methods above itself").

New section:

```markdown
## External tools

- Each host launches tools its own way. The desktop app spawns programs from Rust. The VSCode
  extension host answers the five launch methods with VSCode APIs (`vscode.open`,
  `vscode.diff`, `_open.mergeEditor`, `createTerminal`, `revealFileInOS`), and they never reach
  the sidecar.
- `git-core::process` splits editor/terminal command lines itself (POSIX or Windows rules) and
  never uses a shell for them. A git `difftool.<name>.cmd`/`mergetool.<name>.cmd` runs as
  `sh -c` with `LOCAL`/`REMOTE`/`MERGED`/`BASE` as environment variables, exactly as git runs
  it. On Windows, `sh` is Git for Windows' `sh.exe`. Without it, the cmd is split and expanded
  directly.
- The repo worker only prepares a tool launch: it resolves config and writes the
  `LOCAL`/`REMOTE`/`BASE` temp files under `<temp>/browsitory-tools/`. The Tauri command
  spawns the tool and, for the merge tool, waits on `spawn_blocking`. A tool window that stays
  open for hours therefore never blocks the repo's command queue.
- VSCode reads historical file contents through the `browsitory-file:` content provider, which
  calls the sidecar's internal `read_file_version`. No temp files are needed there.
- Work tree paths come from `WorkerHandle::workdir()` (captured at spawn), never from the raw
  repo path, which may be a subdirectory (`Repository::discover`).
```

- [ ] **Step 4: Full verification**

Run:
```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && pnpm build && cd ..
cd extension && pnpm test -- --run && pnpm run compile && cd ..
```
Expected: all PASS.

Manual smoke test on the desktop app (`cargo tauri dev` from `crates/tauri-app`):
1. Set `git config diff.tool meld` (or another installed tool) in a scratch repo.
2. Edit a file, then use "Open in diff tool" and "Open in editor" with no editor set.
3. Run "Open terminal here".
4. Create a conflict, use "Open in merge tool", save in the tool, then Mark resolved.

Note anything that does not work in the PR description. There is no E2E for this batch.

- [ ] **Step 5: Commit**

```bash
git add CHANGELOG.md docs/USER_GUIDE.md docs/ARCHITECTURE.md
git commit -m "docs: external tools in user guide, changelog and architecture"
```

---

## Self-review notes

- **Spec coverage:**
  - Editor, reveal and terminal: Tasks 2 and 8.
  - Diff and merge tool resolution: Task 3.
  - Temp sides, launch and wait: Task 4.
  - Mark resolved: Tasks 5, 7, 8, 9, 12 and 13.
  - `[tools]` settings: Tasks 6, 8, 9, 12 and 14.
  - `WorkerHandle::workdir`: Task 7.
  - VSCode mapping: Tasks 9 and 11.
  - `RepoClient`: Task 10.
  - Menus, notice and tab menu: Task 13.
  - Preferences, palette and gear: Task 14.
  - Docs: Task 15.
  - 4d `Range` source: Tasks 4, 8, 11 and 13 Step 4.
- **Type names used across tasks:**
  - Rust: `DiffSource`, `FileVersion`, `PreparedDiffTool`, `PreparedMergeTool`, `MergeToolOutcome`, `MarkResolvedOutcome`, `ExternalToolSettings`, `GitTools`/`GitToolInfo`, `ConfigSource`.
  - Frontend: `ExternalToolsApi`, `ExternalToolSettingsApi`, `ToolCommands`, `GitConfigSource`.
  - If Pre-flight renames `DiffSource` to `ToolDiffSource`, rename it in all of these tasks.
