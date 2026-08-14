mod common;

use git_core::remote::{
    add_remote, clear_current_upstream, current_upstream, list_remotes, remove_remote,
    set_current_upstream, update_remote_urls, RemoteError,
};

#[test]
fn remote_crud_and_upstream_round_trip() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");

    add_remote(&repo, "origin", "file:///tmp/origin.git", None).unwrap();
    set_current_upstream(&repo, "origin", "main").unwrap();

    assert_eq!(list_remotes(&repo).unwrap()[0].name, "origin");
    assert_eq!(
        current_upstream(&repo).unwrap().unwrap().remote_name,
        "origin"
    );
    assert!(matches!(
        remove_remote(&repo, "origin"),
        Err(RemoteError::RemoteInUse { .. })
    ));

    clear_current_upstream(&repo).unwrap();
    remove_remote(&repo, "origin").unwrap();
}

#[test]
fn updating_a_remote_without_a_push_url_keeps_push_url_unset() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "file:///tmp/origin.git", None).unwrap();

    update_remote_urls(&repo, "origin", "file:///tmp/updated-origin.git", None).unwrap();

    let remote = list_remotes(&repo).unwrap().pop().unwrap();
    assert_eq!(remote.fetch_url, "file:///tmp/updated-origin.git");
    assert_eq!(remote.push_url, None);
}
