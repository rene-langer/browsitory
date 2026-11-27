//! Writers that change `config.toml` at the same time (#1127): the desktop app's commands run on
//! a thread pool, each VS Code sidecar request has its own thread, and the app and every VS Code
//! window share the file. No writer may undo another's change.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};

use config::{
    add_recent_repo_at, get_last_seen_version_at, list_recent_repos_at, list_repo_aliases_at,
    set_last_seen_version_at, set_repo_alias_at,
};

/// A tab restore: five tabs open at once, each moving its repository to the front of the recent
/// list, while the app saves the version it last showed the release notes of.
#[test]
fn threads_writing_at_once_lose_no_change() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut lost = Vec::new();
    for round in 0..50 {
        let file = dir.path().join(format!("config-{round}.toml"));
        set_last_seen_version_at(&file, "0.17.0").unwrap();
        let barrier = Arc::new(Barrier::new(6));
        let mut threads = Vec::new();
        for repo in 0..5 {
            let (file, barrier) = (file.clone(), Arc::clone(&barrier));
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                add_recent_repo_at(&file, &PathBuf::from(format!("/repos/{repo}"))).unwrap();
            }));
        }
        let (version_file, version_barrier) = (file.clone(), Arc::clone(&barrier));
        threads.push(std::thread::spawn(move || {
            version_barrier.wait();
            set_last_seen_version_at(&version_file, "0.18.0").unwrap();
        }));
        for thread in threads {
            thread.join().unwrap();
        }
        let version = get_last_seen_version_at(&file).unwrap();
        let recent = list_recent_repos_at(&file).unwrap().len();
        if version.as_deref() != Some("0.18.0") || recent != 5 {
            lost.push(format!(
                "round {round}: version {version:?}, {recent} recent"
            ));
        }
    }
    assert!(lost.is_empty(), "lost updates:\n{}", lost.join("\n"));
}

/// The variable that makes this test binary a writer child of
/// `processes_writing_at_once_lose_no_change`: `<config file>|<writer number>`.
const CHILD_ENV: &str = "BROWSITORY_CONFIG_TEST_WRITER";
const PROCESSES: usize = 4;
const WRITES_PER_PROCESS: usize = 25;

fn alias_path(writer: usize, write: usize) -> PathBuf {
    PathBuf::from(format!("/repos/{writer}/{write}"))
}

/// Run by the parent test in a child process; does nothing in a normal test run.
#[test]
fn writer_child() {
    let Ok(spec) = std::env::var(CHILD_ENV) else {
        return;
    };
    let (file, writer) = spec.rsplit_once('|').unwrap();
    let writer: usize = writer.parse().unwrap();
    for write in 0..WRITES_PER_PROCESS {
        set_repo_alias_at(Path::new(file), &alias_path(writer, write), Some("tab")).unwrap();
    }
}

/// The desktop app and VS Code windows: separate processes, one file.
#[test]
fn processes_writing_at_once_lose_no_change() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    let exe = std::env::current_exe().unwrap();
    let children: Vec<_> = (0..PROCESSES)
        .map(|writer| {
            std::process::Command::new(&exe)
                .args(["--exact", "writer_child", "--test-threads", "1"])
                .env(CHILD_ENV, format!("{}|{writer}", file.display()))
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for (writer, child) in children.into_iter().enumerate() {
        let output = child.wait_with_output().unwrap();
        // Its panic message (libtest prints it on stdout) tells a lock timeout or a sharing
        // violation from a lost write (#1212).
        assert!(
            output.status.success(),
            "writer process {writer} failed ({}):\n{}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let mut stored: Vec<PathBuf> = list_repo_aliases_at(&file)
        .unwrap()
        .into_iter()
        .map(|alias| alias.repo_path)
        .collect();
    stored.sort();
    let mut expected: Vec<PathBuf> = (0..PROCESSES)
        .flat_map(|writer| (0..WRITES_PER_PROCESS).map(move |write| alias_path(writer, write)))
        .collect();
    expected.sort();
    assert_eq!(
        stored.len(),
        expected.len(),
        "{} of {} tab names lost",
        expected.len() - stored.len(),
        expected.len()
    );
    assert_eq!(stored, expected);
}

/// A `config.toml` that is a link into a dotfiles folder stays a link, and the change lands in
/// the file it points to.
#[cfg(unix)]
#[test]
fn a_symlinked_config_file_stays_a_link() {
    let dir = tempfile::TempDir::new().unwrap();
    let dotfiles = dir.path().join("dotfiles");
    std::fs::create_dir(&dotfiles).unwrap();
    let target = dotfiles.join("browsitory.toml");
    std::fs::write(&target, "last_seen_version = \"0.17.0\"\n").unwrap();
    let link = dir.path().join("config.toml");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    set_last_seen_version_at(&link, "0.18.0").unwrap();

    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        get_last_seen_version_at(&target).unwrap().as_deref(),
        Some("0.18.0")
    );
}
