//! #1070: `list_stashes_cached` works out a stash's counts once and keeps them while the stash is
//! listed. Seen through the object store: once counted, the stash's untracked tree (over
//! libgit2's 4 KiB object cache limit, and read through a fresh handle) can go missing.

mod common;

use common::{commit_all, init_repo, write_file};

#[test]
fn a_listed_stash_keeps_its_counts_without_reading_its_trees_again() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial");
    write_file(dir.path(), "file.txt", "v2");
    for n in 0..300 {
        write_file(dir.path(), &format!("untracked-{n:04}.txt"), "new");
    }
    git_core::stash::save_stash(&mut repo, &git_core::stash::StashOptions::default()).unwrap();
    let mut cache = git_core::stash::StashCountCache::default();
    let first = git_core::stash::list_stashes_cached(&mut repo, &mut cache).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].file_count, 301);

    let stash = repo
        .find_commit(git2::Oid::from_str(&first[0].commit_id).unwrap())
        .unwrap();
    let tree = stash.parent(2).unwrap().tree_id().to_string();
    drop(stash);
    let loose = repo
        .path()
        .join("objects")
        .join(&tree[..2])
        .join(&tree[2..]);
    let mut permissions = std::fs::metadata(&loose).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&loose, permissions).unwrap();
    std::fs::remove_file(&loose).unwrap();
    let mut fresh = git2::Repository::open(dir.path()).unwrap();

    let again = git_core::stash::list_stashes_cached(&mut fresh, &mut cache).unwrap();
    assert_eq!(again, first);
}
