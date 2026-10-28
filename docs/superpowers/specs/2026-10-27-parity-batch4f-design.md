# Feature parity, batch 4f: external tools

## Goal

Open the user's own tools from Browsitory: a file in an editor, a file or the repository in the
system file manager, a terminal in the repository, a file's changes in the git-configured diff
tool, and a conflicted file in the git-configured merge tool. After the merge tool closes,
Browsitory offers "Mark resolved". The desktop app launches programs itself. The VSCode
extension maps every action to VSCode's own editor, diff, merge editor, terminal and "Reveal in
OS". Refines section 4f of `docs/superpowers/specs/2026-10-27-parity-batch4-roadmap-design.md`
(issue #95).

## Current state

- The only external launch is `open_external_url` (`crates/tauri-app/src/commands/mod.rs`). It
  calls `tauri_plugin_opener::open_url`. `tauri-plugin-opener` 2.5.4 is already a dependency and
  also exports the free functions `open_path(path, None::<&str>)` and
  `reveal_item_in_dir(path)` (`reveal_item_in_dir` canonicalizes, so a missing path is an
  error). Calls from Rust need no capability entry. `opener:default` in
  `capabilities/default.json` only covers calls from JS.
- The VSCode extension host answers some `RepoClient` methods itself. `NATIVE_METHODS` in
  `extension/src/sidecarBridge.ts` lists them (`pick_repo_folder`, `open_external_url`,
  `get_app_version`, `get_last_seen_version`, `set_last_seen_version`, `log_frontend_error`).
  `handleNativeRequest` dispatches them against injected `SidecarBridgeDependencies`, so they can
  be unit tested without VSCode. The bridge can also call the sidecar for its own needs through
  the private `callSidecar`.
- Preferences: there is no preferences dialog. Global settings live in the `config` crate's
  TOML file (`config.toml`). Only `[diff]` (`DiffSettings`, edited from the diff pane toolbar
  and palette) is a settings table. `ConfigFile` keeps plain keys first, then tables, then
  arrays of tables (a TOML ordering rule, as the comment there says).
- Process spawning: `git-core::signing::run_program` (`pub(crate)`) pipes stdin, captures
  output, waits, and sets `CREATE_NO_WINDOW` on Windows. Nothing spawns a program that should
  outlive the call.
- `git-core` reads git config through `repo.config()` and a private
  `config_string(config, key)` helper in `signing.rs` (unset or empty counts as `None`).
- File rows: `UncommittedFileSection` (`frontend/src/components/DiffPane.tsx`) has a "…" menu and
  a right-click menu, built by `fileMenuItems` in `FileActionsMenu.tsx`. They exist only for
  unstaged, non-conflicted entries (`hasFileActions`). `CommitFileSection` has no menu.
  Conflicted entries render `ConflictResolutionPane`.
- `RepoTabs` has no context menu. The palette is `buildCommands` (`frontend/src/lib/commands.ts`),
  with optional command groups passed as trailing positional parameters.
- `DiffSource`, which the roadmap's plumbing names, does not exist anywhere. This batch defines
  it.
- `git_core::repo::open` uses `Repository::discover`, so a repo path opened from the UI may be a
  subdirectory of the work tree. Paths from `get_status` are relative to the work tree root.

## Decisions

- **Each host launches its own way.** Desktop: Rust spawns the programs. VSCode: the extension
  host calls VSCode APIs. The frontend only calls `RepoClient`. `vscodeRepoClient` sends the five
  launch methods to the host as native methods, so they never reach the sidecar.
- **The editor is a Browsitory preference, not git config.** `[tools] editor` in `config.toml`.
  `core.editor` is never used, because it is usually a terminal editor such as `vim`, and git
  uses it for commit messages. When no editor is set, the file opens with the OS default app
  (`tauri_plugin_opener::open_path`).
- **No editor set and the file could run: refuse.** The OS default "open" action runs some files
  instead of opening them:
  - Windows: `exe com bat cmd ps1 vbs vbe js jse wsf wsh msi msc scr lnk url hta cpl jar`.
  - macOS/Linux: any file with an exec bit, plus `app command tool desktop jar sh appimage
    run`. In that case the call fails with "No editor is configured, and opening FILE with the
  system default app could run it. Set an editor command in Preferences." This rule lives in
  the pure function `launch::may_execute` and has unit tests.
- **Command lines are split by Browsitory and never passed to a shell.** This applies to the
  editor and terminal preferences. `process::split_command_line(input, style)` uses POSIX rules
  on Unix (quotes, backslash escapes, an error on an unclosed quote) and Windows
  `CommandLineToArgvW` rules on Windows (backslashes are literal unless they come before a
  quote). Both styles are tested on every OS. Placeholders are replaced inside each word after
  splitting, so a path with spaces stays one argument:
  - `{file}`: absolute path. When the editor command has no `{file}`, the path is appended as
    the last argument.
  - `{line}`: the line number. When no line is known, it is `1`.
  - `{dir}`: absolute work-tree path.
  - An unknown `{name}` is left as it is.
- **Line numbers.** The file header passes the first hunk's `newStart` when its diff is loaded,
  and `null` otherwise. Commit file sections pass `null`.
- **Terminal default per OS.** This is the pure function `launch::default_terminal(os,
  $TERMINAL, has_program)`:
  - macOS: `open -a Terminal {dir}`.
  - Windows: `wt -d {dir}` when `wt` is on `PATH`, otherwise `cmd /K` in a new console.
  - Linux and other Unix: `$TERMINAL` (split as a command line), then the first of
    `x-terminal-emulator`, `gnome-terminal --working-directory={dir}`, `konsole --workdir {dir}`,
    `xfce4-terminal --working-directory={dir}`, `alacritty --working-directory {dir}`,
    `kitty --directory {dir}` and `xterm` found on `PATH`.
  - When none is found: "No terminal found. Set a terminal command in Preferences."

  The working directory is always the work tree root.
- **Reveal** uses `tauri_plugin_opener::reveal_item_in_dir`. For a missing path (a deleted file),
  it reveals the nearest existing parent inside the work tree (`launch::nearest_existing`).
  `path = null` reveals the work tree root.
- **Diff and merge tools come from git config, resolved like git does** (`tools::resolve_tool`):
  - The diff tool is `diff.tool`, else `merge.tool` (git's difftool fallback). The merge tool is
    `merge.tool`. When neither is set, the error says which key to set. Example: "No diff tool
    is configured. Set diff.tool in your git config, for example: git config --global diff.tool
    meld".
  - When `difftool.<name>.cmd` / `mergetool.<name>.cmd` is set, it runs exactly as git runs it:
    `sh -c <cmd>` in the work tree root, with `LOCAL`, `REMOTE`, `MERGED` and `BASE` set as
    environment variables. git sets these as shell variables. Browsitory does no text
    substitution, so quoting in the user's cmd works as it does in git.
  - On Windows, `sh` is the `sh.exe` that comes with Git for Windows. `process::find_git_sh`
    looks next to `git.exe` on `PATH`: `..\bin\sh.exe`, `..\usr\bin\sh.exe`. When it isn't found,
    Browsitory splits the cmd itself with POSIX rules, expands `$LOCAL`/`${LOCAL}` (and the
    other three) inside each word (`tools::expand_tool_vars`), and runs the program directly.
  - **Built-in presets** cover tool names without a `cmd`: `meld`, `kdiff3`, `bc`/`bc3`/`bc4`,
    `p4merge`, `vscode`, `winmerge`. Each is an argv template, expanded with
    `expand_tool_vars` and run without a shell on every OS. `difftool.<name>.path` /
    `mergetool.<name>.path` replaces the preset's program. git has a much longer built-in list.
    This short list covers the common GUI tools, and every other tool works with an explicit
    `cmd`.
  - Terminal tools (`vimdiff`, `vimdiff1`–`3`, `nvimdiff`, `nvimdiff1`–`3`, `emerge`) are
    rejected: "vimdiff runs in a terminal, which Browsitory can't host. Choose a graphical tool,
    or set difftool.vimdiff.cmd to run it in a terminal window." An unknown name without `cmd`
    is rejected too: "diff.tool is set to "foo", but difftool.foo.cmd is not set and Browsitory
    has no built-in command for it."
- **Which file is `LOCAL`, `REMOTE`, `BASE` and `MERGED`.** Contents come from `git2`, never
  from `git show`:

  | Source | `LOCAL` | `REMOTE` |
  |---|---|---|
  | `Unstaged` | index (stage 0) | the work tree file itself (not copied, so edits in the tool are real) |
  | `Staged` | `HEAD` | index |
  | `Commit { commitId, oldPath }` | first parent at `oldPath ?? path` (root commit: missing) | commit at `path` |
  | `Range { fromId, toId, oldPath }` | `fromId` at `oldPath ?? path` | `toId` at `path` |

  Merge: `LOCAL` = stage 2 (ours), `REMOTE` = stage 3 (theirs), `BASE` = stage 1 (missing for an
  add/add conflict), and `MERGED` = the work tree file. A missing side becomes an empty temp
  file, because tools handle that better than `/dev/null`. A conflict without both stage 2 and
  stage 3 (add/delete) is refused with the existing `NotATextConflict` text. Gitlink
  (submodule) entries are refused: "PATH is a submodule, not a file."
- **Temp files.** Each launch gets its own directory,
  `std::env::temp_dir()/browsitory-tools/<random>/`, holding `<stem>_LOCAL<.ext>`,
  `<stem>_REMOTE<.ext>` and `<stem>_BASE<.ext>`. The extension is kept so tools can pick syntax
  highlighting. The directory is removed when the tool exits. Directories left behind by a
  crash are swept: any `browsitory-tools/*` older than 24 hours goes when a tool is prepared
  (`tools::sweep_stale_tool_dirs`). No `.orig` backup is written; git's `mergetool.keepBackup`
  is not honoured.
- **Diff tool launch returns after a short grace period.** The process is spawned with stdin and
  stdout null and stderr drained into a 4 KiB tail buffer. If it exits non-zero within 1 s
  (`DIFF_TOOL_GRACE`), the call fails with "The diff tool failed (exit N): <stderr tail>". This
  catches `sh: meld: not found`. Otherwise the call returns `Ok`, and a background thread waits
  for the tool, keeps draining stderr so the tool never gets `SIGPIPE`, and removes the temp
  directory. A quick exit 0 within the grace period leaves the directory for the 24 h sweep. That
  is usually a launcher handing off to an already-running instance, which still needs the files.
  Tool commands must otherwise wait for their window to close (`code --wait`), as with `git
  difftool`.
- **The merge tool is waited on, off the repo worker.** The worker only prepares the launch:
  config, temp files and a hash of the `MERGED` contents (`PreparedMergeTool`). Tauri runs
  `tools::run_merge_tool` on `tauri::async_runtime::spawn_blocking`. The repo's in-order worker
  stays free while the tool is open, which can be hours.
- **Merge tool outcome:**
  - `trustExitCode` true (`mergetool.<name>.trustExitCode`, default false like git): exit 0 →
    `Resolved`, else `Unresolved`.
  - `trustExitCode` false: `MERGED` contents changed → `Resolved`, else `Unresolved`. git
    compares mtimes and then asks, but Browsitory has no prompt to ask with.
  - A failure to start (not found) is an error, not an outcome.
  - VSCode returns `HandedOff`, because Browsitory cannot see when VSCode's merge editor is done.
- **"Mark resolved" belongs to this batch.** 4f lands before 4g, and 4g's roadmap names the same
  function. `git_core::merge::mark_resolved(repo, path, force) -> Result<MarkResolvedOutcome,
  MergeError>`:
  - Without `force`, it returns `MarkersRemain` when the work tree file still has a line that
    starts with `<<<<<<<` or `>>>>>>>` followed by a space or the end of the line. `=======` is
    not checked on its own, because it is also a Markdown/RST underline.
  - Binary files (a NUL in the first 8000 bytes) skip the check.
  - Otherwise it stages the file and returns `Resolved`. Staging removes the index conflict
    entries.
  - Without a conflict for the path → `NoConflict`.

  An outcome instead of an error lets the UI ask "Mark resolved anyway?" and then call again
  with `force: true`. 4g only adds a "Mark resolved" button to the conflict pane that calls
  `appState.markResolved`.
- **No undo entries.** No action here moves a ref. `MarkResolved` follows `ResolveConflict` and
  `ResolveAddDeleteConflict`, which `undo::plan_for` does not record either.
- **The merge tool result is a persistent notice, not a toast.** Toasts close after 5 s, which is
  usually before the user is back from the tool window. The conflicted file's section shows "The
  merge tool finished." with **Mark resolved** and **Dismiss**, or "The merge tool closed without
  resolving the conflict." with **Dismiss**. The notice is dropped when the path is no longer
  conflicted. "Open in merge tool" is disabled for that path while its tool is open.
- **VSCode mapping.** The host handles all five launch methods. Repository file contents reach
  VSCode through a `TextDocumentContentProvider` registered for the scheme `browsitory-file`. It
  is fed by an internal sidecar method `read_file_version`, which is never on `RepoClient`. There
  are no temp files and no dependency on the built-in Git extension's private `git:` URI format.
  - editor → `vscode.open` with `selection` at the line.
  - reveal → `revealFileInOS`.
  - terminal → `window.createTerminal({ cwd, name: "Browsitory: <repo name>" }).show()`.
  - diff → `vscode.diff(left, right, title)`, with the work tree side as a real `file:` URI so
    it is editable.
  - merge → the `_open.mergeEditor` command (internal, but the one VSCode's own Git extension
    uses) with base/ours/theirs as `browsitory-file:` URIs and the work tree file as output. If
    that command throws, the file is opened with `vscode.open`, and VSCode's conflict
    decorations take over.

  The host gets the work tree root from a second internal sidecar method, `repo_workdir`,
  because the repo path may be a subdirectory.
- **Desktop also uses the work tree root, not the raw repo path.** `WorkerHandle` gets
  `workdir: Option<PathBuf>`, captured when the worker spawns (`repo.workdir()`), and
  `WorkerHandle::workdir() -> Result<PathBuf, String>`. It needs no channel round trip.
- **Paths from IPC are checked** by `launch::resolve_in_workdir(workdir, rel)`. A path that is
  empty, absolute, or has a `..`, root or prefix component is refused with "Invalid path." The
  host's `resolveRepoFile` applies the same rule.
- **Preferences dialog.** It opens from the palette ("Preferences…") and from a gear button in
  the header (aria-label "Preferences"). It is not tied to a repo, but it reads git tools for the
  active tab.
  - `PreferencesDialog` is a `FormDialog` shell that renders its `children` as sections. Each
    section is a `PreferencesSection` (a `<fieldset>` with a `<legend>`). 4g adds its "Commit
    message" section as another child, with no change to the shell.
  - 4f adds the "External tools" section. It has the fields "Editor command" and "Terminal
    command", with placeholders "System default app" and "System default terminal", Save
    (disabled until something changed) and Close.
  - Below the fields are read-only rows "Diff tool" and "Merge tool". Each shows the name, the
    command, and its source ("from global git config", "from repository git config", "from merge.tool in …",
    "built-in command"), or the resolution problem as text.
  - Empty fields save as `null`.
  - In VSCode (`toolHost="vscode"`), the section shows only "In VSCode, files, diffs, merges and
    terminals open in VSCode itself." Git tool rows are hidden, because VSCode does not use them.
- **The host kind is an `App` prop, not a `RepoClient` method.** `App` already takes
  host-specific props from `main.tsx` (`updateBanner`). `vscode-main.tsx` passes
  `toolHost="vscode"`, and the default is `"desktop"`.
- **The repo tab menu** opens with a right-click, the ContextMenu key or Shift+F10 on a tab. It
  has "Open terminal here" and "Reveal repository". Errors from it show in the app-level
  `InlineError` row next to `restoreError`.
- **Tool errors inside a workspace** show in an `InlineError` under the global error row
  (`useExternalTools().error`), not in `state.error`. Opening a tool does not set `pending` and
  does not refresh. The exceptions: merge tool exit and Mark resolved refresh, and Mark resolved
  goes through `runMutation`.
- **IPC uses `loggedInvoke`** for the launch commands, like `open_external_url`. Launch failures
  are rare and worth a log line.

## Backend

### `git-core/src/process.rs` (new; shared with 4e)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStyle { Posix, Windows }
impl CommandStyle { pub fn native() -> Self; }

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SplitError {
    #[error("The command is empty.")]
    Empty,
    #[error("The command has an unclosed quote.")]
    UnclosedQuote,
}

pub fn split_command_line(input: &str, style: CommandStyle) -> Result<Vec<String>, SplitError>;

/// `name` with a path separator is returned as is when it exists. Otherwise each `PATH` dir is
/// searched, trying each `PATHEXT` extension on Windows (`.COM;.EXE;.BAT;.CMD` when unset).
pub fn find_program(name: &str) -> Option<PathBuf>;
pub fn find_program_in(name: &str, dirs: &[PathBuf], exts: &[String]) -> Option<PathBuf>;

/// Git for Windows' `sh.exe` (`None` elsewhere, and when git isn't on PATH).
pub fn find_git_sh() -> Option<PathBuf>;
pub fn git_sh_candidates(git_exe: &Path) -> Vec<PathBuf>;

pub struct SpawnSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, OsString)>,
    pub new_console: bool,   // Windows `CREATE_NEW_CONSOLE` (terminal `cmd /K`)
    pub no_window: bool,     // Windows `CREATE_NO_WINDOW` (`sh.exe` running a tool)
    pub capture_stderr: bool,
}

/// stdin/stdout null; stderr null or piped. Unix: `process_group(0)`, so the child outlives
/// a Ctrl+C in `cargo tauri dev`. Windows: `CREATE_NEW_PROCESS_GROUP` plus the flags above.
pub fn spawn(spec: &SpawnSpec) -> Result<Child, LaunchError>;

/// Detached launch for editors and terminals: spawn, and reap on a background thread.
pub fn spawn_detached(spec: &SpawnSpec) -> Result<(), LaunchError>;

/// Keeps reading `stderr` until EOF, keeping only the last `cap` bytes.
pub fn drain_tail(stderr: ChildStderr, cap: usize) -> Arc<Mutex<Vec<u8>>>;
pub fn tail_text(bytes: &[u8]) -> String; // last 3 non-empty lines, lossy UTF-8

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("Couldn't find the program '{0}'.")]
    NotFound(String),
    #[error(transparent)]
    Split(#[from] SplitError),
    #[error("Couldn't start '{program}': {source}")]
    Io { program: String, source: std::io::Error },
}
```

### `git-core/src/launch.rs` (new): editor, terminal, reveal

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os { Linux, MacOs, Windows }
impl Os { pub fn current() -> Self; }

pub fn resolve_in_workdir(workdir: &Path, rel: &str) -> Result<PathBuf, OpenError>;
pub fn nearest_existing(workdir: &Path, target: &Path) -> PathBuf;
pub fn expand_placeholders(word: &str, file: Option<&Path>, line: Option<u32>, dir: &Path) -> String;
pub fn may_execute(path: &Path, os: Os, has_exec_bit: bool) -> bool;

pub struct Invocation { pub argv: Vec<String>, pub new_console: bool }
pub fn editor_invocation(command: &str, style: CommandStyle, file: &Path, line: Option<u32>, dir: &Path)
    -> Result<Invocation, SplitError>;
pub fn default_terminal(os: Os, env_terminal: Option<&str>, has_program: &dyn Fn(&str) -> bool)
    -> Option<Invocation>; // words still contain `{dir}`
pub fn terminal_invocation(configured: Option<&str>, os: Os, style: CommandStyle, env_terminal: Option<&str>,
    has_program: &dyn Fn(&str) -> bool, dir: &Path) -> Result<Invocation, OpenError>;

/// The editor call: `Ok(EditorTarget::Command(invocation))`, or `Ok(EditorTarget::SystemDefault(path))`
/// when no editor is configured and the file can't run.
pub enum EditorTarget { Command(Invocation), SystemDefault(PathBuf) }
pub fn editor_target(configured: Option<&str>, os: Os, style: CommandStyle, file: &Path, line: Option<u32>, dir: &Path,
    has_exec_bit: bool) -> Result<EditorTarget, OpenError>;

#[derive(Debug, thiserror::Error)]
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
```

### `git-core/src/tools.rs` (new): git diff and merge tools

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind { Diff, Merge }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource { System, Xdg, Global, Local, Worktree, Other }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCommand {
    Shell(String),                                   // `<kind>tool.<name>.cmd`
    Preset { program: String, args: Vec<String> },   // built-in template, `.path` applied
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTool {
    pub name: String,
    pub source: ConfigSource,     // level of the `diff.tool`/`merge.tool` entry used
    pub via_merge_tool: bool,     // diff tool fell back to `merge.tool`
    pub command: ToolCommand,
    pub trust_exit_code: bool,    // merge only; false for diff
}

pub fn resolve_tool(config: &git2::Config, kind: ToolKind) -> Result<ResolvedTool, ToolError>;
pub fn preset(kind: ToolKind, name: &str) -> Option<(&'static str, &'static [&'static str])>;
pub fn expand_tool_vars(word: &str, vars: &ToolVars) -> String;  // $NAME and ${NAME}, 4 names only

pub struct ToolVars { pub local: PathBuf, pub remote: PathBuf, pub merged: PathBuf, pub base: PathBuf }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitToolInfo {
    pub name: String,
    pub command: Option<String>,  // the cmd, or the preset joined with spaces
    pub source: ConfigSource,
    pub via_merge_tool: bool,
    pub problem: Option<String>,  // ToolError text when it can't be used
}
pub struct GitTools { pub diff_tool: Option<GitToolInfo>, pub merge_tool: Option<GitToolInfo> }
pub fn git_tools(repo: &Repository) -> Result<GitTools, ToolError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSource {
    Unstaged,
    Staged,
    Commit { commit_id: String, old_path: Option<String> },
    Range { from_id: String, to_id: String, old_path: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileVersion { Head, Index, Stage(u8), Commit(String), FirstParent(String) }

pub const MAX_FILE_VERSION_BYTES: usize = 20 * 1024 * 1024;  // only the VSCode read path passes it
/// `Ok(None)` when the path doesn't exist in that version; `TooLarge` above `limit`.
pub fn file_version_contents(repo: &Repository, path: &str, version: &FileVersion, limit: usize)
    -> Result<Option<Vec<u8>>, ToolError>;

pub struct ToolInvocation { pub program: PathBuf, pub args: Vec<String>, pub env: Vec<(String, OsString)>,
    pub cwd: PathBuf, pub no_window: bool }

pub struct PreparedDiffTool { pub invocation: ToolInvocation, pub temp_dir: PathBuf }
pub struct PreparedMergeTool {
    pub invocation: ToolInvocation, pub temp_dir: PathBuf, pub merged: PathBuf,
    pub merged_before: Vec<u8>, pub trust_exit_code: bool,
}

pub fn prepare_diff_tool(repo: &Repository, path: &str, source: &DiffSource) -> Result<PreparedDiffTool, ToolError>;
pub fn prepare_merge_tool(repo: &Repository, path: &str) -> Result<PreparedMergeTool, ToolError>;
// `prepare_*_in(…, root: &Path)` variants take the temp root, for tests
pub fn tool_temp_root() -> PathBuf;  // temp_dir()/browsitory-tools
pub fn sweep_stale_tool_dirs(root: &Path, max_age: Duration);

pub const DIFF_TOOL_GRACE: Duration = Duration::from_secs(1);
pub fn launch_diff_tool(prepared: PreparedDiffTool, grace: Duration) -> Result<(), ToolError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeToolOutcome { Resolved, Unresolved }
pub fn run_merge_tool(prepared: PreparedMergeTool) -> Result<MergeToolOutcome, ToolError>;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("No diff tool is configured. Set diff.tool in your git config, for example: git config --global diff.tool meld")]
    NoDiffTool,
    #[error("No merge tool is configured. Set merge.tool in your git config, for example: git config --global merge.tool meld")]
    NoMergeTool,
    #[error("{name} runs in a terminal, which Browsitory can't host. Choose a graphical tool, or set {key} to run it in a terminal window.")]
    TerminalTool { name: String, key: String },
    #[error("{setting} is set to \"{name}\", but {key} is not set and Browsitory has no built-in command for it.")]
    UnknownTool { setting: String, name: String, key: String },
    #[error("Invalid path.")]
    InvalidPath,
    #[error("repository has no working directory (bare repository)")]
    NoWorkdir,
    #[error("no conflict found for path '{0}'")]
    NoConflict(String),
    #[error("'{0}' is an add/delete conflict, not a text conflict")]
    NotATextConflict(String),
    #[error("{0} is a submodule, not a file.")]
    NotAFile(String),
    #[error("{0} is too large to open (over 20 MB).")]
    TooLarge(String),
    #[error("The {tool} failed (exit {code}): {output}")]
    ToolFailed { tool: &'static str, code: i32, output: String },
    #[error(transparent)]
    Launch(#[from] LaunchError),
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
```

Built-in presets (`$X` expanded by `expand_tool_vars`; the program is replaced by `.path`):

| Name | Diff argv | Merge argv |
|---|---|---|
| `meld` | `meld $LOCAL $REMOTE` | `meld --output=$MERGED $LOCAL $BASE $REMOTE` |
| `kdiff3` | `kdiff3 $LOCAL $REMOTE` | `kdiff3 --auto -o $MERGED $BASE $LOCAL $REMOTE` |
| `bc`, `bc3`, `bc4` | `bcompare $LOCAL $REMOTE` | `bcompare $LOCAL $REMOTE $BASE -mergeoutput=$MERGED` |
| `p4merge` | `p4merge $LOCAL $REMOTE` | `p4merge $BASE $LOCAL $REMOTE $MERGED` |
| `vscode` | `code --wait --diff $LOCAL $REMOTE` | `code --wait --merge $REMOTE $LOCAL $BASE $MERGED` |
| `winmerge` | `WinMergeU -u -e $LOCAL $REMOTE` | `WinMergeU -u -e -wl -wr -fm $LOCAL $BASE $REMOTE -o $MERGED` |

### `git-core/src/merge.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkResolvedOutcome { Resolved, MarkersRemain }
pub fn mark_resolved(repo: &Repository, path: &str, force: bool) -> Result<MarkResolvedOutcome, MergeError>;
pub(crate) fn has_conflict_markers(content: &[u8]) -> bool;
```

### `config`

```rust
/// `[tools]` table. Empty strings are stored as `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExternalToolSettings { pub editor: Option<String>, pub terminal: Option<String> }
pub fn get_external_tool_settings() -> Result<ExternalToolSettings, ConfigError>;
pub fn set_external_tool_settings(settings: &ExternalToolSettings) -> Result<(), ConfigError>;
// plus the `_at(config_file, …)` variants for tests
```

`ConfigFile.tools` sits right after `diff` (tables before arrays of tables). 4g adds its
`[commit]` table after `tools`.

### `repo-service`

- `WorkerHandle.workdir: Option<PathBuf>` and `WorkerHandle::workdir()`.
- `worker/tools.rs` (new):
  - `Command::PrepareDiffTool { path, source, reply }` → `PreparedDiffTool`.
  - `Command::PrepareMergeTool { path, reply }` → `PreparedMergeTool`.
  - `Command::GetGitTools { reply }` → `GitTools`.
  - `Command::ReadFileVersion { path, version, reply }` → `Option<Vec<u8>>`.
- `worker/merge.rs`: `Command::MarkResolved { path, force, reply }` → `MarkResolvedOutcome`.
- None of these is added to `undo::plan_for` or `hold_reply`.

### Tauri (`commands/tools.rs`, new; `mark_resolved` in `commands/merge.rs`)

| Command | Args | Returns |
|---|---|---|
| `open_in_editor` | `repoPath, path, line: u32 \| null` | `null` |
| `reveal_in_file_manager` | `repoPath, path: string \| null` | `null` |
| `open_terminal` | `repoPath` | `null` |
| `open_diff_tool` | `repoPath, path, source: DiffSourceDto` | `null` |
| `open_merge_tool` | `repoPath, path` | `"Resolved" \| "Unresolved"` |
| `mark_resolved` | `repoPath, path, force` | `"Resolved" \| "MarkersRemain"` |
| `get_external_tool_settings` | none | `{ editor, terminal }` |
| `set_external_tool_settings` | `settings` | `null` |
| `get_git_tools` | `repoPath` | `{ diffTool, mergeTool }` |

`DiffSourceDto` is `#[serde(tag = "kind")]` with variants `Unstaged`, `Staged`,
`Commit { commitId, oldPath }` and `Range { fromId, toId, oldPath }`. `GitToolDto` is
`{ name, command, source: "System" | "Xdg" | "Global" | "Local" | "Worktree" | "Other",
viaMergeTool, problem }`.

### Sidecar

- `mark_resolved`, `get_external_tool_settings`, `set_external_tool_settings` and
  `get_git_tools`, with the same wire format as Tauri.
- Internal, called only by the extension host:
  - `read_file_version { repoPath, path, version }` → `{ text: string | null }` (lossy UTF-8).
    `version` is `{ kind: "Head" | "Index" }`, `{ kind: "Stage", stage: 1|2|3 }`, or
    `{ kind: "Commit" | "FirstParent", commitId }`.
  - `repo_workdir { repoPath }` → `string`.

## `RepoClient`

```ts
export type DiffSource =
  | { kind: "Unstaged" }
  | { kind: "Staged" }
  | { kind: "Commit"; commitId: string; oldPath: string | null }
  | { kind: "Range"; fromId: string; toId: string; oldPath: string | null };
export type MergeToolOutcome = "Resolved" | "Unresolved" | "HandedOff";
export type MarkResolvedOutcome = "Resolved" | "MarkersRemain";
export interface ExternalToolSettings { editor: string | null; terminal: string | null }
export type GitConfigSource = "System" | "Xdg" | "Global" | "Local" | "Worktree" | "Other";
export interface GitToolInfo {
  name: string;
  command: string | null;
  source: GitConfigSource;
  viaMergeTool: boolean;
  problem: string | null;
}
export interface GitTools { diffTool: GitToolInfo | null; mergeTool: GitToolInfo | null }

openInEditor(repoPath: string, path: string, line: number | null): Promise<void>;
revealInFileManager(repoPath: string, path: string | null): Promise<void>;
openTerminal(repoPath: string): Promise<void>;
openDiffTool(repoPath: string, path: string, source: DiffSource): Promise<void>;
openMergeTool(repoPath: string, path: string): Promise<MergeToolOutcome>;
markResolved(repoPath: string, path: string, force: boolean): Promise<MarkResolvedOutcome>;
getExternalToolSettings(): Promise<ExternalToolSettings>;
setExternalToolSettings(settings: ExternalToolSettings): Promise<void>;
getGitTools(repoPath: string): Promise<GitTools>;
```

There are two deviations from the roadmap:
- `openMergeTool` returns an outcome instead of `void`, because the UI needs it to offer Mark
  resolved.
- `DiffSource` gains `Range` for 4d's compare view, and `Commit` carries `oldPath` for renames.

## Extension host

- `NATIVE_METHODS` gains `open_in_editor`, `reveal_in_file_manager`, `open_terminal`,
  `open_diff_tool` and `open_merge_tool`.
- `SidecarBridgeDependencies` gains:
  - `openFile(path, line)`
  - `revealInOS(path)`
  - `createTerminal(cwd, name)`
  - `openDiff(left, right, title)`, with URI strings
  - `openMergeEditor({ base, ours, theirs, output })`, which resolves `false` when it fell back
    to a plain open
- `SidecarBridge.readFileVersion(params)` is public and used by the content provider.
- `extension/src/toolUris.ts` (new, pure):
  - `resolveRepoFile(workdir, path)`
  - `fileVersionUri(repoPath, path, version)` / `parseFileVersionUri(uri)`
  - `diffSides(repoPath, workdir, path, source)` → `{ left, right, title }`
  - `mergeSides(repoPath, workdir, path)`
- `extension.ts` registers the `browsitory-file` content provider on activation. The provider
  routes to `currentBridge`. Without an open panel it fails with "Browsitory is not open."

Diff titles: `"<name> (Index ↔ Working Tree)"`, `"<name> (HEAD ↔ Index)"`,
`"<name> (<short> ↔ parent)"` and `"<name> (<short from> ↔ <short to>)"`.

## Frontend

- `state/useExternalTools.ts` (new): `{ openInEditor, reveal, openTerminal, openDiffTool,
  openMergeTool, mergeToolRunning: ReadonlySet<string>, mergeToolResults: ReadonlyMap<string,
  "Resolved" | "Unresolved">, dismissMergeToolResult, error, dismissError }`.
  - `openMergeTool` records the outcome. `HandedOff` records nothing.
  - After any merge tool exit (outcome or error), it calls `refresh`.
  - Results for paths that are no longer conflicted are dropped.
- `useMergeRebaseActions.markResolved(path, force): Promise<MarkResolvedOutcome | null>` runs
  through `runMutation`. `null` means it failed, and the error is in `state.error`.
- `state/useExternalToolSettings.ts` (new): load on mount, `save(next)`, `error`.
- `FileActionsMenu.tsx`:
  - `FileAction` gains `openInEditor | reveal | diffTool | mergeTool`.
  - `fileMenuItems(entry, onPick, tools?: { mergeToolRunning: boolean })`: the tool items are
    added only when `tools` is given, so existing callers and tests are unchanged.
  - `commitFileMenuItems(onPick)` covers commit file sections.
- Menu rules for an uncommitted entry:
  - Discard and ignore items as today.
  - "Open in editor": disabled for `Deleted`, with the title "The file was deleted".
  - "Reveal in file manager".
  - "Open in diff tool": not for `Conflicted`. The source is `Staged` or `Unstaged` from
    `entry.staged`.
  - "Open in merge tool": `Conflicted` only, disabled while running.

  The "…" button and right-click menu now exist for every entry. Commit sections get a "…"
  button (aria-label `Actions for <path>`) and a right-click menu with "Open in editor",
  "Reveal in file manager" and "Open in diff tool" (source `Commit`).
- Conflicted section notice (role `status`): "The merge tool finished." [Mark resolved]
  [Dismiss], or "The merge tool closed without resolving the conflict." [Dismiss].
  - Mark resolved → `MarkersRemain` → `ConfirmDialog`: "Conflict markers remain in PATH. Mark
    it resolved anyway?", confirm label "Mark resolved".
  - `force: true` on confirm.
- `RepoTabs` gains a `tabMenuItems?: (path: string) => ContextMenuItem[]` prop. The menu opens
  on right-click, the `ContextMenu` key or Shift+F10.
- `components/PreferencesDialog.tsx` (new), with `PreferencesSection` and
  `ExternalToolsSection`, as in Decisions.
- Palette:
  - `AppCommands.onOpenPreferences` → "Preferences…" (keywords preferences, settings, options,
    editor, terminal).
  - A new trailing `toolCommands?: ToolCommands` parameter (`ToolCommands` holds
    `onOpenTerminal`, `onRevealRepository`, `onOpenCurrentFileInEditor`,
    `onOpenCurrentFileInDiffTool`) adds:
    - "Open terminal here" (keywords terminal, shell, console, cmd)
    - "Reveal repository" (keywords reveal, finder, explorer, file manager, folder)
    - "Open current file in editor" and "Open current file in diff tool", only when
      `diffCommands.currentFile !== null`.

  All of them are read-only, so they stay available while an operation runs.
- `App`: a `toolHost?: "desktop" | "vscode"` prop, the gear button, `PreferencesDialog` state in
  `AppContent`, the tab menu, and wiring of `useExternalTools` into `DiffPane` through a single
  `externalTools` prop.

## Overlaps with other batches

- **4e (hooks), which lands first.** 4e adds a process runner (capture, timeout, kill) and
  Windows `sh` lookup "next to `git.exe` or on `PATH`".
  - 4f's `process.rs` holds the shared pieces: `find_program`, `find_git_sh`, the Windows
    creation flags, and `tail_text`, which is the same idea as `signing.rs`'s `stderr_tail`.
  - Pre-flight checks what 4e actually added. If 4e already has a module with these helpers,
    4f extends it instead of creating `process.rs`, and keeps 4e's names.
- **4g (conflicts/polish), which lands after.**
  - 4f owns `merge::mark_resolved` + `MarkResolvedOutcome`, the worker command, both IPC
    layers, `RepoClient.markResolved` and `appState.markResolved`. 4g's conflict pane "Mark
    resolved" button reuses them.
  - 4f owns `PreferencesDialog` / `PreferencesSection`. 4g's commit-ruler setting is a new
    `PreferencesSection` child plus a `[commit]` table after `[tools]`.
  - 4g's `blob_contents(repo, oid)` (20 MB cap, image preview) should share
    `tools::MAX_FILE_VERSION_BYTES`, or use `file_version_contents`.
- **4d (compare), which lands first.**
  - `DiffSource::Range` exists for 4d's compare view. Pre-flight finds 4d's compare-pane file
    section component, and Task 13 gives it the same commit-section menu with a `Range` source.
  - If 4d introduced its own type called `DiffSource`, rename 4f's to `ToolDiffSource` and
    note it in the design.
- **4a–4e: `buildCommands` trailing parameters.** Each batch appends one. Pre-flight
  re-reads the signature, and `toolCommands` goes last.

## Testing

- `git-core` inline (pure):
  - `split_command_line`: both styles. POSIX quotes, escapes and unclosed quotes. Windows
    backslash/quote rules, including `"C:\Program Files\x\y.exe" {file}:{line}`.
  - `find_program_in` with a temp dir (PATHEXT on a `Windows`-style ext list).
  - `git_sh_candidates`.
  - `expand_placeholders`: spaces in the path, a missing line becomes 1, and unknown
    placeholders are left as they are.
  - `editor_invocation` appends the file when there is no `{file}`.
  - `may_execute` per OS.
  - `default_terminal` for each OS and `$TERMINAL`, the fallback order, and none found.
  - `terminal_invocation`: configured beats the default, and `{dir}` is expanded.
  - `resolve_in_workdir` rejects `..`, absolute paths and empty paths.
  - `nearest_existing`.
  - `expand_tool_vars`: `$LOCAL`, `${REMOTE}` inside `--output=$MERGED`, `$HOME` unchanged.
  - `preset` table lookups, `has_conflict_markers`.
- `git-core` `tests/tools.rs` (real temp repos and real git config through `repo.config()`):
  - `resolve_tool`: a `cmd` beats a preset, diff falls back to `merge.tool`, `.path` overrides
    the preset program, terminal and unknown tools are rejected, nothing configured is
    rejected, trustExitCode is read, and the source is `Local`.
  - `git_tools` reports problems.
  - `file_version_contents` for every version (missing → `None`, root commit's first parent →
    `None`, too large).
  - `prepare_diff_tool` for each source, with the temp file contents and names checked and
    `REMOTE` the real work tree file for `Unstaged`.
  - `prepare_merge_tool`: stage contents, an empty BASE for add/add, add/delete refused, no
    conflict refused, `merged_before`.
  - `sweep_stale_tool_dirs` removes old dirs and keeps fresh ones.
  - `#[cfg(unix)]`: `run_merge_tool` with `mergetool.t.cmd = 'printf done > "$MERGED"'` →
    `Resolved` and the temp dir removed; `cmd = true` with no trust and no change →
    `Unresolved`; trust plus `exit 3` → `Unresolved`; `launch_diff_tool` with `cmd = 'exit
    7'` → `ToolFailed { code: 7 }`.
- `git-core` `tests/merge.rs`: `mark_resolved`
  - stages a clean file
  - reports `MarkersRemain` and leaves the conflict
  - `force` stages anyway
  - Markdown `=======` is not a marker
  - no conflict → `NoConflict`
- `config` `tests/external_tool_settings.rs`: defaults, round trip next to `[diff]`, empty
  strings saved as `None`.
- `repo-service`:
  - `workdir()` is the root when opened from a subdirectory.
  - Prepare diff tool round trips through the worker, with temp files written.
  - `mark_resolved` round trips.
  - `read_file_version` for the index.
- `tauri-app`: DTO serialization and deserialization for `DiffSourceDto` (all four variants),
  `GitToolsDto`, `ExternalToolSettingsDto`, `MergeToolOutcomeDto` and
  `MarkResolvedOutcomeDto`.
- `vscode-sidecar` round trips:
  - `get_git_tools` with a repo-local `diff.tool`
  - `mark_resolved` on a conflicted repo
  - settings round trip
  - `read_file_version` for HEAD and a missing path
  - `repo_workdir`
- `extension`:
  - `toolUris.test.ts`: URI round trip, `diffSides` per source, `resolveRepoFile` rejects `..`.
  - `sidecarBridge.test.ts`: each native method calls the right dependency with the right
    arguments, invalid params → -32602, merge editor fallback → `HandedOff`.
- Frontend (mocked `RepoClient`):
  - Both clients' wiring, plus `vscodeRepoClient` for the new methods.
  - `useExternalTools`: the error, merge tool running and result, refresh after exit, result
    dropped when no longer conflicted.
  - `useExternalToolSettings`.
  - `fileMenuItems` / `commitFileMenuItems`.
  - DiffPane menus, the notice, Mark resolved and its confirm flow.
  - The `RepoTabs` tab menu (mouse and keyboard).
  - `PreferencesDialog`: load, save, the empty-field-to-null rule, git tool rows, the VSCode
    note.
  - Palette entries.
  - App: the gear opens Preferences.
- No E2E: every flow spawns an external program.

## Docs

- `docs/USER_GUIDE.md`: a new "External tools" section before "Command palette". It covers
  editor and terminal settings with placeholders, the defaults per OS, diff/merge tool
  configuration with the preset list and a `cmd` example, Mark resolved, and VSCode behavior.
- `CHANGELOG.md` under Unreleased: an "External tools" subsection.
- `docs/ARCHITECTURE.md`:
  - Fix the "five VSCode-native ones" list in "Crate/package layout". It is already six today
    because `log_frontend_error` is missing, and it becomes eleven.
  - Add an "External tools" section covering host split, worker prepare vs off-worker wait,
    the `browsitory-file` scheme, and `sh -c` with env vars.
- No `docs/LICENSE_COMPLIANCE.md` change. No new dependency, because `tauri-plugin-opener`
  already provides `open_path` and `reveal_item_in_dir`.

## Out of scope

Bundled diff/merge tools, per-file-type tool mapping, git's full built-in tool list, `.orig`
backups (`mergetool.keepBackup`), running terminal-based tools (`vimdiff`) inside a terminal
window automatically, directory diffs (`difftool --dir-diff`), and a keyboard shortcut for
Preferences (Ctrl/Cmd+, belongs to VSCode's own settings).
