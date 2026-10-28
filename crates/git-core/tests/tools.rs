mod common;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use common::{commit_all, write_file};
use git2::{Config, ConfigLevel, Repository};
use git_core::tools::{
    file_version_contents, git_tools, git_tools_from, prepare_diff_tool_with,
    prepare_merge_tool_with, resolve_tool, sweep_stale_tool_dirs, ConfigSource, DiffSource,
    FileVersion, PreparedDiffTool, ToolCommand, ToolError, ToolKind,
};
use tempfile::TempDir;

/// Points libgit2's system/XDG/global config search paths at an empty directory for this whole
/// test binary, so the developer's own `diff.tool` can never leak in (#111). Every test calls
/// it before touching config; the first call does the work.
fn isolate() {
    static EMPTY: OnceLock<TempDir> = OnceLock::new();
    EMPTY.get_or_init(|| {
        let dir = TempDir::new().unwrap();
        for level in [
            ConfigLevel::ProgramData,
            ConfigLevel::System,
            ConfigLevel::XDG,
            ConfigLevel::Global,
        ] {
            // SAFETY: runs once, under the `OnceLock`, before this binary reads any config.
            unsafe { git2::opts::set_search_path(level, dir.path()).unwrap() };
        }
        dir
    });
}

fn init_repo() -> (TempDir, Repository) {
    isolate();
    common::init_repo()
}

/// A config like `repo.config()`, but whose global level is a fresh temp file holding
/// `global`; the repository's own `.git/config` is the local level.
fn user_config(repo: Option<&Repository>, global: &[(&str, &str)]) -> (TempDir, Config) {
    isolate();
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("gitconfig");
    std::fs::write(&file, "").unwrap();
    let mut config = Config::new().unwrap();
    config.add_file(&file, ConfigLevel::Global, false).unwrap();
    if let Some(repo) = repo {
        config
            .add_file(&repo.path().join("config"), ConfigLevel::Local, false)
            .unwrap();
    }
    {
        let mut level = config.open_level(ConfigLevel::Global).unwrap();
        for (key, value) in global {
            level.set_str(key, value).unwrap();
        }
    }
    (dir, config)
}

fn set_local(repo: &Repository, entries: &[(&str, &str)]) {
    let mut config = repo
        .config()
        .unwrap()
        .open_level(ConfigLevel::Local)
        .unwrap();
    for (key, value) in entries {
        config.set_str(key, value).unwrap();
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn env(prepared_env: &[(String, std::ffi::OsString)], name: &str) -> PathBuf {
    prepared_env
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into())
        .unwrap()
}

fn same_file(a: &Path, b: &Path) -> bool {
    a.canonicalize().unwrap() == b.canonicalize().unwrap()
}

// ---------------------------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------------------------

#[test]
fn a_cmd_beats_a_preset_and_runs_in_the_shell() {
    let (_d, config) = user_config(
        None,
        &[
            ("diff.tool", "meld"),
            ("difftool.meld.cmd", "meld --newtab \"$LOCAL\" \"$REMOTE\""),
        ],
    );
    let tool = resolve_tool(&config, ToolKind::Diff).unwrap();
    assert_eq!(tool.name, "meld");
    assert_eq!(tool.source, ConfigSource::Global);
    assert!(!tool.via_merge_tool);
    assert_eq!(
        tool.command,
        ToolCommand::Shell("meld --newtab \"$LOCAL\" \"$REMOTE\"".into())
    );
}

#[test]
fn a_preset_honours_the_path_setting() {
    let (_d, config) = user_config(
        None,
        &[
            ("merge.tool", "kdiff3"),
            ("mergetool.kdiff3.path", "/opt/kdiff3/bin/kdiff3"),
        ],
    );
    let tool = resolve_tool(&config, ToolKind::Merge).unwrap();
    assert_eq!(
        tool.command,
        ToolCommand::Preset {
            program: "/opt/kdiff3/bin/kdiff3".into(),
            args: ["--auto", "-o", "$MERGED", "$BASE", "$LOCAL", "$REMOTE"]
                .map(String::from)
                .to_vec(),
        }
    );
    assert!(!tool.trust_exit_code);
}

#[test]
fn the_diff_tool_falls_back_to_merge_tool() {
    let (_d, config) = user_config(None, &[("merge.tool", "p4merge")]);
    let tool = resolve_tool(&config, ToolKind::Diff).unwrap();
    assert!(tool.via_merge_tool);
    assert_eq!(tool.name, "p4merge");
}

#[test]
fn trust_exit_code_is_read_for_merge_tools() {
    let (_d, config) = user_config(
        None,
        &[
            ("merge.tool", "t"),
            ("mergetool.t.cmd", "true"),
            ("mergetool.t.trustExitCode", "yes"),
        ],
    );
    assert!(
        resolve_tool(&config, ToolKind::Merge)
            .unwrap()
            .trust_exit_code
    );
    let (_d, config) = user_config(
        None,
        &[
            ("merge.tool", "t"),
            ("mergetool.t.cmd", "true"),
            ("mergetool.t.trustExitCode", "false"),
        ],
    );
    assert!(
        !resolve_tool(&config, ToolKind::Merge)
            .unwrap()
            .trust_exit_code
    );
}

#[test]
fn unusable_configurations_have_specific_messages() {
    let (_d, empty) = user_config(None, &[]);
    assert_eq!(
        resolve_tool(&empty, ToolKind::Diff).unwrap_err().to_string(),
        "No diff tool is configured. Set diff.tool in your git config, for example: git config --global diff.tool meld"
    );
    assert_eq!(
        resolve_tool(&empty, ToolKind::Merge).unwrap_err().to_string(),
        "No merge tool is configured. Set merge.tool in your git config, for example: git config --global merge.tool meld"
    );
    let (_d, vim) = user_config(None, &[("diff.tool", "vimdiff")]);
    assert_eq!(
        resolve_tool(&vim, ToolKind::Diff).unwrap_err().to_string(),
        "vimdiff runs in a terminal, which Browsitory can't host. Choose a graphical tool, or set difftool.vimdiff.cmd to run it in a terminal window."
    );
    let (_d, unknown) = user_config(None, &[("merge.tool", "frobmerge")]);
    assert_eq!(
        resolve_tool(&unknown, ToolKind::Merge).unwrap_err().to_string(),
        "merge.tool is set to \"frobmerge\", but mergetool.frobmerge.cmd is not set and Browsitory has no built-in command for it."
    );
}

#[test]
fn a_repositorys_own_config_can_never_choose_or_define_a_tool() {
    let (_dir, repo) = init_repo();
    set_local(
        &repo,
        &[
            ("diff.tool", "evil"),
            ("difftool.evil.cmd", "touch /tmp/pwned"),
            ("merge.tool", "meld"),
        ],
    );
    let (_d, config) = user_config(Some(&repo), &[]);
    assert_eq!(
        resolve_tool(&config, ToolKind::Diff).unwrap_err().to_string(),
        "diff.tool is set to \"evil\" only in this repository's git config. Browsitory ignores tool settings from a repository's own config, so a repository can't make it run commands. Set it in your global git config instead, for example: git config --global diff.tool evil"
    );
    assert!(matches!(
        resolve_tool(&config, ToolKind::Merge),
        Err(ToolError::RepositoryConfigIgnored { setting, name }) if setting == "merge.tool" && name == "meld"
    ));
}

#[test]
fn a_repository_cannot_override_the_users_tool_command_or_program() {
    let (_dir, repo) = init_repo();
    set_local(
        &repo,
        &[
            ("diff.tool", "evil"),
            ("difftool.meld.cmd", "touch /tmp/pwned"),
            ("difftool.meld.path", "/tmp/evil"),
            ("mergetool.t.cmd", "touch /tmp/pwned"),
            ("mergetool.t.trustExitCode", "true"),
        ],
    );
    let (_d, config) = user_config(
        Some(&repo),
        &[
            ("diff.tool", "meld"),
            ("merge.tool", "t"),
            ("mergetool.t.cmd", "mine"),
        ],
    );
    let diff = resolve_tool(&config, ToolKind::Diff).unwrap();
    assert_eq!(diff.name, "meld");
    assert_eq!(
        diff.command,
        ToolCommand::Preset {
            program: "meld".into(),
            args: vec!["$LOCAL".into(), "$REMOTE".into()],
        }
    );
    let merge = resolve_tool(&config, ToolKind::Merge).unwrap();
    assert_eq!(merge.command, ToolCommand::Shell("mine".into()));
    assert!(!merge.trust_exit_code);
}

#[test]
fn git_tools_reports_both_tools_with_problems_as_text() {
    let (_d, config) = user_config(None, &[("diff.tool", "meld"), ("merge.tool", "emerge")]);
    let tools = git_tools_from(&config).unwrap();
    let diff = tools.diff_tool.unwrap();
    assert_eq!(
        (diff.name.as_str(), diff.command.as_deref(), diff.problem),
        ("meld", Some("meld $LOCAL $REMOTE"), None)
    );
    assert_eq!(diff.source, ConfigSource::Global);
    let merge = tools.merge_tool.unwrap();
    assert_eq!(merge.name, "emerge");
    assert!(merge
        .problem
        .unwrap()
        .starts_with("emerge runs in a terminal"));
}

#[test]
fn git_tools_explains_an_ignored_repository_setting() {
    let (_dir, repo) = init_repo();
    set_local(&repo, &[("merge.tool", "meld")]);
    let tools = git_tools(&repo).unwrap();
    let diff = tools.diff_tool.unwrap();
    assert_eq!(diff.name, "meld");
    assert_eq!(diff.source, ConfigSource::Local);
    assert!(diff.via_merge_tool);
    assert!(diff.command.is_none());
    assert!(diff
        .problem
        .unwrap()
        .starts_with("merge.tool is set to \"meld\" only in this repository's git config."));
    assert_eq!(tools.merge_tool.unwrap().source, ConfigSource::Local);
}

#[test]
fn git_tools_is_empty_without_configuration() {
    let (_dir, repo) = init_repo();
    let tools = git_tools(&repo).unwrap();
    assert_eq!((tools.diff_tool, tools.merge_tool), (None, None));
}

// ---------------------------------------------------------------------------------------------
// File versions and temp sides
// ---------------------------------------------------------------------------------------------

#[test]
fn file_versions_come_from_head_index_commits_and_parents() {
    let (dir, repo) = init_repo();
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

    let get = |v: FileVersion| {
        file_version_contents(&repo, "a.txt", &v, usize::MAX)
            .unwrap()
            .map(|b| String::from_utf8(b).unwrap())
    };
    assert_eq!(get(FileVersion::Head).as_deref(), Some("two\n"));
    assert_eq!(get(FileVersion::Index).as_deref(), Some("three\n"));
    assert_eq!(
        get(FileVersion::Commit(first.clone())).as_deref(),
        Some("one\n")
    );
    assert_eq!(
        get(FileVersion::FirstParent(second)).as_deref(),
        Some("one\n")
    );
    assert_eq!(get(FileVersion::FirstParent(first)), None);
    assert_eq!(
        file_version_contents(&repo, "missing.txt", &FileVersion::Head, usize::MAX).unwrap(),
        None
    );
    assert!(matches!(
        file_version_contents(&repo, "a.txt", &FileVersion::Head, 2),
        Err(ToolError::TooLarge(p)) if p == "a.txt"
    ));
    for bad in ["../x", "/etc/passwd", "", "a/./b"] {
        assert!(
            matches!(
                file_version_contents(&repo, bad, &FileVersion::Head, usize::MAX),
                Err(ToolError::InvalidPath)
            ),
            "{bad:?}"
        );
    }
}

/// Any existing program: preparing a launch resolves it, but these tests never run it. Using a
/// preset with `.path` keeps them independent of `sh` (Windows CI).
fn some_program() -> String {
    std::env::current_exe()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}

fn diff_config(repo: &Repository) -> (TempDir, Config) {
    let program = some_program();
    user_config(
        Some(repo),
        &[("diff.tool", "meld"), ("difftool.meld.path", &program)],
    )
}

fn prepare(
    repo: &Repository,
    config: &Config,
    path: &str,
    source: DiffSource,
    root: &Path,
) -> PreparedDiffTool {
    prepare_diff_tool_with(repo, config, path, &source, root).unwrap()
}

#[test]
fn unstaged_diff_compares_the_index_with_the_real_work_tree_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "src/main.rs", "old\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "src/main.rs", "new\n");
    let (_c, config) = diff_config(&repo);
    let root = TempDir::new().unwrap();

    let prepared = prepare(
        &repo,
        &config,
        "src/main.rs",
        DiffSource::Unstaged,
        root.path(),
    );

    let local = env(&prepared.invocation.env, "LOCAL");
    assert_eq!(local.file_name().unwrap(), "main_LOCAL.rs");
    assert!(local.starts_with(&prepared.temp_dir));
    assert!(prepared.temp_dir.starts_with(root.path()));
    assert!(prepared
        .temp_dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("browsitory-tool-"));
    assert_eq!(read(&local), "old\n");
    let remote = env(&prepared.invocation.env, "REMOTE");
    assert!(same_file(&remote, &dir.path().join("src/main.rs")));
    assert!(same_file(
        &env(&prepared.invocation.env, "MERGED"),
        &dir.path().join("src/main.rs")
    ));
    assert!(same_file(&prepared.invocation.cwd, dir.path()));
    // Every path handed to the tool is absolute, so none can be taken for an option.
    for (_, value) in &prepared.invocation.env {
        assert!(Path::new(value).is_absolute());
    }
}

#[cfg(unix)]
#[test]
fn side_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "old\n");
    commit_all(&repo, "base");
    let (_c, config) = diff_config(&repo);
    let root = TempDir::new().unwrap();
    let prepared = prepare(&repo, &config, "a.txt", DiffSource::Staged, root.path());
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&prepared.temp_dir), 0o700);
    assert_eq!(mode(&env(&prepared.invocation.env, "LOCAL")), 0o600);
}

#[test]
fn staged_commit_and_range_diffs_write_both_sides() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "old.txt", "v1\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap().to_string();
    std::fs::remove_file(dir.path().join("old.txt")).unwrap();
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
    write_file(dir.path(), "new.txt", "v4\n");
    let (_c, config) = diff_config(&repo);
    let root = TempDir::new().unwrap();
    let sides = |source: DiffSource| {
        let p = prepare(&repo, &config, "new.txt", source, root.path());
        (
            read(&env(&p.invocation.env, "LOCAL")),
            read(&env(&p.invocation.env, "REMOTE")),
        )
    };

    assert_eq!(sides(DiffSource::Staged), ("v2\n".into(), "v3\n".into()));
    assert_eq!(
        sides(DiffSource::Commit {
            commit_id: renamed.clone(),
            old_path: Some("old.txt".into())
        }),
        ("v1\n".into(), "v2\n".into())
    );
    assert_eq!(
        sides(DiffSource::Range {
            from_id: base.clone(),
            to_id: Some(renamed),
            old_path: Some("old.txt".into())
        }),
        ("v1\n".into(), "v2\n".into())
    );
    assert_eq!(
        sides(DiffSource::Range {
            from_id: base,
            to_id: None,
            old_path: Some("old.txt".into())
        }),
        ("v1\n".into(), "v4\n".into())
    );
}

#[cfg(unix)]
#[test]
fn a_missing_side_is_an_empty_file_and_presets_run_without_a_shell() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "new.txt", "fresh\n");
    let (_c, config) = user_config(
        Some(&repo),
        &[("diff.tool", "meld"), ("difftool.meld.path", "/bin/echo")],
    );
    let root = TempDir::new().unwrap();

    let prepared = prepare(&repo, &config, "new.txt", DiffSource::Unstaged, root.path());

    assert_eq!(read(&env(&prepared.invocation.env, "LOCAL")), "");
    assert_eq!(prepared.invocation.program, Path::new("/bin/echo"));
    assert!(same_file(
        Path::new(&prepared.invocation.args[1]),
        &dir.path().join("new.txt")
    ));
}

#[cfg(unix)]
#[test]
fn a_symlink_is_compared_as_its_target_text_never_followed() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    std::os::unix::fs::symlink("/etc/passwd", dir.path().join("link")).unwrap();
    let (_c, config) = diff_config(&repo);
    let root = TempDir::new().unwrap();
    let prepared = prepare(&repo, &config, "link", DiffSource::Unstaged, root.path());
    let remote = env(&prepared.invocation.env, "REMOTE");
    assert!(remote.starts_with(&prepared.temp_dir));
    assert_eq!(read(&remote), "/etc/passwd");
}

#[cfg(unix)]
#[test]
fn a_path_through_a_symlinked_directory_out_of_the_work_tree_is_refused() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let outside = TempDir::new().unwrap();
    std::fs::write(outside.path().join("secret"), "s").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("out")).unwrap();
    let (_c, config) = diff_config(&repo);
    let root = TempDir::new().unwrap();
    assert!(matches!(
        prepare_diff_tool_with(
            &repo,
            &config,
            "out/secret",
            &DiffSource::Unstaged,
            root.path()
        ),
        Err(ToolError::InvalidPath)
    ));
}

#[test]
fn prepare_rejects_an_escaping_path_and_leaves_nothing_behind() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let (_c, config) = diff_config(&repo);
    let root = TempDir::new().unwrap();
    for bad in ["../a.txt", "/etc/passwd", "./a.txt"] {
        assert!(
            matches!(
                prepare_diff_tool_with(&repo, &config, bad, &DiffSource::Unstaged, root.path()),
                Err(ToolError::InvalidPath)
            ),
            "{bad:?}"
        );
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn a_tool_that_cannot_be_found_leaves_no_temp_dir() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let (_c, config) = user_config(
        Some(&repo),
        &[
            ("diff.tool", "meld"),
            ("difftool.meld.path", "browsitory-no-such-meld"),
        ],
    );
    let root = TempDir::new().unwrap();
    let err = prepare_diff_tool_with(&repo, &config, "a.txt", &DiffSource::Staged, root.path())
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Couldn't find the program 'browsitory-no-such-meld'."
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

fn conflicted_repo() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "base\n");
    commit_all(&repo, "base");
    let main = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "shared.txt", "theirs\n");
    write_file(dir.path(), "added.txt", "theirs added\n");
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    write_file(dir.path(), "shared.txt", "ours\n");
    write_file(dir.path(), "added.txt", "ours added\n");
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    (dir, repo)
}

#[cfg(unix)]
fn merge_config(repo: &Repository, cmd: &str, trust: bool) -> (TempDir, Config) {
    user_config(
        Some(repo),
        &[
            ("merge.tool", "t"),
            ("mergetool.t.cmd", cmd),
            (
                "mergetool.t.trustExitCode",
                if trust { "true" } else { "false" },
            ),
        ],
    )
}

fn preset_merge_config(repo: &Repository, trust: bool) -> (TempDir, Config) {
    let program = some_program();
    user_config(
        Some(repo),
        &[
            ("merge.tool", "meld"),
            ("mergetool.meld.path", &program),
            (
                "mergetool.meld.trustExitCode",
                if trust { "true" } else { "false" },
            ),
        ],
    )
}

#[test]
fn merge_tool_sides_come_from_the_conflict_stages() {
    let (dir, repo) = conflicted_repo();
    let (_c, config) = preset_merge_config(&repo, false);
    let root = TempDir::new().unwrap();

    let prepared = prepare_merge_tool_with(&repo, &config, "shared.txt", root.path()).unwrap();

    let e = &prepared.invocation.env;
    assert_eq!(read(&env(e, "LOCAL")), "ours\n");
    assert_eq!(read(&env(e, "REMOTE")), "theirs\n");
    assert_eq!(read(&env(e, "BASE")), "base\n");
    assert_eq!(env(e, "BASE").file_name().unwrap(), "shared_BASE.txt");
    assert!(same_file(&env(e, "MERGED"), &dir.path().join("shared.txt")));
    assert_eq!(
        prepared.merged_before,
        std::fs::read(dir.path().join("shared.txt")).unwrap()
    );
    assert!(!prepared.trust_exit_code);
}

#[test]
fn an_add_add_conflict_gets_an_empty_base() {
    let (_dir, repo) = conflicted_repo();
    let (_c, config) = preset_merge_config(&repo, true);
    let root = TempDir::new().unwrap();
    let prepared = prepare_merge_tool_with(&repo, &config, "added.txt", root.path()).unwrap();
    assert_eq!(read(&env(&prepared.invocation.env, "BASE")), "");
    assert_eq!(
        read(&env(&prepared.invocation.env, "LOCAL")),
        "ours added\n"
    );
    assert!(prepared.trust_exit_code);
}

#[test]
fn merge_tool_refuses_paths_without_a_text_conflict() {
    let (dir, repo) = conflicted_repo();
    write_file(dir.path(), "other.txt", "x\n");
    let (_c, config) = preset_merge_config(&repo, false);
    let root = TempDir::new().unwrap();
    assert_eq!(
        prepare_merge_tool_with(&repo, &config, "other.txt", root.path())
            .unwrap_err()
            .to_string(),
        "no conflict found for path 'other.txt'"
    );
    assert!(matches!(
        prepare_merge_tool_with(&repo, &config, "../shared.txt", root.path()),
        Err(ToolError::InvalidPath)
    ));
}

#[test]
fn sweep_removes_only_old_tool_dirs() {
    let root = TempDir::new().unwrap();
    std::fs::create_dir(root.path().join("browsitory-tool-old")).unwrap();
    std::fs::create_dir(root.path().join("someone-elses-dir")).unwrap();
    sweep_stale_tool_dirs(root.path(), Duration::from_secs(3600));
    assert!(root.path().join("browsitory-tool-old").exists());
    sweep_stale_tool_dirs(root.path(), Duration::ZERO);
    assert!(!root.path().join("browsitory-tool-old").exists());
    assert!(root.path().join("someone-elses-dir").exists());
}

#[cfg(unix)]
#[test]
fn sweep_never_follows_a_symlink() {
    let root = TempDir::new().unwrap();
    let victim = TempDir::new().unwrap();
    std::fs::write(victim.path().join("keep"), "x").unwrap();
    std::os::unix::fs::symlink(victim.path(), root.path().join("browsitory-tool-link")).unwrap();
    sweep_stale_tool_dirs(root.path(), Duration::ZERO);
    assert!(victim.path().join("keep").exists());
}

#[cfg(unix)]
mod spawning {
    use super::*;
    use git_core::tools::{launch_diff_tool, run_merge_tool, MergeToolOutcome};

    fn merge_with(
        cmd: &str,
        trust: bool,
    ) -> (TempDir, Result<MergeToolOutcome, ToolError>, PathBuf) {
        let (dir, repo) = conflicted_repo();
        let (_c, config) = merge_config(&repo, cmd, trust);
        let root = TempDir::new().unwrap();
        let prepared = prepare_merge_tool_with(&repo, &config, "shared.txt", root.path()).unwrap();
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
        assert_eq!(
            merge_with("true", false).1.unwrap(),
            MergeToolOutcome::Unresolved
        );
        assert_eq!(
            merge_with("printf x > \"$MERGED\"; exit 3", true)
                .1
                .unwrap(),
            MergeToolOutcome::Unresolved
        );
        assert_eq!(
            merge_with("exit 0", true).1.unwrap(),
            MergeToolOutcome::Resolved
        );
    }

    #[test]
    fn a_missing_program_is_an_error_not_an_outcome() {
        let err = merge_with("definitely-not-a-merge-tool-4f", false)
            .1
            .unwrap_err();
        assert!(
            matches!(
                err,
                ToolError::ToolFailed {
                    tool: "merge tool",
                    code: 127,
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn file_names_reach_the_cmd_only_as_environment_values() {
        let (dir, repo) = init_repo();
        let name = "$(touch pwned); `touch pwned2`.txt";
        write_file(dir.path(), name, "a\n");
        commit_all(&repo, "base");
        write_file(dir.path(), name, "b\n");
        let (_c, config) = user_config(
            Some(&repo),
            &[
                ("diff.tool", "t"),
                ("difftool.t.cmd", "cat \"$REMOTE\" >/dev/null; exit 5"),
            ],
        );
        let root = TempDir::new().unwrap();
        let prepared = prepare(&repo, &config, name, DiffSource::Unstaged, root.path());
        let err = launch_diff_tool(prepared, Duration::from_secs(5)).unwrap_err();
        assert!(
            matches!(err, ToolError::ToolFailed { code: 5, .. }),
            "{err}"
        );
        assert!(!dir.path().join("pwned").exists());
        assert!(!dir.path().join("pwned2").exists());
    }

    #[test]
    fn a_diff_tool_that_fails_at_once_reports_its_exit_code_and_stderr() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a\n");
        commit_all(&repo, "base");
        let (_c, config) = user_config(
            Some(&repo),
            &[
                ("diff.tool", "t"),
                ("difftool.t.cmd", "echo broken >&2; exit 7"),
            ],
        );
        let root = TempDir::new().unwrap();
        let prepared = prepare(&repo, &config, "a.txt", DiffSource::Unstaged, root.path());
        let temp_dir = prepared.temp_dir.clone();
        let err = launch_diff_tool(prepared, Duration::from_secs(5)).unwrap_err();
        assert_eq!(err.to_string(), "The diff tool failed (exit 7): broken");
        assert!(!temp_dir.exists());
    }

    #[test]
    fn a_long_running_diff_tool_returns_after_the_grace_period() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a\n");
        commit_all(&repo, "base");
        let (_c, config) = user_config(
            Some(&repo),
            &[("diff.tool", "t"), ("difftool.t.cmd", "sleep 2")],
        );
        let root = TempDir::new().unwrap();
        let prepared = prepare(&repo, &config, "a.txt", DiffSource::Unstaged, root.path());
        let started = std::time::Instant::now();
        launch_diff_tool(prepared, Duration::from_millis(200)).unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
