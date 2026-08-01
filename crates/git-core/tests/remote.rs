mod common;

use common::init_repo;
use git_core::{add_remote, list_remotes, remove_remote, rename_remote, set_remote_url};

#[test]
fn add_and_list_remotes() {
    let (_dir, repo) = init_repo();

    add_remote(&repo, "origin", "https://example.com/repo.git").unwrap();

    let remotes = list_remotes(&repo).unwrap();
    assert_eq!(remotes.len(), 1);
    assert_eq!(remotes[0].name, "origin");
    assert_eq!(remotes[0].url, "https://example.com/repo.git");
    assert_eq!(remotes[0].push_url, None);
}

#[test]
fn remove_remote_drops_it_from_the_list() {
    let (_dir, repo) = init_repo();
    add_remote(&repo, "origin", "https://example.com/repo.git").unwrap();

    remove_remote(&repo, "origin").unwrap();

    assert!(list_remotes(&repo).unwrap().is_empty());
}

#[test]
fn rename_remote_preserves_its_url() {
    let (_dir, repo) = init_repo();
    add_remote(&repo, "origin", "https://example.com/repo.git").unwrap();

    rename_remote(&repo, "origin", "upstream").unwrap();

    let remotes = list_remotes(&repo).unwrap();
    assert_eq!(remotes.len(), 1);
    assert_eq!(remotes[0].name, "upstream");
    assert_eq!(remotes[0].url, "https://example.com/repo.git");
}

#[test]
fn set_remote_url_updates_it() {
    let (_dir, repo) = init_repo();
    add_remote(&repo, "origin", "https://example.com/repo.git").unwrap();

    set_remote_url(&repo, "origin", "https://example.com/other.git").unwrap();

    let remotes = list_remotes(&repo).unwrap();
    assert_eq!(remotes[0].url, "https://example.com/other.git");
}
