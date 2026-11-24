//! What the app's status reads and `head_info` say about files under an external filter driver
//! (#1087): such a file is marked with the driver's name when it shows as changed, a Git LFS
//! file whose content matches its stored pointer isn't listed, and `head_info` names the
//! configured drivers the repository's attributes use. The driver commands here would fail if
//! run; no test runs them.
mod common;

use common::{commit_all, init_repo, write_file};
use git2::Repository;
use git_core::merge::{MergeOptions, MergeOutcome};
use git_core::status::{StatusEntry, StatusKind};
use tempfile::TempDir;

const SECRET: &str = "secret.env";
const PLAIN: &str = "plain.txt";

/// `secret.env` (filtered) and `plain.txt` committed; no driver configured yet.
fn fixture() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(
        dir.path(),
        ".gitattributes",
        "secret.env filter=crypt\nnew/** filter=crypt\nlink filter=crypt\n",
    );
    write_file(dir.path(), SECRET, "v1\n");
    write_file(dir.path(), PLAIN, "p1\n");
    commit_all(&repo, "base");
    (dir, repo)
}

fn configure_driver(repo: &Repository) {
    let mut config = repo.config().unwrap();
    config.set_str("filter.crypt.clean", "false").unwrap();
    config.set_str("filter.crypt.smudge", "false").unwrap();
}

/// Both reads the app lists changes from must agree.
fn app_status(repo: &Repository) -> Vec<StatusEntry> {
    let refreshing = git_core::status::status_refreshing_index(repo).unwrap();
    let with_stamp = git_core::change_stamp::status_with_stamp(repo)
        .unwrap()
        .status;
    assert_eq!(refreshing, with_stamp);
    refreshing
}

fn entry<'a>(entries: &'a [StatusEntry], path: &str, staged: bool) -> &'a StatusEntry {
    entries
        .iter()
        .find(|entry| entry.path == path && entry.staged == staged)
        .unwrap_or_else(|| panic!("no {path} (staged: {staged}) in {entries:?}"))
}

#[test]
fn a_changed_filtered_file_is_marked_with_its_driver() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), SECRET, "v2\n");

    let entries = app_status(&repo);

    let secret = entry(&entries, SECRET, false);
    assert_eq!(secret.kind, StatusKind::Modified);
    assert_eq!(secret.external_filter.as_deref(), Some("crypt"));
}

#[test]
fn a_new_filtered_file_is_marked_also_in_an_untracked_folder() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), "new/a.env", "a\n");

    let entries = app_status(&repo);
    let folder = git_core::status::untracked_folder_files(&repo, "new/").unwrap();

    let listed = entry(&entries, "new/a.env", false);
    assert_eq!(listed.kind, StatusKind::New);
    assert_eq!(listed.external_filter.as_deref(), Some("crypt"));
    assert_eq!(folder.len(), 1);
    assert_eq!(folder[0].external_filter.as_deref(), Some("crypt"));
}

#[test]
fn a_staged_change_of_a_filtered_file_is_not_marked() {
    let (dir, repo) = fixture();
    write_file(dir.path(), SECRET, "v2\n");
    git_core::stage::stage_file(&repo, SECRET).unwrap();
    configure_driver(&repo);

    let entries = app_status(&repo);

    assert_eq!(entry(&entries, SECRET, true).external_filter, None);
    assert!(!entries.iter().any(|entry| !entry.staged), "{entries:?}");
}

#[test]
fn a_deleted_filtered_file_is_not_marked() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    std::fs::remove_file(dir.path().join(SECRET)).unwrap();

    let entries = app_status(&repo);

    let secret = entry(&entries, SECRET, false);
    assert_eq!(secret.kind, StatusKind::Deleted);
    assert_eq!(secret.external_filter, None);
}

#[test]
fn an_unfiltered_file_is_not_marked() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    write_file(dir.path(), PLAIN, "p2\n");

    let entries = app_status(&repo);

    assert_eq!(entry(&entries, PLAIN, false).external_filter, None);
}

#[test]
fn without_driver_config_nothing_is_marked() {
    let (dir, repo) = fixture();
    write_file(dir.path(), SECRET, "v2\n");

    let entries = app_status(&repo);

    assert_eq!(entry(&entries, SECRET, false).external_filter, None);
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_the_filter_names_is_not_marked() {
    let (dir, repo) = fixture();
    configure_driver(&repo);
    std::os::unix::fs::symlink(PLAIN, dir.path().join("link")).unwrap();

    let entries = app_status(&repo);

    assert_eq!(entry(&entries, "link", false).external_filter, None);
}

#[test]
fn a_conflicted_filtered_file_is_marked() {
    let (dir, repo) = fixture();
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let blob = repo.blob(b"other\n").unwrap();
    let mut builder = repo.treebuilder(Some(&base.tree().unwrap())).unwrap();
    builder.insert(SECRET, blob, 0o100644).unwrap();
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let signature = repo.signature().unwrap();
    repo.commit(
        Some("refs/heads/other"),
        &signature,
        &signature,
        "other",
        &tree,
        &[&base],
    )
    .unwrap();
    write_file(dir.path(), SECRET, "main\n");
    commit_all(&repo, "main side");
    let outcome = git_core::merge::start_merge(&repo, "other", MergeOptions::default()).unwrap();
    assert!(
        matches!(outcome, MergeOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    configure_driver(&repo);

    let entries = app_status(&repo);

    let secret = entry(&entries, SECRET, false);
    assert_eq!(secret.kind, StatusKind::Conflicted);
    assert_eq!(secret.external_filter.as_deref(), Some("crypt"));
}

// --- Git LFS: a file whose content matches its stored pointer is unchanged ---

const LFS_CONTENT: &str = "hello lfs\n";
/// `sha256sum` of `LFS_CONTENT`.
const LFS_OID: &str = "4859402c258b836d02e955d1090e29f586e58b2040504d68afec3d8d43757bba";

fn pointer(oid: &str, size: usize) -> String {
    format!("version https://git-lfs.github.com/spec/v1\noid sha256:{oid}\nsize {size}\n")
}

/// What `git lfs install` writes; never run here.
fn configure_lfs(repo: &Repository) {
    let mut config = repo.config().unwrap();
    config
        .set_str("filter.lfs.process", "git-lfs filter-process")
        .unwrap();
    config
        .set_str("filter.lfs.clean", "git-lfs clean -- %f")
        .unwrap();
    config
        .set_str("filter.lfs.smudge", "git-lfs smudge -- %f")
        .unwrap();
    config.set_bool("filter.lfs.required", true).unwrap();
}

/// `big.bin` committed as `stored` (`*.bin filter=lfs`), then `on_disk` written over it, as the
/// LFS driver's smudge would have, with the index's stat data no longer matching.
fn lfs_fixture(stored: &str, on_disk: &str) -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), ".gitattributes", "*.bin filter=lfs\n");
    write_file(dir.path(), "big.bin", stored);
    commit_all(&repo, "base");
    write_file(dir.path(), "big.bin", on_disk);
    (dir, repo)
}

#[test]
fn an_lfs_file_whose_content_matches_its_pointer_is_not_listed() {
    let (_dir, repo) = lfs_fixture(&pointer(LFS_OID, LFS_CONTENT.len()), LFS_CONTENT);
    configure_lfs(&repo);

    let entries = app_status(&repo);

    assert!(
        !entries.iter().any(|entry| entry.path == "big.bin"),
        "{entries:?}"
    );
}

#[test]
fn an_lfs_file_with_other_content_of_the_same_size_is_listed_and_marked() {
    let (_dir, repo) = lfs_fixture(&pointer(LFS_OID, LFS_CONTENT.len()), "HELLO LFS\n");
    configure_lfs(&repo);

    let entries = app_status(&repo);

    let big = entry(&entries, "big.bin", false);
    assert_eq!(big.kind, StatusKind::Modified);
    assert_eq!(big.external_filter.as_deref(), Some("lfs"));
}

#[test]
fn an_lfs_file_of_another_size_is_listed_and_marked() {
    let (_dir, repo) = lfs_fixture(&pointer(LFS_OID, LFS_CONTENT.len()), "hello lfs!\n");
    configure_lfs(&repo);

    let entries = app_status(&repo);

    assert_eq!(
        entry(&entries, "big.bin", false).external_filter.as_deref(),
        Some("lfs")
    );
}

#[test]
fn an_lfs_pointer_with_an_extension_line_is_not_trusted() {
    let stored = format!(
        "version https://git-lfs.github.com/spec/v1\next-0-foo sha256:{LFS_OID}\n\
         oid sha256:{LFS_OID}\nsize {}\n",
        LFS_CONTENT.len()
    );
    let (_dir, repo) = lfs_fixture(&stored, LFS_CONTENT);
    configure_lfs(&repo);

    let entries = app_status(&repo);

    assert_eq!(
        entry(&entries, "big.bin", false).external_filter.as_deref(),
        Some("lfs")
    );
}

#[test]
fn a_matching_pointer_under_a_driver_that_is_not_git_lfs_stays_listed() {
    let (_dir, repo) = lfs_fixture(&pointer(LFS_OID, LFS_CONTENT.len()), LFS_CONTENT);
    let mut config = repo.config().unwrap();
    config
        .set_str("filter.lfs.clean", "other-tool clean")
        .unwrap();

    let entries = app_status(&repo);

    assert_eq!(
        entry(&entries, "big.bin", false).external_filter.as_deref(),
        Some("lfs")
    );
}

#[test]
fn the_read_only_status_still_lists_a_matching_lfs_file_unmarked() {
    // The write commands' own read stays raw (#1087 changes only what the app lists).
    let (_dir, repo) = lfs_fixture(&pointer(LFS_OID, LFS_CONTENT.len()), LFS_CONTENT);
    configure_lfs(&repo);

    let entries = git_core::status::status(&repo).unwrap();

    assert_eq!(entry(&entries, "big.bin", false).external_filter, None);
}

// --- head_info: the drivers the repository's attributes name ---

fn head_filters(repo: &Repository) -> Vec<String> {
    git_core::repo::head_info(repo).unwrap().external_filters
}

#[test]
fn head_info_names_a_configured_driver_the_attributes_use() {
    let (_dir, repo) = fixture();
    configure_driver(&repo);

    assert_eq!(head_filters(&repo), vec!["crypt".to_string()]);
}

#[test]
fn head_info_names_no_driver_without_driver_config() {
    let (_dir, repo) = fixture();

    assert!(head_filters(&repo).is_empty());
}

#[test]
fn head_info_names_no_driver_that_no_attribute_uses() {
    // A global `git lfs install` configures `lfs` for every repository.
    let (dir, repo) = init_repo();
    write_file(
        dir.path(),
        ".gitattributes",
        "# a.bin filter=lfs\n*.txt text\n",
    );
    write_file(dir.path(), PLAIN, "p1\n");
    commit_all(&repo, "base");
    configure_lfs(&repo);

    assert!(head_filters(&repo).is_empty());
}

#[test]
fn head_info_reads_a_nested_attributes_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "sub/.gitattributes", "*.bin filter=lfs -text\n");
    write_file(dir.path(), PLAIN, "p1\n");
    commit_all(&repo, "base");
    configure_lfs(&repo);
    configure_driver(&repo);

    assert_eq!(head_filters(&repo), vec!["lfs".to_string()]);
}

#[test]
fn head_info_reads_info_attributes_and_macros() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), PLAIN, "p1\n");
    commit_all(&repo, "base");
    std::fs::create_dir_all(repo.path().join("info")).unwrap();
    std::fs::write(
        repo.path().join("info/attributes"),
        "[attr]secret filter=crypt diff=crypt\n*.env secret\n",
    )
    .unwrap();
    configure_driver(&repo);
    configure_lfs(&repo);

    assert_eq!(head_filters(&repo), vec!["crypt".to_string()]);
}

#[test]
fn head_info_reads_the_configured_attributes_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), PLAIN, "p1\n");
    commit_all(&repo, "base");
    let outside = TempDir::new().unwrap();
    let file = outside.path().join("attributes");
    std::fs::write(&file, "*.psd filter=lfs\n\"with space.env\" filter=crypt\n").unwrap();
    let mut config = repo.config().unwrap();
    config
        .set_str("core.attributesFile", file.to_str().unwrap())
        .unwrap();
    configure_driver(&repo);
    configure_lfs(&repo);

    assert_eq!(
        head_filters(&repo),
        vec!["crypt".to_string(), "lfs".to_string()]
    );
}

#[test]
fn head_info_reads_an_untracked_top_level_attributes_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), PLAIN, "p1\n");
    commit_all(&repo, "base");
    write_file(dir.path(), ".gitattributes", "secret.env filter=crypt\n");
    configure_driver(&repo);

    assert_eq!(head_filters(&repo), vec!["crypt".to_string()]);
}

#[test]
fn head_info_reads_a_tracked_attributes_file_from_the_index_when_deleted_on_disk() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "sub/.gitattributes", "*.env filter=crypt\n");
    commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("sub/.gitattributes")).unwrap();
    configure_driver(&repo);

    assert_eq!(head_filters(&repo), vec!["crypt".to_string()]);
}
