//! "Open in editor", "Open terminal here" and "Reveal in file manager" for the desktop app.
//!
//! Every rule is a pure function with the OS, environment and `PATH` lookup as parameters, so it
//! is unit tested on every OS; `prepare_*` bind them to the real environment. Command lines are
//! split by `process::split_command_line` and never handed to a shell; placeholders are replaced
//! inside each word after splitting, so a path with spaces or shell metacharacters stays one
//! literal argument. All paths given to a program are absolute (work tree root joined with a
//! checked relative path), so none can start with `-` and be read as an option.
//!
//! See `docs/superpowers/specs/2026-10-27-parity-batch4f-design.md`.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::process::{self, CommandStyle, LaunchError, SpawnSpec, SplitError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Windows,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
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

/// A program and its arguments; `argv[0]` is looked up on `PATH` by `run_invocation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub argv: Vec<String>,
    pub new_console: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditorTarget {
    Command(Invocation),
    /// No editor configured and the file cannot run: open it with the OS default app.
    SystemDefault(PathBuf),
}

/// Joins a repository-relative path from IPC onto the work tree root. Refused: empty, absolute,
/// or any `.`, `..`, root or prefix component, and on Windows a `:` (alternate data streams,
/// drive-relative paths). Also refused when a directory on the way is a symlink that leads out
/// of the work tree (`etc -> /etc`), so the result can never leave it.
pub fn resolve_in_workdir(workdir: &Path, rel: &str) -> Result<PathBuf, OpenError> {
    if !is_safe_relative(rel) {
        return Err(OpenError::InvalidPath);
    }
    let path = workdir.join(rel);
    if parent_stays_inside(workdir, &path) {
        Ok(path)
    } else {
        Err(OpenError::InvalidPath)
    }
}

/// Whether the directory holding `path` (its nearest existing ancestor, with symlinks resolved)
/// is inside `workdir`. A leaf symlink is not judged here: callers decide what following it
/// means for them.
pub(crate) fn parent_stays_inside(workdir: &Path, path: &Path) -> bool {
    let Ok(root) = workdir.canonicalize() else {
        // Nothing on disk to compare against (pure callers); the lexical checks still apply.
        return true;
    };
    let Some(parent) = path.parent() else {
        return true;
    };
    match parent.ancestors().find(|p| p.exists()) {
        Some(existing) => existing
            .canonicalize()
            .is_ok_and(|resolved| resolved.starts_with(&root)),
        None => true,
    }
}

/// Shared with `tools`: see `resolve_in_workdir`.
pub(crate) fn is_safe_relative(rel: &str) -> bool {
    let path = Path::new(rel);
    !rel.is_empty()
        && !rel.contains('\0')
        && !path.is_absolute()
        && !(cfg!(windows) && rel.contains(':'))
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        // `Path::components` silently drops interior `.` components ("a/./b"); refuse them too.
        && rel.split(['/', '\\']).all(|part| part != "." && part != "..")
}

/// `target` itself when it exists, else its nearest existing ancestor inside `workdir`, else
/// `workdir`.
pub fn nearest_existing(workdir: &Path, target: &Path) -> PathBuf {
    target
        .ancestors()
        .find(|p| p.starts_with(workdir) && p.exists())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| workdir.to_path_buf())
}

/// Replaces `{file}`, `{line}` (1 when unknown) and `{dir}` in one argv word, in a single pass:
/// text inserted for one placeholder is never scanned again. Unknown `{name}`s stay as they are.
pub fn expand_placeholders(
    word: &str,
    file: Option<&Path>,
    line: Option<u32>,
    dir: &Path,
) -> String {
    let mut out = String::with_capacity(word.len());
    let mut rest = word;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let replacement = if after.starts_with("{file}") {
            file.map(|f| (6, f.display().to_string()))
        } else if after.starts_with("{line}") {
            Some((6, line.unwrap_or(1).to_string()))
        } else if after.starts_with("{dir}") {
            Some((5, dir.display().to_string()))
        } else {
            None
        };
        match replacement {
            Some((len, text)) => {
                out.push_str(&text);
                rest = &after[len..];
            }
            None => {
                out.push('{');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Extensions some desktop runs (or installs, or follows) instead of opening as a document:
/// Windows programs, scripts, installers and shortcuts; macOS bundles, scripts and launchers;
/// Linux launchers and shell scripts. Lower case. Every OS checks the whole list, because Wine
/// on Linux and macOS hands `.msi`, `.lnk`, `.bat` and `.exe` files from `xdg-open`/`open` to
/// Windows programs, and a repository is often shared between platforms.
const RUNNABLE_EXTENSIONS: &[&str] = &[
    "exe",
    "com",
    "bat",
    "cmd",
    "ps1",
    "psm1",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "ws",
    "msi",
    "msp",
    "mst",
    "msc",
    "scr",
    "pif",
    "lnk",
    "url",
    "website",
    "hta",
    "cpl",
    "jar",
    "jnlp",
    "reg",
    "inf",
    "scf",
    "sct",
    "shb",
    "shs",
    "chm",
    "application",
    "appref-ms",
    "appx",
    "appxbundle",
    "msix",
    "msixbundle",
    "appinstaller",
    "gadget",
    "diagcab",
    "settingcontent-ms",
    "library-ms",
    "search-ms",
    "py",
    "pyw",
    "pyz",
    "rb",
    "pl",
    "app",
    "command",
    "tool",
    "terminal",
    "workflow",
    "action",
    "scpt",
    "scptd",
    "applescript",
    "pkg",
    "mpkg",
    "fileloc",
    "inetloc",
    "webloc",
    "desktop",
    "sh",
    "bash",
    "zsh",
    "csh",
    "tcsh",
    "ksh",
    "fish",
    "run",
    "bin",
    "appimage",
    "flatpakref",
    "msu",
    "rdp",
    "ahk",
    "pyc",
    "pyo",
    "pyzw",
    "theme",
    "themepack",
    "deskthemepack",
    "searchconnector-ms",
    "xll",
    "vb",
    "prefpane",
    "mobileconfig",
];

/// Old Office documents share the OLE container format (`D0 CF 11 E0`) with `.msi` installers;
/// with one of these extensions an OLE file is a document.
const OLE_DOCUMENTS: &[&str] = &[
    "doc", "dot", "xls", "xlt", "ppt", "pot", "pps", "msg", "vsd", "pub", "mpp",
];

const OLE_MAGIC: &[u8] = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1";

fn lower_extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

/// Whether the OS default "open" action could run `path` rather than open it: a runnable
/// extension (any platform's, see `RUNNABLE_EXTENSIONS`), or an execute bit on macOS/Linux.
/// Pure: the caller says whether the file has an execute bit.
pub fn may_execute(path: &Path, os: Os, has_exec_bit: bool) -> bool {
    RUNNABLE_EXTENSIONS.contains(&lower_extension(path).as_str())
        || (matches!(os, Os::Linux | Os::MacOs) && has_exec_bit)
}

/// A script (`#!`), a native executable (ELF, PE `MZ`, Mach-O, fat Mach-O), an OLE container
/// (`.msi` installers), or a freedesktop `[Desktop Entry]` launcher, judged by the first bytes.
/// A file can be both runnable and misnamed, and desktop handlers sniff contents.
pub fn looks_runnable(head: &[u8]) -> bool {
    const MAGICS: &[&[u8]] = &[
        b"#!",
        b"\x7fELF",
        b"MZ",
        b"\xfe\xed\xfa\xce",
        b"\xfe\xed\xfa\xcf",
        b"\xce\xfa\xed\xfe",
        b"\xcf\xfa\xed\xfe",
        b"\xca\xfe\xba\xbe",
        OLE_MAGIC,
    ];
    MAGICS.iter().any(|magic| head.starts_with(magic)) || is_desktop_entry(head)
}

/// The first line that is not blank or a `#` comment is `[Desktop Entry]` (after a UTF-8 BOM).
fn is_desktop_entry(head: &[u8]) -> bool {
    let head = head.strip_prefix(b"\xef\xbb\xbf").unwrap_or(head);
    head.split(|b| *b == b'\n')
        .map(|line| line.trim_ascii())
        .find(|line| !line.is_empty() && !line.starts_with(b"#"))
        .is_some_and(|line| line == b"[Desktop Entry]")
}

#[cfg(unix)]
pub fn has_exec_bit(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub fn has_exec_bit(_path: &Path) -> bool {
    false
}

pub fn editor_invocation(
    command: &str,
    style: CommandStyle,
    file: &Path,
    line: Option<u32>,
    dir: &Path,
) -> Result<Invocation, SplitError> {
    let words = process::split_command_line(command, style)?;
    let has_file = words.iter().any(|w| w.contains("{file}"));
    let mut argv: Vec<String> = words
        .iter()
        .map(|w| expand_placeholders(w, Some(file), line, dir))
        .collect();
    if !has_file {
        argv.push(file.display().to_string());
    }
    Ok(Invocation {
        argv,
        new_console: false,
    })
}

/// What "Open in editor" runs. `runnable` is the caller's verdict on the file itself (execute
/// bit, contents, symlink target): it only matters without a configured editor.
#[allow(clippy::too_many_arguments)]
pub fn editor_target(
    configured: Option<&str>,
    os: Os,
    style: CommandStyle,
    file: &Path,
    line: Option<u32>,
    dir: &Path,
    runnable: bool,
) -> Result<EditorTarget, OpenError> {
    match configured.map(str::trim).filter(|c| !c.is_empty()) {
        Some(command) => Ok(EditorTarget::Command(editor_invocation(
            command, style, file, line, dir,
        )?)),
        None if runnable || may_execute(file, os, false) => Err(OpenError::MayExecute(
            file.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| file.display().to_string()),
        )),
        None => Ok(EditorTarget::SystemDefault(file.to_path_buf())),
    }
}

fn inv(words: &[&str], new_console: bool) -> Invocation {
    Invocation {
        argv: words.iter().map(|w| w.to_string()).collect(),
        new_console,
    }
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

/// The terminal used when none is configured. Words may still contain `{dir}`; every terminal
/// also starts with the work tree root as its working directory.
pub fn default_terminal(
    os: Os,
    env_terminal: Option<&str>,
    has_program: &dyn Fn(&str) -> bool,
) -> Option<Invocation> {
    match os {
        Os::MacOs => Some(inv(&["open", "-a", "Terminal", "{dir}"], false)),
        // Never `-d {dir}`: `;` in a path would split wt's command line into subcommands. wt
        // starts in its working directory, which `run_invocation` sets to the work tree root.
        Os::Windows if has_program("wt") => Some(inv(&["wt", "-d", "."], false)),
        Os::Windows => Some(inv(&["cmd", "/K"], true)),
        Os::Linux => {
            if let Some(words) = env_terminal
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .and_then(|t| process::split_command_line(t, CommandStyle::Posix).ok())
            {
                return Some(Invocation {
                    argv: words,
                    new_console: false,
                });
            }
            LINUX_TERMINALS
                .iter()
                .find(|c| has_program(c[0]))
                .map(|c| inv(c, false))
        }
    }
}

pub fn terminal_invocation(
    configured: Option<&str>,
    os: Os,
    style: CommandStyle,
    env_terminal: Option<&str>,
    has_program: &dyn Fn(&str) -> bool,
    dir: &Path,
) -> Result<Invocation, OpenError> {
    let base = match configured.map(str::trim).filter(|c| !c.is_empty()) {
        Some(command) => Invocation {
            argv: process::split_command_line(command, style)?,
            new_console: false,
        },
        None => default_terminal(os, env_terminal, has_program).ok_or(OpenError::NoTerminal)?,
    };
    Ok(Invocation {
        argv: base
            .argv
            .iter()
            .map(|w| expand_placeholders(w, None, None, dir))
            .collect(),
        new_console: base.new_console,
    })
}

/// Whether opening `file` with the OS default app could run it: its name, its execute bit, its
/// first bytes, and for a symlink the same checks on the file it points to.
fn file_may_run(file: &Path, os: Os) -> bool {
    let check = |path: &Path| {
        may_execute(path, os, has_exec_bit(path))
            || read_head(path).is_some_and(|head| {
                looks_runnable(&head)
                    && !(head.starts_with(OLE_MAGIC)
                        && OLE_DOCUMENTS.contains(&lower_extension(path).as_str()))
            })
    };
    if check(file) {
        return true;
    }
    match std::fs::symlink_metadata(file) {
        Ok(meta) if meta.file_type().is_symlink() => match std::fs::canonicalize(file) {
            Ok(target) => check(&target),
            // A dangling or unreadable link: refuse rather than guess.
            Err(_) => true,
        },
        _ => false,
    }
}

fn read_head(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut head = Vec::with_capacity(1024);
    std::fs::File::open(path)
        .ok()?
        .take(1024)
        .read_to_end(&mut head)
        .ok()?;
    Some(head)
}

pub fn prepare_editor(
    workdir: &Path,
    rel: &str,
    line: Option<u32>,
    configured: Option<&str>,
) -> Result<EditorTarget, OpenError> {
    let file = resolve_in_workdir(workdir, rel)?;
    if std::fs::symlink_metadata(&file).is_err() {
        return Err(OpenError::Missing(rel.to_string()));
    }
    let os = Os::current();
    let has_editor = configured.is_some_and(|c| !c.trim().is_empty());
    // Only inspect the file when its verdict matters (no editor configured).
    let runnable = !has_editor && file_may_run(&file, os);
    editor_target(
        configured,
        os,
        CommandStyle::native(),
        &file,
        line,
        workdir,
        runnable,
    )
}

pub fn prepare_terminal(workdir: &Path, configured: Option<&str>) -> Result<Invocation, OpenError> {
    let env_terminal = std::env::var("TERMINAL").ok();
    terminal_invocation(
        configured,
        Os::current(),
        CommandStyle::native(),
        env_terminal.as_deref(),
        &|p| process::find_program(p).is_some(),
        workdir,
    )
}

/// What "Reveal" shows: the work tree root for `None`, else the file, or its nearest existing
/// parent inside the work tree when it was deleted.
/// A symlink reveals its folder: the file manager would show the link's target, which may be
/// outside the work tree.
pub fn reveal_target(workdir: &Path, rel: Option<&str>) -> Result<PathBuf, OpenError> {
    let Some(rel) = rel else {
        return Ok(workdir.to_path_buf());
    };
    let path = resolve_in_workdir(workdir, rel)?;
    let is_link = std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
    let shown = match (is_link, path.parent()) {
        (true, Some(parent)) => parent.to_path_buf(),
        _ => path,
    };
    Ok(nearest_existing(workdir, &shown))
}

/// Starts `invocation` detached, with `cwd` as its working directory. `argv[0]` is resolved to
/// an absolute path on `PATH` first, so a program in the work tree can never shadow it.
pub fn run_invocation(invocation: &Invocation, cwd: &Path) -> Result<(), OpenError> {
    let (program, args) = invocation
        .argv
        .split_first()
        .ok_or(OpenError::Launch(LaunchError::Split(SplitError::Empty)))?;
    let program =
        process::find_program(program).ok_or_else(|| LaunchError::NotFound(program.clone()))?;
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
        assert_eq!(
            resolve_in_workdir(root, "src/a b.rs").unwrap(),
            root.join("src/a b.rs")
        );
        for bad in [
            "",
            "../outside",
            "src/../../x",
            "/etc/passwd",
            "./a",
            "a/./b",
            "a/.",
            "a\0b",
        ] {
            assert!(
                matches!(resolve_in_workdir(root, bad), Err(OpenError::InvalidPath)),
                "{bad:?}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn resolve_in_workdir_rejects_windows_escapes() {
        let root = Path::new(r"C:\repo");
        for bad in [
            r"C:\x",
            r"C:x",
            r"\\server\share\x",
            r"a\..\..\x",
            "a:stream",
        ] {
            assert!(
                matches!(resolve_in_workdir(root, bad), Err(OpenError::InvalidPath)),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn nearest_existing_walks_up_but_never_out_of_the_workdir() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        assert_eq!(
            nearest_existing(dir.path(), &dir.path().join("src/gone/x.rs")),
            dir.path().join("src")
        );
        assert_eq!(
            nearest_existing(dir.path(), &dir.path().join("gone.rs")),
            dir.path()
        );
    }

    #[test]
    fn placeholders_expand_inside_words() {
        let file = Path::new("/r/my file.rs");
        let dir = Path::new("/r");
        assert_eq!(
            expand_placeholders("{file}:{line}", Some(file), Some(12), dir),
            "/r/my file.rs:12"
        );
        assert_eq!(
            expand_placeholders("--line={line}", Some(file), None, dir),
            "--line=1"
        );
        assert_eq!(
            expand_placeholders("--cwd={dir}", None, None, dir),
            "--cwd=/r"
        );
        assert_eq!(
            expand_placeholders("{other} {", Some(file), None, dir),
            "{other} {"
        );
        // Inserted text is not expanded again.
        assert_eq!(
            expand_placeholders("{dir}", None, None, Path::new("/x/{line}")),
            "/x/{line}"
        );
    }

    #[test]
    fn editor_invocation_keeps_a_spaced_path_as_one_argument() {
        let file = Path::new("/r/my file;$(rm).rs");
        let inv = editor_invocation(
            "code --goto {file}:{line}",
            Posix,
            file,
            Some(3),
            Path::new("/r"),
        )
        .unwrap();
        assert_eq!(argv(&inv), ["code", "--goto", "/r/my file;$(rm).rs:3"]);
        let inv = editor_invocation(
            r#""C:\Program Files\Sublime Text\subl.exe" {file}:{line}"#,
            Windows,
            file,
            None,
            Path::new("/r"),
        )
        .unwrap();
        assert_eq!(
            argv(&inv),
            [
                r"C:\Program Files\Sublime Text\subl.exe",
                "/r/my file;$(rm).rs:1"
            ]
        );
    }

    #[test]
    fn editor_invocation_appends_the_file_without_a_placeholder() {
        let inv = editor_invocation(
            "gedit --new-window",
            Posix,
            Path::new("/r/a.rs"),
            None,
            Path::new("/r"),
        )
        .unwrap();
        assert_eq!(argv(&inv), ["gedit", "--new-window", "/r/a.rs"]);
    }

    #[test]
    fn may_execute_per_os() {
        assert!(may_execute(Path::new("setup.EXE"), Os::Windows, false));
        assert!(may_execute(Path::new("run.bat"), Os::Windows, false));
        assert!(may_execute(Path::new("x.lnk"), Os::Windows, false));
        assert!(may_execute(Path::new("app.js"), Os::Windows, false));
        assert!(may_execute(Path::new("tool.ps1"), Os::Windows, false));
        assert!(may_execute(Path::new("x.reg"), Os::Windows, false));
        assert!(!may_execute(Path::new("README.md"), Os::Windows, false));
        assert!(may_execute(Path::new("build"), Os::Linux, true));
        assert!(may_execute(Path::new("x.desktop"), Os::Linux, false));
        assert!(may_execute(Path::new("x.sh"), Os::Linux, false));
        assert!(may_execute(Path::new("x.AppImage"), Os::Linux, false));
        assert!(may_execute(Path::new("x.command"), Os::MacOs, false));
        assert!(may_execute(Path::new("Tool.app"), Os::MacOs, false));
        assert!(may_execute(Path::new("x.terminal"), Os::MacOs, false));
        assert!(!may_execute(Path::new("notes.txt"), Os::MacOs, false));
        assert!(!may_execute(Path::new("Makefile"), Os::Linux, false));
    }

    #[test]
    fn may_execute_checks_every_platforms_list_on_every_os() {
        // Wine on Linux/macOS hands Windows programs and shortcuts to `xdg-open`/`open` too.
        for os in [Os::Linux, Os::MacOs, Os::Windows] {
            for name in [
                "setup.msi",
                "x.lnk",
                "run.bat",
                "run.CMD",
                "x.desktop",
                "Tool.app",
                "x.sh",
                "x.msu",
                "x.rdp",
                "x.ahk",
                "x.pyc",
                "x.pyo",
                "x.pyzw",
                "x.theme",
                "x.themepack",
                "x.deskthemepack",
                "x.searchConnector-ms",
                "x.xll",
                "x.vb",
                "x.prefPane",
                "x.mobileconfig",
                "x.PY",
            ] {
                assert!(may_execute(Path::new(name), os, false), "{name} on {os:?}");
            }
            assert!(!may_execute(Path::new("README.md"), os, false));
        }
    }

    #[test]
    fn looks_runnable_detects_desktop_entries_and_installers() {
        assert!(looks_runnable(b"[Desktop Entry]\nExec=x\n"));
        assert!(looks_runnable(b"# comment\n\n  [Desktop Entry]\nExec=x\n"));
        assert!(looks_runnable(b"\xef\xbb\xbf[Desktop Entry]\n"));
        assert!(looks_runnable(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1rest"));
        assert!(!looks_runnable(b"# Notes\n[Desktop Entry] is a header\n"));
    }

    #[test]
    fn an_old_office_document_is_not_mistaken_for_an_installer() {
        let dir = tempfile::TempDir::new().unwrap();
        let ole = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1rest";
        std::fs::write(dir.path().join("report.doc"), ole).unwrap();
        std::fs::write(dir.path().join("notes.txt"), ole).unwrap();
        assert!(matches!(
            prepare_editor(dir.path(), "report.doc", None, None),
            Ok(EditorTarget::SystemDefault(_))
        ));
        assert!(matches!(
            prepare_editor(dir.path(), "notes.txt", None, None),
            Err(OpenError::MayExecute(_))
        ));
    }

    #[test]
    fn an_extensionless_desktop_entry_is_refused() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("LAUNCH"), "[Desktop Entry]\nExec=sh -c x\n").unwrap();
        assert!(matches!(
            prepare_editor(dir.path(), "LAUNCH", None, None),
            Err(OpenError::MayExecute(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_symlink_out_of_the_work_tree_is_refused() {
        let dir = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "s").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("etc")).unwrap();
        assert!(matches!(
            prepare_editor(dir.path(), "etc/secret.txt", None, Some("code")),
            Err(OpenError::InvalidPath)
        ));
        assert!(matches!(
            reveal_target(dir.path(), Some("etc/secret.txt")),
            Err(OpenError::InvalidPath)
        ));
        assert!(matches!(
            reveal_target(dir.path(), Some("etc/gone/x.txt")),
            Err(OpenError::InvalidPath)
        ));
        // A symlink inside the work tree is fine.
        std::fs::create_dir(dir.path().join("real")).unwrap();
        std::fs::write(dir.path().join("real/a.txt"), "a").unwrap();
        std::os::unix::fs::symlink("real", dir.path().join("alias")).unwrap();
        assert!(prepare_editor(dir.path(), "alias/a.txt", None, Some("code")).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn revealing_a_file_symlink_shows_the_link_not_its_target() {
        let dir = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "s").unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), dir.path().join("link"))
            .unwrap();
        assert_eq!(reveal_target(dir.path(), Some("link")).unwrap(), dir.path());
    }

    #[test]
    fn looks_runnable_detects_scripts_and_binaries() {
        assert!(looks_runnable(b"#!/bin/sh\n"));
        assert!(looks_runnable(b"\x7fELF\x02\x01"));
        assert!(looks_runnable(b"MZ\x90\x00"));
        assert!(looks_runnable(b"\xcf\xfa\xed\xfe"));
        assert!(!looks_runnable(b"# Title\n"));
        assert!(!looks_runnable(b""));
    }

    #[test]
    fn editor_target_uses_the_default_app_only_for_safe_files() {
        let dir = Path::new("/r");
        assert_eq!(
            editor_target(
                None,
                Os::Linux,
                Posix,
                Path::new("/r/a.md"),
                None,
                dir,
                false
            )
            .unwrap(),
            EditorTarget::SystemDefault(PathBuf::from("/r/a.md"))
        );
        assert_eq!(
            editor_target(
                Some("  "),
                Os::Linux,
                Posix,
                Path::new("/r/a.md"),
                None,
                dir,
                false
            )
            .unwrap(),
            EditorTarget::SystemDefault(PathBuf::from("/r/a.md"))
        );
        assert!(matches!(
            editor_target(None, Os::Linux, Posix, Path::new("/r/a.md"), None, dir, true),
            Err(OpenError::MayExecute(name)) if name == "a.md"
        ));
    }

    #[test]
    fn editor_target_refuses_an_executable_without_an_editor() {
        let err = editor_target(
            None,
            Os::Windows,
            Windows,
            Path::new("/r/setup.exe"),
            None,
            Path::new("/r"),
            false,
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "No editor is configured, and opening setup.exe with the system default app could run it. Set an editor command in Preferences."
        );
        // A configured editor opens anything.
        assert!(matches!(
            editor_target(
                Some("code"),
                Os::Windows,
                Windows,
                Path::new("/r/setup.exe"),
                None,
                Path::new("/r"),
                true
            ),
            Ok(EditorTarget::Command(_))
        ));
    }

    #[test]
    fn default_terminal_per_os() {
        let none = |_: &str| false;
        let only = |name: &'static str| move |p: &str| p == name;
        assert_eq!(
            argv(&default_terminal(Os::MacOs, None, &none).unwrap()),
            ["open", "-a", "Terminal", "{dir}"]
        );
        assert_eq!(
            argv(&default_terminal(Os::Windows, None, &only("wt")).unwrap()),
            // `;` separates wt subcommands, so the directory is never on its command line: wt
            // starts in its working directory, which is the work tree root.
            ["wt", "-d", "."]
        );
        let cmd = default_terminal(Os::Windows, None, &none).unwrap();
        assert_eq!((argv(&cmd), cmd.new_console), (vec!["cmd", "/K"], true));
        assert_eq!(
            argv(&default_terminal(Os::Linux, Some("foot --app-id x"), &none).unwrap()),
            ["foot", "--app-id", "x"]
        );
        assert_eq!(
            argv(&default_terminal(Os::Linux, Some(""), &only("xterm")).unwrap()),
            ["xterm"]
        );
        assert_eq!(
            argv(&default_terminal(Os::Linux, None, &only("gnome-terminal")).unwrap()),
            ["gnome-terminal", "--working-directory={dir}"]
        );
        let both = |p: &str| p == "x-terminal-emulator" || p == "konsole";
        assert_eq!(
            argv(&default_terminal(Os::Linux, None, &both).unwrap()),
            ["x-terminal-emulator"]
        );
        assert_eq!(default_terminal(Os::Linux, None, &none), None);
    }

    #[test]
    fn terminal_invocation_prefers_the_configured_command_and_expands_dir() {
        let none = |_: &str| false;
        let dir = Path::new("/r w");
        let inv = terminal_invocation(
            Some("kitty --directory {dir}"),
            Os::Linux,
            Posix,
            None,
            &none,
            dir,
        )
        .unwrap();
        assert_eq!(argv(&inv), ["kitty", "--directory", "/r w"]);
        let inv = terminal_invocation(None, Os::MacOs, Posix, None, &none, dir).unwrap();
        assert_eq!(argv(&inv), ["open", "-a", "Terminal", "/r w"]);
        assert_eq!(
            terminal_invocation(None, Os::Linux, Posix, None, &none, dir)
                .unwrap_err()
                .to_string(),
            "No terminal found. Set a terminal command in Preferences."
        );
        assert_eq!(
            terminal_invocation(Some("kitty \"x"), Os::Linux, Posix, None, &none, dir)
                .unwrap_err()
                .to_string(),
            "The command has an unclosed quote."
        );
    }

    #[test]
    fn prepare_editor_reports_a_missing_file_and_reveal_target_falls_back() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        assert_eq!(
            prepare_editor(dir.path(), "gone.txt", None, Some("code"))
                .unwrap_err()
                .to_string(),
            "gone.txt no longer exists in the working tree."
        );
        assert!(matches!(
            prepare_editor(dir.path(), "a.txt", Some(2), Some("code")),
            Ok(EditorTarget::Command(_))
        ));
        assert_eq!(
            prepare_editor(dir.path(), "a.txt", None, None).unwrap(),
            EditorTarget::SystemDefault(dir.path().join("a.txt"))
        );
        assert!(matches!(
            prepare_editor(dir.path(), "../a.txt", None, Some("code")),
            Err(OpenError::InvalidPath)
        ));
        assert_eq!(reveal_target(dir.path(), None).unwrap(), dir.path());
        assert_eq!(
            reveal_target(dir.path(), Some("gone/x.txt")).unwrap(),
            dir.path()
        );
        assert_eq!(
            reveal_target(dir.path(), Some("a.txt")).unwrap(),
            dir.path().join("a.txt")
        );
        assert!(matches!(
            reveal_target(dir.path(), Some("../x")),
            Err(OpenError::InvalidPath)
        ));
    }

    #[test]
    fn prepare_editor_refuses_a_script_by_its_contents_without_an_editor() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("notes.txt"), "#!/bin/sh\nrm -rf ~\n").unwrap();
        assert!(matches!(
            prepare_editor(dir.path(), "notes.txt", None, None),
            Err(OpenError::MayExecute(name)) if name == "notes.txt"
        ));
        assert!(matches!(
            prepare_editor(dir.path(), "notes.txt", None, Some("code")),
            Ok(EditorTarget::Command(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn has_exec_bit_reads_the_mode_and_prepare_editor_refuses_it() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("s");
        std::fs::write(&file, "plain").unwrap();
        assert!(!has_exec_bit(&file));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(has_exec_bit(&file));
        assert!(matches!(
            prepare_editor(dir.path(), "s", None, None),
            Err(OpenError::MayExecute(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn prepare_editor_checks_what_a_symlink_points_to() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("launcher.desktop"), "[Desktop Entry]\n").unwrap();
        std::os::unix::fs::symlink("launcher.desktop", dir.path().join("readme.txt")).unwrap();
        std::os::unix::fs::symlink("nowhere", dir.path().join("dangling.txt")).unwrap();
        assert!(matches!(
            prepare_editor(dir.path(), "readme.txt", None, None),
            Err(OpenError::MayExecute(_))
        ));
        assert!(matches!(
            prepare_editor(dir.path(), "dangling.txt", None, None),
            Err(OpenError::MayExecute(_))
        ));
    }
}
