//! #1162: a process killed during a status read leaves its scratch folder
//! (`browsitory-index-refresh-*`, see `index_refresh`) in the git folder. A later status read
//! removes such a folder once no running process owns it, and nothing else.

mod common;

use std::path::{Path, PathBuf};
use std::process::Stdio;

use common::{commit_all, init_repo, write_file};
use git_core::status::status_refreshing_index;

const PREFIX: &str = "browsitory-index-refresh-";

fn committed_repo() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "first");
    (dir, repo)
}

/// A scratch folder as a killed scan leaves it: the index copy, maybe its lock.
fn leftover(git_dir: &Path, name: &str, files: &[&str]) -> PathBuf {
    let folder = git_dir.join(name);
    std::fs::create_dir(&folder).unwrap();
    for file in files {
        std::fs::write(folder.join(file), "DIRC leftover").unwrap();
    }
    folder
}

/// The id of a process that ran and has been reaped.
fn ended_pid(dir: &Path) -> u32 {
    let mut child = test_support::git_command(dir)
        .arg("--version")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

#[test]
fn a_status_read_removes_the_scratch_folder_of_a_process_that_ended() {
    let (dir, repo) = committed_repo();
    let name = format!("{PREFIX}{}-a1B2c3", ended_pid(dir.path()));
    let folder = leftover(repo.path(), &name, &["index", "index.lock"]);

    status_refreshing_index(&repo).unwrap();

    assert!(!folder.exists(), "{name} is still there");
}

#[test]
fn a_status_read_keeps_the_scratch_folders_of_running_processes() {
    let (dir, repo) = committed_repo();
    // Waits for its input until it is killed.
    let mut running = test_support::git_command(dir.path())
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let theirs = leftover(
        repo.path(),
        &format!("{PREFIX}{}-a1B2c3", running.id()),
        &["index"],
    );
    let ours = leftover(
        repo.path(),
        &format!("{PREFIX}{}-d4E5f6", std::process::id()),
        &["index"],
    );

    status_refreshing_index(&repo).unwrap();

    running.kill().unwrap();
    running.wait().unwrap();
    assert!(theirs.exists(), "a running process's folder was removed");
    assert!(ours.exists(), "this process's folder was removed");
}

#[test]
fn a_status_read_keeps_what_isnt_a_scratch_folder() {
    let (dir, repo) = committed_repo();
    let pid = ended_pid(dir.path());
    // Something else in it than the index copy and its lock.
    let other = leftover(
        repo.path(),
        &format!("{PREFIX}{pid}-a1B2c3"),
        &["index", "notes.txt"],
    );
    // A file, not a folder.
    let file = repo.path().join(format!("{PREFIX}{pid}-d4E5f6"));
    std::fs::write(&file, "mine").unwrap();
    // Not the prefix.
    let unrelated = leftover(repo.path(), &format!("browsitory-{pid}-g7H8i9"), &["index"]);

    status_refreshing_index(&repo).unwrap();

    assert!(other.join("notes.txt").exists());
    assert!(file.exists());
    assert!(unrelated.exists());
}

/// Folders without a process id, as versions before #1162 named them: removed once they are an
/// hour old, which no scan takes.
#[cfg(unix)]
#[test]
fn a_status_read_removes_an_old_scratch_folder_without_a_process_id() {
    use std::time::{Duration, SystemTime};

    let (_dir, repo) = committed_repo();
    let old = leftover(repo.path(), &format!("{PREFIX}a1B2c3"), &["index"]);
    let recent = leftover(repo.path(), &format!("{PREFIX}d4E5f6"), &["index"]);
    let two_hours_ago = SystemTime::now() - Duration::from_secs(2 * 60 * 60);
    std::fs::File::open(&old)
        .unwrap()
        .set_modified(two_hours_ago)
        .unwrap();

    status_refreshing_index(&repo).unwrap();

    assert!(!old.exists(), "the old folder is still there");
    assert!(
        recent.exists(),
        "a folder a running scan may use was removed"
    );
}

#[test]
fn a_status_read_leaves_no_scratch_folder_of_its_own() {
    let (_dir, repo) = committed_repo();

    status_refreshing_index(&repo).unwrap();

    let names: Vec<String> = std::fs::read_dir(repo.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(PREFIX))
        .collect();
    assert!(names.is_empty(), "{names:?}");
}
