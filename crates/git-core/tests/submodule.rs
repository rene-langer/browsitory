#[allow(dead_code)]
mod common;

use std::path::Path;

use common::{commit_all, write_file};
use git2::Repository;
use git_core::status::StatusKind;
use git_core::submodule::{init_submodule, list_submodules, update_submodule, SubmoduleError};

#[test]
fn lists_configured_metadata_then_initializes_and_updates_a_submodule() {
    let (dir, parent, child_url, child_head) = configured_submodule_checkout();

    let entries = list_submodules(&parent).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "deps/child");
    assert_eq!(entries[0].url.as_deref(), Some(child_url.as_str()));
    assert_eq!(entries[0].gitlink_id.as_deref(), Some(child_head.as_str()));
    assert!(!entries[0].initialized);
    assert_eq!(entries[0].head_id, None);

    init_submodule(&parent, "deps/child").unwrap();
    update_submodule(&parent, "deps/child", false).unwrap();

    let entries = list_submodules(&parent).unwrap();
    assert!(entries[0].initialized);
    assert_eq!(entries[0].head_id.as_deref(), Some(child_head.as_str()));

    drop(dir);
}

#[test]
fn refuses_to_update_an_uninitialized_submodule_without_creating_config() {
    let (_dir, parent, _, _) = configured_submodule_checkout();
    let key = "submodule.deps/child.url";

    assert!(parent.config().unwrap().get_string(key).is_err());

    let result = update_submodule(&parent, "deps/child", false);

    let Err(SubmoduleError::Git(error)) = result else {
        panic!("uninitialized update unexpectedly succeeded");
    };
    assert_eq!(error.message(), "submodule is not initialized");
    assert!(parent.config().unwrap().get_string(key).is_err());
    assert!(!list_submodules(&parent).unwrap()[0].initialized);
    assert_eq!(list_submodules(&parent).unwrap()[0].head_id, None);
}

fn configured_submodule_checkout() -> (tempfile::TempDir, Repository, String, String) {
    let dir = tempfile::TempDir::new().unwrap();
    let child_path = dir.path().join("child-source");
    let child = Repository::init(&child_path).unwrap();
    configure_identity(&child);
    write_file(&child_path, "child.txt", "initial child commit");
    commit_all(&child, "child commit");
    let child_head = child.head().unwrap().target().unwrap().to_string();
    let child_url = child_path.to_string_lossy().into_owned();

    let parent_path = dir.path().join("parent-source");
    let parent = Repository::init(&parent_path).unwrap();
    configure_identity(&parent);
    let mut submodule = parent
        .submodule(&child_url, Path::new("deps/child"), true)
        .unwrap();
    submodule.clone(None).unwrap();
    submodule.add_to_index(true).unwrap();
    submodule.add_finalize().unwrap();
    drop(submodule);
    commit_all(&parent, "add child submodule");

    let checkout_path = dir.path().join("checkout");
    let checkout = Repository::clone(parent_path.to_str().unwrap(), &checkout_path).unwrap();
    configure_identity(&checkout);

    (dir, checkout, child_url, child_head)
}

fn configure_identity(repo: &Repository) {
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
}

#[test]
fn rejects_unknown_and_invalid_submodule_paths() {
    let (_dir, parent, _, _) = configured_submodule_checkout();

    assert!(matches!(
        init_submodule(&parent, "deps/missing"),
        Err(SubmoduleError::NotFound)
    ));
    assert!(matches!(
        update_submodule(&parent, "deps/child/", false),
        Err(SubmoduleError::NotFound)
    ));
    assert!(matches!(
        init_submodule(&parent, "../child"),
        Err(SubmoduleError::InvalidPath)
    ));
}

#[test]
fn initializes_an_already_initialized_submodule_without_overwriting_it() {
    let (_dir, parent, child_url, _) = configured_submodule_checkout();

    init_submodule(&parent, "deps/child").unwrap();
    parent
        .config()
        .unwrap()
        .set_str("submodule.deps/child.url", "https://example.invalid/child")
        .unwrap();
    init_submodule(&parent, "deps/child").unwrap();

    let entry = list_submodules(&parent).unwrap().pop().unwrap();
    assert!(entry.initialized);
    assert_eq!(entry.url.as_deref(), Some(child_url.as_str()));
    assert_eq!(
        parent
            .config()
            .unwrap()
            .get_string("submodule.deps/child.url")
            .unwrap(),
        "https://example.invalid/child"
    );
}

#[test]
fn updates_only_the_requested_submodule_when_recursion_is_disabled() {
    let (_dir, parent) = configured_nested_submodule_checkout();

    init_submodule(&parent, "deps/child").unwrap();
    update_submodule(&parent, "deps/child", false).unwrap();

    assert!(list_submodules(&parent).unwrap()[0].head_id.is_some());
    let child_path = parent.workdir().unwrap().join("deps/child");
    let child = Repository::open(child_path).unwrap();
    let nested = child.submodules().unwrap().pop().unwrap();
    assert_eq!(nested.workdir_id(), None);
}

#[test]
fn reports_a_changed_checked_out_child_head_as_a_parent_gitlink_change() {
    let (_dir, parent, _, _) = configured_submodule_checkout();
    init_submodule(&parent, "deps/child").unwrap();
    update_submodule(&parent, "deps/child", false).unwrap();

    let child_path = parent.workdir().unwrap().join("deps/child");
    let child = Repository::open(&child_path).unwrap();
    configure_identity(&child);
    write_file(&child_path, "child.txt", "advanced child commit");
    commit_all(&child, "advance child");

    let entry = git_core::status::status(&parent)
        .unwrap()
        .into_iter()
        .find(|entry| entry.path == "deps/child")
        .unwrap();
    assert!(!entry.staged);
    assert_eq!(entry.kind, StatusKind::Modified);
}

fn configured_nested_submodule_checkout() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::TempDir::new().unwrap();
    let grandchild_path = dir.path().join("grandchild-source");
    let grandchild = Repository::init(&grandchild_path).unwrap();
    configure_identity(&grandchild);
    write_file(&grandchild_path, "grandchild.txt", "grandchild commit");
    commit_all(&grandchild, "grandchild commit");

    let child_path = dir.path().join("child-source");
    let child = Repository::init(&child_path).unwrap();
    configure_identity(&child);
    write_file(&child_path, "child.txt", "child commit");
    commit_all(&child, "child commit");
    add_submodule(
        &child,
        &grandchild_path.to_string_lossy(),
        "deps/grandchild",
    );
    commit_all(&child, "add grandchild submodule");

    let parent_path = dir.path().join("parent-source");
    let parent = Repository::init(&parent_path).unwrap();
    configure_identity(&parent);
    add_submodule(&parent, &child_path.to_string_lossy(), "deps/child");
    commit_all(&parent, "add child submodule");

    let checkout_path = dir.path().join("checkout");
    let checkout = Repository::clone(parent_path.to_str().unwrap(), checkout_path).unwrap();
    (dir, checkout)
}

fn add_submodule(parent: &Repository, url: &str, path: &str) {
    let mut submodule = parent.submodule(url, Path::new(path), true).unwrap();
    submodule.clone(None).unwrap();
    submodule.add_to_index(true).unwrap();
    submodule.add_finalize().unwrap();
}
