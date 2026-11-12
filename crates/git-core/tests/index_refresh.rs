//! #841: a status read writes refreshed stat data back to the index, as `git status` does, so a
//! file whose time stamps changed but whose content didn't is hashed once, not on every read.
//! Opportunistic: a held `index.lock` skips the write, and a read never fails because of it.

mod common;

use std::path::Path;
use std::time::{Duration, SystemTime};

use common::{commit_all, git_ok, init_repo, write_file};
use git2::Repository;
use git_core::change_stamp::{change_stamp, status_with_stamp};
use git_core::status::{status, status_refreshing_index};

/// A repository with `a.txt` and `dir/b.txt` committed.
fn committed_repo() -> (tempfile::TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    write_file(dir.path(), "dir/b.txt", "two\n");
    commit_all(&repo, "first");
    (dir, repo)
}

/// Sets `relative`'s modification time to a day ago, content unchanged: a `touch` as far as the
/// index's stat data goes, and old enough not to count as racily clean.
fn backdate(workdir: &Path, relative: &str) -> SystemTime {
    let when = SystemTime::now() - Duration::from_secs(24 * 60 * 60);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(workdir.join(relative))
        .unwrap();
    file.set_modified(when).unwrap();
    when
}

/// The modification time (whole seconds) the index on disk records for `path`.
fn indexed_mtime(repo: &Repository, path: &str) -> i32 {
    let index = git2::Index::open(&repo.path().join("index")).unwrap();
    index.get_path(Path::new(path), 0).unwrap().mtime.seconds()
}

fn seconds(time: SystemTime) -> i32 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i32
}

/// Path, id, mode and stage of each index entry: what a stat refresh must not change.
fn index_content(repo: &Repository) -> Vec<(Vec<u8>, git2::Oid, u32, u16)> {
    let index = git2::Index::open(&repo.path().join("index")).unwrap();
    index
        .iter()
        .map(|entry| (entry.path, entry.id, entry.mode, entry.flags))
        .collect()
}

#[test]
fn a_status_read_records_the_stat_data_of_a_touched_file() {
    let (dir, repo) = committed_repo();
    let when = backdate(dir.path(), "a.txt");
    assert_ne!(indexed_mtime(&repo, "a.txt"), seconds(when));

    let entries = status_refreshing_index(&repo).unwrap();

    assert!(entries.is_empty(), "{entries:?}");
    assert_eq!(indexed_mtime(&repo, "a.txt"), seconds(when));
}

#[test]
fn the_combined_read_records_the_stat_data_too() {
    let (dir, repo) = committed_repo();
    let when = backdate(dir.path(), "dir/b.txt");

    let read = status_with_stamp(&repo).unwrap();

    assert!(read.status.is_empty(), "{:?}", read.status);
    assert_eq!(indexed_mtime(&repo, "dir/b.txt"), seconds(when));
}

/// After one refresh the next read trusts the stat data: it no longer opens the file. Shown with
/// a file it can't open, which it would have to hash otherwise.
#[cfg(unix)]
#[test]
fn after_a_refresh_the_next_read_does_not_hash_the_touched_file_again() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        assert!(std::env::var_os("CI").is_none(), "running as root on CI");
        eprintln!("SKIP: running as root, who reads unreadable files");
        return;
    }
    let (dir, repo) = committed_repo();
    // Taking the read permission away changes the file's ctime, which would make it stat-dirty
    // again on its own; with `core.trustctime` off only the times a refresh records count.
    repo.config()
        .unwrap()
        .set_bool("core.trustctime", false)
        .unwrap();
    let repo = Repository::open(dir.path()).unwrap();
    backdate(dir.path(), "a.txt");
    status_refreshing_index(&repo).unwrap();
    let file = dir.path().join("a.txt");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();

    let entries = status(&repo);

    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(entries.unwrap(), vec![]);
}

#[test]
fn a_held_index_lock_leaves_the_index_alone_and_the_read_still_succeeds() {
    let (dir, repo) = committed_repo();
    write_file(dir.path(), "new.txt", "new\n");
    backdate(dir.path(), "a.txt");
    let index_path = repo.path().join("index");
    let lock_path = repo.path().join("index.lock");
    let before = std::fs::read(&index_path).unwrap();
    std::fs::write(&lock_path, "held by someone else").unwrap();

    let entries = status_refreshing_index(&repo).unwrap();
    let read = status_with_stamp(&repo).unwrap();

    assert_eq!(entries, status(&repo).unwrap());
    assert_eq!(read.status, entries);
    assert_eq!(std::fs::read(&index_path).unwrap(), before);
    assert_eq!(
        std::fs::read_to_string(&lock_path).unwrap(),
        "held by someone else"
    );
}

#[test]
fn a_refresh_keeps_the_index_content_and_leaves_no_files_behind() {
    let (dir, repo) = committed_repo();
    write_file(dir.path(), "a.txt", "changed\n");
    write_file(dir.path(), "staged.txt", "staged\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("staged.txt")).unwrap();
    index.write().unwrap();
    write_file(dir.path(), "untracked.txt", "untracked\n");
    backdate(dir.path(), "dir/b.txt");
    let content = index_content(&repo);
    let git_files = |repo: &Repository| {
        let mut names: Vec<_> = std::fs::read_dir(repo.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        names
    };
    let files = git_files(&repo);
    let plain = status(&repo).unwrap();

    let refreshed = status_refreshing_index(&repo).unwrap();

    assert_eq!(refreshed, plain);
    assert_eq!(index_content(&repo), content);
    assert_eq!(git_files(&repo), files);
}

#[test]
fn the_combined_read_answers_what_the_two_separate_reads_do() {
    let (dir, repo) = committed_repo();
    write_file(dir.path(), "a.txt", "changed\n");
    write_file(dir.path(), "untracked.txt", "untracked\n");
    backdate(dir.path(), "dir/b.txt");
    let expected_status = status(&repo).unwrap();
    let expected_stamp = change_stamp(&repo).unwrap();

    let read = status_with_stamp(&repo).unwrap();

    assert_eq!(read.status, expected_status);
    assert_eq!(read.stamp, expected_stamp);
    // The refresh it wrote changes neither.
    assert_eq!(change_stamp(&repo).unwrap(), expected_stamp);
    assert_eq!(status(&repo).unwrap(), expected_status);
}

#[test]
fn a_conflicted_index_keeps_its_stages() {
    let (dir, repo) = committed_repo();
    let main = repo.head().unwrap().name().unwrap().to_string();
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("other", &base, false).unwrap();
    write_file(dir.path(), "a.txt", "ours\n");
    commit_all(&repo, "ours");
    repo.set_head("refs/heads/other").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "a.txt", "theirs\n");
    commit_all(&repo, "theirs");
    repo.set_head(&main).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    let theirs = repo
        .find_annotated_commit(
            repo.find_branch("other", git2::BranchType::Local)
                .unwrap()
                .get()
                .target()
                .unwrap(),
        )
        .unwrap();
    repo.merge(&[&theirs], None, None).unwrap();
    assert!(repo.index().unwrap().has_conflicts());
    backdate(dir.path(), "dir/b.txt");
    let content = index_content(&repo);
    let plain = status(&repo).unwrap();

    let refreshed = status_refreshing_index(&repo).unwrap();

    assert_eq!(refreshed, plain);
    assert_eq!(index_content(&repo), content);
}

#[test]
fn a_linked_worktree_refreshes_its_own_index() {
    let (dir, repo) = committed_repo();
    let outside = tempfile::TempDir::new().unwrap();
    let path = outside.path().join("linked");
    repo.worktree("linked", &path, None).unwrap();
    let linked = Repository::open(&path).unwrap();
    let main_index = std::fs::read(repo.path().join("index")).unwrap();
    let when = backdate(&path, "a.txt");

    let entries = status_refreshing_index(&linked).unwrap();

    assert!(entries.is_empty(), "{entries:?}");
    assert_eq!(indexed_mtime(&linked, "a.txt"), seconds(when));
    assert_eq!(
        std::fs::read(repo.path().join("index")).unwrap(),
        main_index
    );
    let _ = dir;
}

#[test]
fn ignore_case_repositories_read_the_same_status() {
    let (dir, repo) = committed_repo();
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    let repo = Repository::open(dir.path()).unwrap();
    write_file(dir.path(), "a.txt", "changed\n");
    write_file(dir.path(), "C.txt", "new\n");
    let when = backdate(dir.path(), "dir/b.txt");
    let plain = status(&repo).unwrap();

    let refreshed = status_refreshing_index(&repo).unwrap();

    assert_eq!(refreshed, plain);
    assert_eq!(indexed_mtime(&repo, "dir/b.txt"), seconds(when));
}

/// The scan reads a scratch copy of the index; afterwards the repository has its own index back.
#[test]
fn the_repository_keeps_its_own_index() {
    let (dir, repo) = committed_repo();
    backdate(dir.path(), "a.txt");

    status_refreshing_index(&repo).unwrap();
    status_with_stamp(&repo).unwrap();

    let index = repo.index().unwrap();
    assert_eq!(
        index.path().map(Path::to_path_buf),
        Some(repo.path().join("index"))
    );
}

/// libgit2 would drop the intent-to-add mark of an entry it refreshes; git keeps it. Such an
/// index is left as it is.
#[test]
fn an_intent_to_add_entry_keeps_its_mark() {
    let (dir, repo) = committed_repo();
    write_file(dir.path(), "later.txt", "");
    git_ok(dir.path(), &["add", "-N", "later.txt"]);
    backdate(dir.path(), "later.txt");
    backdate(dir.path(), "a.txt");
    let content = index_content(&repo);

    status_refreshing_index(&repo).unwrap();

    assert_eq!(index_content(&repo), content);
    let index = git2::Index::open(&repo.path().join("index")).unwrap();
    let entry = index.get_path(Path::new("later.txt"), 0).unwrap();
    assert_ne!(entry.flags_extended & (1 << 13), 0, "intent-to-add kept");
}

/// The scratch index gets the repository's case handling: in a case-sensitive repository two
/// files that differ only in case stay two files. Linux only: macOS and Windows file systems are
/// case-insensitive by default, where the two names are one file.
#[cfg(target_os = "linux")]
#[test]
fn case_sensitive_repositories_keep_files_that_differ_only_in_case_apart() {
    let (dir, repo) = init_repo();
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", false)
        .unwrap();
    let repo = Repository::open(dir.path()).unwrap();
    write_file(dir.path(), "Foo.txt", "upper\n");
    write_file(dir.path(), "foo.txt", "lower\n");
    commit_all(&repo, "first");
    write_file(dir.path(), "foo.txt", "lower, edited\n");
    write_file(dir.path(), "FOO.txt", "new\n");
    let when = backdate(dir.path(), "Foo.txt");
    let plain = status(&repo).unwrap();
    let content = index_content(&repo);

    let refreshed = status_refreshing_index(&repo).unwrap();

    assert_eq!(refreshed, plain);
    assert_eq!(index_content(&repo), content);
    assert_eq!(indexed_mtime(&repo, "Foo.txt"), seconds(when));
}
