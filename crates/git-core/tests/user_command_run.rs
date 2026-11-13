//! Running user commands (#894): the variables' values from a real repository, and the program
//! run without a shell, in its directory, with a time limit, a cancel and capped output.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::init_repo;
use git_core::process::{find_program_in, CommandStyle};
use git_core::user_command::{
    parse, run, values, Context, ContextError, Expanded, RunError, Values,
};

fn commit_file(repo: &git2::Repository, name: &str, text: &str, message: &str) -> git2::Oid {
    let workdir = repo.workdir().unwrap().to_path_buf();
    std::fs::write(workdir.join(name), text).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(name)).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .into_iter()
        .collect();
    let parents: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap()
}

fn root(repo: &git2::Repository) -> PathBuf {
    repo.workdir().unwrap().to_path_buf()
}

#[test]
fn a_commit_fills_its_full_and_abbreviated_id() {
    let (_dir, repo) = init_repo();
    let id = commit_file(&repo, "a.txt", "a\n", "first");
    let root = root(&repo);
    let short = &id.to_string()[..7];
    let filled = values(
        &repo,
        &root,
        &Context {
            commit: Some(short.to_string()),
            ..Context::default()
        },
    )
    .unwrap();
    assert_eq!(
        filled,
        Values {
            repo: root.to_string_lossy().into_owned(),
            commit: Some(id.to_string()),
            short_commit: Some(short.to_string()),
            ..Values::default()
        }
    );
}

#[test]
fn an_unknown_or_non_hex_commit_is_refused() {
    let (_dir, repo) = init_repo();
    commit_file(&repo, "a.txt", "a\n", "first");
    let root = root(&repo);
    for id in [
        "0123456789abcdef0123456789abcdef01234567",
        "HEAD",
        "main",
        "",
    ] {
        assert_eq!(
            values(
                &repo,
                &root,
                &Context {
                    commit: Some(id.to_string()),
                    ..Context::default()
                }
            )
            .unwrap_err(),
            ContextError::UnknownCommit(id.to_string()),
            "{id}"
        );
    }
}

#[test]
fn a_branch_fills_its_ref_and_tip() {
    let (_dir, repo) = init_repo();
    let id = commit_file(&repo, "a.txt", "a\n", "first");
    repo.branch("feature/x", &repo.find_commit(id).unwrap(), false)
        .unwrap();
    let root = root(&repo);
    let filled = values(
        &repo,
        &root,
        &Context {
            branch: Some("feature/x".to_string()),
            ..Context::default()
        },
    )
    .unwrap();
    assert_eq!(filled.branch.as_deref(), Some("feature/x"));
    assert_eq!(filled.git_ref.as_deref(), Some("refs/heads/feature/x"));
    assert_eq!(filled.commit, Some(id.to_string()));
}

#[test]
fn a_remote_tracking_branch_is_a_branch_too() {
    let (_dir, repo) = init_repo();
    let id = commit_file(&repo, "a.txt", "a\n", "first");
    repo.reference("refs/remotes/origin/main", id, false, "test")
        .unwrap();
    let filled = values(
        &repo,
        &root(&repo),
        &Context {
            branch: Some("origin/main".to_string()),
            ..Context::default()
        },
    )
    .unwrap();
    assert_eq!(filled.git_ref.as_deref(), Some("refs/remotes/origin/main"));
    assert_eq!(filled.commit, Some(id.to_string()));
}

#[test]
fn a_tag_ref_fills_its_commit_but_no_branch() {
    let (_dir, repo) = init_repo();
    let first = commit_file(&repo, "a.txt", "a\n", "first");
    let second = commit_file(&repo, "a.txt", "b\n", "second");
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    repo.tag(
        "v1",
        repo.find_commit(first).unwrap().as_object(),
        &sig,
        "annotated",
        false,
    )
    .unwrap();
    let root = root(&repo);
    let tag = values(
        &repo,
        &root,
        &Context {
            git_ref: Some("refs/tags/v1".to_string()),
            ..Context::default()
        },
    )
    .unwrap();
    assert_eq!(tag.git_ref.as_deref(), Some("refs/tags/v1"));
    assert_eq!(tag.branch, None);
    assert_eq!(tag.commit, Some(first.to_string()));

    // A given commit wins over the ref's tip.
    let both = values(
        &repo,
        &root,
        &Context {
            git_ref: Some("refs/tags/v1".to_string()),
            commit: Some(second.to_string()),
            ..Context::default()
        },
    )
    .unwrap();
    assert_eq!(both.commit, Some(second.to_string()));

    let head_branch = values(
        &repo,
        &root,
        &Context {
            git_ref: Some(repo.head().unwrap().name().unwrap().to_string()),
            ..Context::default()
        },
    )
    .unwrap();
    assert_eq!(
        head_branch.branch.as_deref(),
        repo.head().unwrap().shorthand().ok()
    );
}

#[test]
fn unknown_branches_and_refs_are_refused() {
    let (_dir, repo) = init_repo();
    commit_file(&repo, "a.txt", "a\n", "first");
    let root = root(&repo);
    assert_eq!(
        values(
            &repo,
            &root,
            &Context {
                branch: Some("nope".to_string()),
                ..Context::default()
            }
        )
        .unwrap_err(),
        ContextError::UnknownBranch("nope".to_string())
    );
    assert_eq!(
        values(
            &repo,
            &root,
            &Context {
                git_ref: Some("refs/tags/nope".to_string()),
                ..Context::default()
            }
        )
        .unwrap_err(),
        ContextError::UnknownRef("refs/tags/nope".to_string())
    );
}

#[test]
fn paths_become_absolute_and_path_needs_exactly_one() {
    let (_dir, repo) = init_repo();
    let root = root(&repo);
    let one = values(
        &repo,
        &root,
        &Context {
            paths: vec!["dir/a b.txt".to_string()],
            ..Context::default()
        },
    )
    .unwrap();
    let absolute = root.join("dir/a b.txt").to_string_lossy().into_owned();
    assert_eq!(one.path.as_deref(), Some(absolute.as_str()));
    assert_eq!(one.paths, Some(vec![absolute.clone()]));

    let two = values(
        &repo,
        &root,
        &Context {
            paths: vec!["dir/a b.txt".to_string(), "c.txt".to_string()],
            ..Context::default()
        },
    )
    .unwrap();
    assert_eq!(two.path, None);
    assert_eq!(
        two.paths,
        Some(vec![
            absolute,
            root.join("c.txt").to_string_lossy().into_owned()
        ])
    );

    let none = values(&repo, &root, &Context::default()).unwrap();
    assert_eq!((none.path, none.paths), (None, None));
}

#[test]
fn paths_outside_the_work_tree_are_refused() {
    let (_dir, repo) = init_repo();
    let root = root(&repo);
    for rel in ["../x", "/etc/passwd", "", "a/../../x"] {
        assert_eq!(
            values(
                &repo,
                &root,
                &Context {
                    paths: vec!["ok.txt".to_string(), rel.to_string()],
                    ..Context::default()
                }
            )
            .unwrap_err(),
            ContextError::InvalidPath(rel.to_string()),
            "{rel}"
        );
    }
}

fn no_cancel() -> bool {
    false
}

/// Programs as found on this machine's `PATH`.
fn on_path(name: &str) -> Option<PathBuf> {
    git_core::process::find_program(name)
}

fn expanded(args: &[&str], directory: Option<&str>) -> Expanded {
    Expanded {
        args: args.iter().map(|arg| arg.to_string()).collect(),
        directory: directory.map(str::to_string),
    }
}

#[cfg(unix)]
#[test]
fn runs_the_program_with_its_arguments_in_the_repository_root() {
    let dir = tempfile::TempDir::new().unwrap();
    let outcome = run(
        &expanded(
            &[
                "sh",
                "-c",
                "printf '<%s>' \"$@\"; echo; pwd",
                "sh",
                "a b",
                "$(x) 'q'",
            ],
            None,
        ),
        dir.path(),
        on_path,
        &no_cancel,
        Duration::from_secs(30),
        64 * 1024,
    )
    .unwrap();
    let real = dir.path().canonicalize().unwrap();
    assert_eq!(
        outcome.output,
        format!("<a b><$(x) 'q'>\n{}\n", real.display())
    );
    assert!(outcome.success);
    assert_eq!(outcome.exit_code, Some(0));
    assert!(!outcome.timed_out);
}

#[cfg(unix)]
#[test]
fn a_relative_working_directory_is_below_the_root() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let outcome = run(
        &expanded(&["pwd"], Some("sub")),
        dir.path(),
        on_path,
        &no_cancel,
        Duration::from_secs(30),
        1024,
    )
    .unwrap();
    assert_eq!(
        outcome.output.trim_end(),
        dir.path()
            .join("sub")
            .canonicalize()
            .unwrap()
            .display()
            .to_string()
    );

    let missing = run(
        &expanded(&["pwd"], Some("nope")),
        dir.path(),
        on_path,
        &no_cancel,
        Duration::from_secs(30),
        1024,
    )
    .unwrap_err();
    assert!(matches!(missing, RunError::NoDirectory(_)), "{missing:?}");
}

#[cfg(unix)]
#[test]
fn a_failing_command_reports_its_exit_code_and_output() {
    let dir = tempfile::TempDir::new().unwrap();
    let outcome = run(
        &expanded(&["sh", "-c", "echo out; echo bad >&2; exit 3"], None),
        dir.path(),
        on_path,
        &no_cancel,
        Duration::from_secs(30),
        1024,
    )
    .unwrap();
    assert!(!outcome.success);
    assert_eq!(outcome.exit_code, Some(3));
    assert_eq!(outcome.output, "out\nbad\n");
}

#[test]
fn a_missing_program_is_an_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let error = run(
        &expanded(&["browsitory-surely-missing-program", "x"], None),
        dir.path(),
        on_path,
        &no_cancel,
        Duration::from_secs(30),
        1024,
    )
    .unwrap_err();
    assert!(
        matches!(&error, RunError::NotFound(name) if name == "browsitory-surely-missing-program"),
        "{error:?}"
    );
    let empty = run(
        &expanded(&[], None),
        dir.path(),
        on_path,
        &no_cancel,
        Duration::from_secs(30),
        1024,
    )
    .unwrap_err();
    assert!(matches!(empty, RunError::NoProgram), "{empty:?}");
}

#[cfg(unix)]
#[test]
fn a_relative_program_is_never_run_even_from_the_repository() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    let script = dir.path().join("tool.sh");
    std::fs::write(&script, "#!/bin/sh\necho ran\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    for name in ["./tool.sh", "tool.sh"] {
        let error = run(
            &expanded(&[name], None),
            dir.path(),
            on_path,
            &no_cancel,
            Duration::from_secs(30),
            1024,
        )
        .unwrap_err();
        assert!(
            matches!(&error, RunError::NotFound(n) if n == name),
            "{error:?}"
        );
    }
    // Its absolute path (`${repo}/tool.sh` expands to one) runs.
    let absolute = script.to_string_lossy().into_owned();
    let outcome = run(
        &expanded(&[&absolute], None),
        dir.path(),
        on_path,
        &no_cancel,
        Duration::from_secs(30),
        1024,
    )
    .unwrap();
    assert_eq!(outcome.output, "ran\n");
}

#[cfg(unix)]
#[test]
fn cancel_stops_the_command_and_what_it_started() {
    let dir = tempfile::TempDir::new().unwrap();
    let marker = dir.path().join("still-running");
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let cancelled = Arc::clone(&cancelled);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            cancelled.store(true, Ordering::SeqCst);
        });
    }
    let started = Instant::now();
    let script = format!("(sleep 2; touch '{}') & sleep 30", marker.display());
    let error = run(
        &expanded(&["sh", "-c", &script], None),
        dir.path(),
        on_path,
        &|| cancelled.load(Ordering::SeqCst),
        Duration::from_secs(60),
        1024,
    )
    .unwrap_err();
    assert!(matches!(error, RunError::Cancelled), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        !marker.exists(),
        "a child of the command outlived the cancel"
    );
}

#[cfg(unix)]
#[test]
fn the_time_limit_stops_the_command() {
    let dir = tempfile::TempDir::new().unwrap();
    let started = Instant::now();
    let outcome = run(
        &expanded(&["sh", "-c", "echo begun; sleep 30"], None),
        dir.path(),
        on_path,
        &no_cancel,
        Duration::from_millis(500),
        1024,
    )
    .unwrap();
    assert!(outcome.timed_out);
    assert!(!outcome.success);
    assert_eq!(outcome.exit_code, None);
    assert_eq!(outcome.output, "begun\n");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn output_keeps_only_its_end() {
    let dir = tempfile::TempDir::new().unwrap();
    let outcome = run(
        &expanded(
            &[
                "sh",
                "-c",
                "i=0; while [ $i -lt 2000 ]; do echo line$i; i=$((i+1)); done",
            ],
            None,
        ),
        dir.path(),
        on_path,
        &no_cancel,
        Duration::from_secs(30),
        100,
    )
    .unwrap();
    assert!(outcome.success);
    assert!(
        outcome.output.starts_with("[… earlier output omitted …]\n"),
        "{}",
        outcome.output
    );
    assert!(outcome.output.ends_with("line1999\n"), "{}", outcome.output);
    assert!(outcome.output.len() < 200, "{}", outcome.output.len());
}

#[cfg(unix)]
#[test]
fn variables_reach_the_program_as_single_arguments() {
    // Parse, fill and run together: a branch name with shell syntax stays one argument.
    let (_dir, repo) = init_repo();
    let id = commit_file(&repo, "a b.txt", "a\n", "first");
    repo.branch("odd$x", &repo.find_commit(id).unwrap(), false)
        .unwrap();
    let root = root(&repo);
    let template = parse(
        "sh -c 'printf \"<%s>\" \"$@\"' sh ${branch} ${shortCommit} ${paths}",
        None,
        CommandStyle::Posix,
    )
    .unwrap();
    let filled = values(
        &repo,
        &root,
        &Context {
            branch: Some("odd$x".to_string()),
            paths: vec!["a b.txt".to_string()],
            ..Context::default()
        },
    )
    .unwrap();
    let outcome = run(
        &template.expand(&filled).unwrap(),
        &root,
        on_path,
        &no_cancel,
        Duration::from_secs(30),
        1024,
    )
    .unwrap();
    assert_eq!(
        outcome.output,
        format!(
            "<odd$x><{}><{}>",
            &id.to_string()[..7],
            root.join("a b.txt").display()
        )
    );
}

#[test]
fn windows_lookup_tries_pathext_and_skips_relative_entries() {
    // The rules `run` gets through `find_program` on Windows: `PATHEXT` extensions, and never a
    // relative `PATH` entry (so not the repository the app happens to run in).
    let dir = tempfile::TempDir::new().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(bin.join("tool.CMD"), "@echo off\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(bin.join("tool.CMD"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    let exts = vec![".EXE".to_string(), ".CMD".to_string()];
    assert_eq!(
        find_program_in("tool", &[PathBuf::from("bin"), bin.clone()], &exts),
        Some(bin.join("tool.CMD"))
    );
    assert_eq!(
        find_program_in("tool", &[PathBuf::from("bin")], &exts),
        None
    );
}
