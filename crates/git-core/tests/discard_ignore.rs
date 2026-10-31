mod common;

use git_core::stage::{discard_all_unstaged, discard_file, ignore_path, stage_file, StageError};

fn read(dir: &std::path::Path, p: &str) -> String {
    std::fs::read_to_string(dir.join(p))
        .unwrap()
        .replace("\r\n", "\n")
}

#[test]
fn discard_file_restores_a_modified_tracked_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "v1\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "v2\n");
    discard_file(&repo, "f.txt").unwrap();
    assert_eq!(read(dir.path(), "f.txt"), "v1\n");
}

#[test]
fn discard_file_keeps_staged_changes() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "v1\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "v2\n");
    stage_file(&repo, "f.txt").unwrap();
    common::write_file(dir.path(), "f.txt", "v3\n");

    discard_file(&repo, "f.txt").unwrap();

    assert_eq!(read(dir.path(), "f.txt"), "v2\n");
    let statuses = git_core::status::status(&repo).unwrap();
    assert!(statuses.iter().any(|e| e.path == "f.txt" && e.staged));
    assert!(!statuses.iter().any(|e| e.path == "f.txt" && !e.staged));
}

#[test]
fn discard_file_restores_a_deleted_tracked_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "v1\n");
    common::commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("f.txt")).unwrap();
    discard_file(&repo, "f.txt").unwrap();
    assert_eq!(read(dir.path(), "f.txt"), "v1\n");
}

#[test]
fn discard_file_deletes_an_untracked_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "seed.txt", "s\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "nested/new.txt", "n\n");
    discard_file(&repo, "nested/new.txt").unwrap();
    assert!(!dir.path().join("nested/new.txt").exists());
}

#[test]
fn discard_file_rejects_paths_outside_the_repo() {
    let (_dir, repo) = common::init_repo();
    assert!(matches!(
        discard_file(&repo, "../escape.txt"),
        Err(StageError::InvalidPath)
    ));
    assert!(matches!(
        discard_file(&repo, "/etc/passwd"),
        Err(StageError::InvalidPath)
    ));
}

#[test]
fn discard_all_unstaged_resets_tracked_deletes_untracked_and_keeps_the_index() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a1\n");
    common::write_file(dir.path(), "b.txt", "b1\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "a.txt", "a2\n");
    common::write_file(dir.path(), "b.txt", "b2\n");
    stage_file(&repo, "b.txt").unwrap();
    common::write_file(dir.path(), "u.txt", "u\n");

    discard_all_unstaged(&repo, &unstaged_paths(&repo)).unwrap();

    assert_eq!(read(dir.path(), "a.txt"), "a1\n");
    assert_eq!(read(dir.path(), "b.txt"), "b2\n");
    assert!(!dir.path().join("u.txt").exists());
    let statuses = git_core::status::status(&repo).unwrap();
    assert!(statuses.iter().all(|e| e.staged), "{statuses:?}");
}

/// The list the "Discard all" dialog shows: unstaged, non-conflicted entries.
fn unstaged_paths(repo: &git2::Repository) -> Vec<String> {
    git_core::status::status(repo)
        .unwrap()
        .into_iter()
        .filter(|e| !e.staged && e.kind != git_core::status::StatusKind::Conflicted)
        .map(|e| e.path)
        .collect()
}

#[test]
fn discard_all_skips_a_nested_repo_discards_the_rest_and_names_what_it_skipped() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a1\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "a.txt", "a2\n");
    common::write_file(dir.path(), "u.txt", "u\n");
    let _nested = git2::Repository::init(dir.path().join("nested")).unwrap();
    common::write_file(dir.path(), "nested/inner.txt", "i\n");
    let paths = unstaged_paths(&repo);
    assert!(paths.contains(&"nested/".to_string()), "{paths:?}");

    let result = discard_all_unstaged(&repo, &paths);

    match result {
        Err(e @ StageError::DiscardIncomplete { .. }) => {
            let text = e.to_string();
            assert!(
                text.starts_with("Couldn't discard 1 file: nested/"),
                "{text}"
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(read(dir.path(), "a.txt"), "a1\n");
    assert!(!dir.path().join("u.txt").exists());
    assert!(dir.path().join("nested/inner.txt").exists());
    assert!(dir.path().join("nested/.git").exists());
}

#[test]
fn discard_all_only_touches_the_listed_paths() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a1\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "u.txt", "u\n");
    let paths = unstaged_paths(&repo);
    // Created after the dialog listed what it would discard.
    common::write_file(dir.path(), "late.txt", "late\n");
    common::write_file(dir.path(), "a.txt", "a2\n");

    discard_all_unstaged(&repo, &paths).unwrap();

    assert!(!dir.path().join("u.txt").exists());
    assert_eq!(read(dir.path(), "late.txt"), "late\n");
    assert_eq!(read(dir.path(), "a.txt"), "a2\n");
}

#[test]
fn discard_all_still_skips_conflicted_entries() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "shared.txt", "one\ntwo\n");
    common::commit_all(&repo, "base");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    common::write_file(dir.path(), "shared.txt", "one\nfeature\n");
    common::commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    common::write_file(dir.path(), "shared.txt", "one\nmain\n");
    common::commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    let conflicted = read(dir.path(), "shared.txt");
    assert!(conflicted.contains("<<<<<<<"), "{conflicted}");

    discard_all_unstaged(&repo, &["shared.txt".to_string()]).unwrap();

    assert_eq!(read(dir.path(), "shared.txt"), conflicted);
}

#[test]
fn ignore_path_creates_gitignore() {
    let (dir, repo) = common::init_repo();
    ignore_path(&repo, "/build.log").unwrap();
    assert_eq!(read(dir.path(), ".gitignore"), "/build.log\n");
}

#[test]
fn ignore_path_appends_and_fixes_a_missing_trailing_newline() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), ".gitignore", "target");
    ignore_path(&repo, "*.log").unwrap();
    assert_eq!(read(dir.path(), ".gitignore"), "target\n*.log\n");
}

#[test]
fn ignore_path_skips_duplicates() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), ".gitignore", "*.log\n");
    ignore_path(&repo, "*.log").unwrap();
    assert_eq!(read(dir.path(), ".gitignore"), "*.log\n");
}

#[test]
fn ignore_path_rejects_empty_and_multiline_patterns() {
    let (_dir, repo) = common::init_repo();
    assert!(matches!(
        ignore_path(&repo, "  "),
        Err(StageError::InvalidPattern)
    ));
    assert!(matches!(
        ignore_path(&repo, "a\nb"),
        Err(StageError::InvalidPattern)
    ));
}

#[test]
fn ignore_path_keeps_a_non_utf8_gitignore_untouched() {
    let (dir, repo) = common::init_repo();
    let bytes: &[u8] = b"caf\xe9\n";
    std::fs::write(dir.path().join(".gitignore"), bytes).unwrap();
    assert!(ignore_path(&repo, "*.log").is_err());
    assert_eq!(std::fs::read(dir.path().join(".gitignore")).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn discard_file_does_not_delete_through_a_symlinked_directory() {
    let (dir, repo) = common::init_repo();
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("victim.txt"), "keep\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
    assert!(matches!(
        discard_file(&repo, "link/victim.txt"),
        Err(StageError::BeyondSymlink(_))
    ));
    assert!(outside.path().join("victim.txt").exists());
}

// --- Final-review fixes: symlinked parents, symlinked .gitignore, pattern validation ---

/// Tracked `d/f.txt`, then `d` replaced by a symlink to an outside dir holding its own `f.txt`.
#[cfg(unix)]
fn setup_symlinked_parent() -> (tempfile::TempDir, git2::Repository, tempfile::TempDir) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "d/f.txt", "inside\n");
    common::commit_all(&repo, "base");
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("f.txt"), "outside\n").unwrap();
    std::fs::remove_dir_all(dir.path().join("d")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("d")).unwrap();
    (dir, repo, outside)
}

#[cfg(unix)]
#[test]
fn discard_file_does_not_write_a_tracked_file_through_a_symlinked_parent() {
    let (_dir, repo, outside) = setup_symlinked_parent();
    let result = discard_file(&repo, "d/f.txt");
    assert!(
        matches!(result, Err(StageError::BeyondSymlink(_))),
        "{result:?}"
    );
    assert_eq!(read(outside.path(), "f.txt"), "outside\n");
}

#[cfg(unix)]
#[test]
fn ignore_path_refuses_a_symlinked_gitignore() {
    let (dir, repo) = common::init_repo();
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("target"), "keep\n").unwrap();
    std::os::unix::fs::symlink(outside.path().join("target"), dir.path().join(".gitignore"))
        .unwrap();
    let result = ignore_path(&repo, "*.log");
    assert!(matches!(result, Err(StageError::InvalidPath)), "{result:?}");
    assert_eq!(read(outside.path(), "target"), "keep\n");
}

#[test]
fn ignore_path_rejects_surrounding_whitespace_instead_of_trimming() {
    let (dir, repo) = common::init_repo();
    for bad in ["", " *.log", "*.log ", "\t*.log", "a\rb"] {
        let result = ignore_path(&repo, bad);
        assert!(
            matches!(result, Err(StageError::InvalidPattern)),
            "{bad:?}: {result:?}"
        );
    }
    assert!(!dir.path().join(".gitignore").exists());
}

#[test]
fn discard_file_on_an_untracked_folder_explains_instead_of_failing_raw() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "seed.txt", "s\n");
    common::commit_all(&repo, "base");
    let _nested = git2::Repository::init(dir.path().join("nested")).unwrap();
    common::write_file(dir.path(), "nested/inner.txt", "i\n");
    let result = discard_file(&repo, "nested/");
    match result {
        Err(e @ StageError::IsFolder(_)) => assert_eq!(
            e.to_string(),
            "nested/ is a folder (for example a nested repository) and wasn't discarded."
        ),
        other => panic!("{other:?}"),
    }
    assert!(dir.path().join("nested/inner.txt").exists());
    assert!(dir.path().join("nested/.git").exists());
}

#[test]
fn discard_all_caps_the_names_it_lists() {
    let (dir, repo) = common::init_repo();
    let paths: Vec<String> = (1..=5)
        .map(|i| {
            common::write_file(dir.path(), &format!("d{i}/x.txt"), "x\n");
            format!("d{i}/")
        })
        .collect();
    let err = discard_all_unstaged(&repo, &paths).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Couldn't discard 5 files: d1/, d2/, d3/ and 2 more. \
         Folders (for example nested repositories) are never discarded."
    );
}

#[cfg(unix)]
fn typechanged_tc() -> (tempfile::TempDir, git2::Repository, tempfile::TempDir) {
    let out = common::outside_dir();
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "tc", "plain\n");
    common::write_file(dir.path(), "d/f", "inner\n");
    common::commit_all(&repo, "base");
    common::replace_with_link(dir.path(), "tc", &out.path().join("target.txt"));
    (dir, repo, out)
}

#[test]
#[cfg(unix)]
fn discard_file_of_a_typechanged_symlink_restores_the_file_and_leaves_the_target_alone() {
    let (dir, repo, out) = typechanged_tc();
    discard_file(&repo, "tc").unwrap();
    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "tc"));
    assert_eq!(read(dir.path(), "tc"), "plain\n");
}

#[test]
#[cfg(unix)]
fn discard_all_unstaged_of_a_typechanged_symlink_does_not_write_through() {
    let (dir, repo, out) = typechanged_tc();
    discard_all_unstaged(&repo, &["tc".into()]).unwrap();
    common::assert_outside_untouched(out.path());
    assert_eq!(read(dir.path(), "tc"), "plain\n");
}

#[test]
#[cfg(unix)]
fn discard_file_still_refuses_a_path_beyond_a_symlinked_directory() {
    let (dir, repo, out) = typechanged_tc();
    common::replace_with_link(dir.path(), "d", out.path());
    assert!(matches!(
        discard_file(&repo, "d/f"),
        Err(StageError::BeyondSymlink(_))
    ));
    common::assert_outside_untouched(out.path());
}

/// Runs `f` on another thread and fails (instead of hanging) after 5 s.
#[cfg(unix)]
fn within_5s<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("blocked")
}

/// #149: `.gitignore` is read and written through the work tree walker, which never opens a
/// FIFO in blocking mode or writes into one.
#[cfg(unix)]
#[test]
fn ignore_path_does_not_block_on_a_fifo_gitignore() {
    let (dir, _repo) = common::init_repo();
    let fifo = std::ffi::CString::new(dir.path().join(".gitignore").to_str().unwrap()).unwrap();
    // SAFETY: `fifo` is a valid NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let path = dir.path().to_path_buf();
    let result = within_5s(move || {
        let repo = git2::Repository::open(path).unwrap();
        ignore_path(&repo, "*.log").map_err(|e| e.to_string())
    });
    assert!(result.is_err(), "{result:?}");
}

/// Stress test for #149: a thread swaps `rd` between a real folder and a link to an outside
/// folder while `discard_file` deletes the untracked `rd/f`. Run manually:
/// `cargo test -p git-core --test discard_ignore swap_race -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "stress: run manually"]
fn discard_swap_race_never_deletes_outside() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "base");
    let outside = tempfile::TempDir::new().unwrap();
    let rd = dir.path().join("rd");
    let real = dir.path().join("rd-real");
    std::fs::create_dir(&real).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let swapper = {
        let (stop, rd, real, out) = (
            stop.clone(),
            rd.clone(),
            real.clone(),
            outside.path().to_path_buf(),
        );
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let _ = std::fs::rename(&real, &rd);
                let _ = std::fs::rename(&rd, &real);
                let _ = std::os::unix::fs::symlink(&out, &rd);
                let _ = std::fs::remove_file(&rd);
            }
        })
    };
    let mut leaks = 0;
    for _ in 0..20_000 {
        std::fs::write(outside.path().join("f"), "OUTSIDE\n").unwrap();
        let _ = std::fs::write(real.join("f"), "inside\n");
        let _ = discard_file(&repo, "rd/f");
        if !outside.path().join("f").exists() {
            leaks += 1;
        }
    }
    stop.store(true, Ordering::Relaxed);
    swapper.join().unwrap();
    eprintln!("{leaks} outside files deleted");
    assert_eq!(leaks, 0);
}

/// Stress test for #149: a thread swaps `.gitignore` between a file and a link to an outside
/// file while `ignore_path` appends to it. Run manually (see above).
#[cfg(unix)]
#[test]
#[ignore = "stress: run manually"]
fn ignore_swap_race_never_writes_outside() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let (dir, repo) = common::init_repo();
    let outside = tempfile::TempDir::new().unwrap();
    let target = outside.path().join("target");
    std::fs::write(&target, "OUTSIDE\n").unwrap();
    let gitignore = dir.path().join(".gitignore");
    let stop = Arc::new(AtomicBool::new(false));
    let swapper = {
        let (stop, gitignore, target) = (stop.clone(), gitignore.clone(), target.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let _ = std::fs::remove_file(&gitignore);
                let _ = std::os::unix::fs::symlink(&target, &gitignore);
                let _ = std::fs::remove_file(&gitignore);
                let _ = std::fs::write(&gitignore, "x\n");
            }
        })
    };
    for i in 0..20_000 {
        let _ = ignore_path(&repo, &format!("p{i}"));
    }
    stop.store(true, Ordering::Relaxed);
    swapper.join().unwrap();
    let outside_text = std::fs::read_to_string(&target).unwrap();
    eprintln!("outside target = {:?}", outside_text.lines().count());
    assert_eq!(outside_text, "OUTSIDE\n");
}

/// HEAD has `d/f`; the folder `d` was replaced by a regular (untracked) file `d` (#154).
fn folder_replaced_by_file() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "d/f", "f\n");
    common::commit_all(&repo, "base");
    std::fs::remove_dir_all(dir.path().join("d")).unwrap();
    std::fs::write(dir.path().join("d"), "now a file\n").unwrap();
    (dir, repo)
}

fn index_paths(repo: &git2::Repository) -> Vec<String> {
    repo.index()
        .unwrap()
        .iter()
        .map(|e| String::from_utf8(e.path).unwrap())
        .collect()
}

#[test]
fn discarding_a_file_whose_folder_is_now_a_file_names_the_file_in_the_way() {
    let (dir, repo) = folder_replaced_by_file();

    let err = discard_file(&repo, "d/f").unwrap_err();

    assert_eq!(
        err.to_string(),
        "Couldn't discard 'd/f': 'd' is now a file, not a folder. Discard or move 'd' first."
    );
    assert_eq!(read(dir.path(), "d"), "now a file\n");
    assert_eq!(index_paths(&repo), vec!["d/f".to_string()]);
}

#[test]
fn discard_all_with_the_file_and_the_folder_restores_the_folder() {
    let (dir, repo) = folder_replaced_by_file();

    discard_all_unstaged(&repo, &["d/f".into(), "d".into()]).unwrap();

    assert_eq!(read(dir.path(), "d/f"), "f\n");
}

#[test]
fn discard_all_with_only_the_folders_file_names_the_reason() {
    let (dir, repo) = folder_replaced_by_file();

    let err = discard_all_unstaged(&repo, &["d/f".into()]).unwrap_err();

    assert_eq!(
        err.to_string(),
        "Couldn't discard 1 file: d/f. 'd' is now a file, not a folder."
    );
    assert_eq!(read(dir.path(), "d"), "now a file\n");
}

#[test]
fn discarding_a_stale_untracked_path_below_such_a_file_is_a_no_op() {
    let (dir, repo) = folder_replaced_by_file();

    discard_file(&repo, "d/u").unwrap();

    assert_eq!(read(dir.path(), "d"), "now a file\n");
}

#[cfg(unix)]
#[test]
fn discarding_a_file_whose_folder_is_now_a_link_still_says_beyond_a_symbolic_link() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "d/f", "f\n");
    common::commit_all(&repo, "base");
    let outside = common::outside_dir();
    common::replace_with_link(dir.path(), "d", outside.path());

    let err = discard_file(&repo, "d/f").unwrap_err();

    assert!(
        matches!(err, StageError::BeyondSymlink(ref p) if p == "d/f"),
        "{err:?}"
    );
    common::assert_outside_untouched(outside.path());
}
