//! #840: the app turns off libgit2's per-read object hash check, as git does on ordinary reads.
//! One test in its own binary, because the setting is process-wide.

mod common;

use std::path::{Path, PathBuf};

use common::init_repo;

fn loose_object_path(git_dir: &Path, id: git2::Oid) -> PathBuf {
    let hex = id.to_string();
    git_dir.join("objects").join(&hex[..2]).join(&hex[2..])
}

/// Stores `real` and `stand_in` as loose blobs, then puts `stand_in`'s object file under
/// `real`'s id: the object inflates fine, but its content doesn't hash to its id.
fn blob_with_wrong_content(repo: &git2::Repository, real: &[u8], stand_in: &[u8]) -> git2::Oid {
    let real_id = repo.blob(real).unwrap();
    let stand_in_id = repo.blob(stand_in).unwrap();
    let target = loose_object_path(repo.path(), real_id);
    let mut permissions = std::fs::metadata(&target).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&target, permissions).unwrap();
    std::fs::copy(loose_object_path(repo.path(), stand_in_id), &target).unwrap();
    real_id
}

#[test]
fn configure_libgit2_reads_objects_without_rehashing_them() {
    let (_dir, repo) = init_repo();
    let id = blob_with_wrong_content(&repo, b"real\n", b"stand-in\n");
    // libgit2's default: the read fails on the hash mismatch.
    let path = repo.path().to_path_buf();
    assert!(git2::Repository::open(&path)
        .unwrap()
        .find_blob(id)
        .is_err());

    git_core::repo::configure_libgit2();

    let reopened = git2::Repository::open(&path).unwrap();
    assert_eq!(reopened.find_blob(id).unwrap().content(), b"stand-in\n");
}
