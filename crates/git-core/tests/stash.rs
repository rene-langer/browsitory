mod common;

use common::{init_repo, path_exists, write_file};
use git_core::{
    apply_stash, create_commit, create_stash, drop_stash, list_stashes, pop_stash, stage_path,
    status,
};

fn commit_initial(dir: &tempfile::TempDir, repo: &mut git2::Repository) {
    write_file(dir, "a.txt", "hello\n");
    stage_path(repo, "a.txt").unwrap();
    create_commit(repo, "initial commit").unwrap();
}

#[test]
fn create_and_list_stash() {
    let (dir, mut repo) = init_repo();
    commit_initial(&dir, &mut repo);
    write_file(&dir, "a.txt", "changed\n");

    create_stash(&mut repo, Some("wip changes")).unwrap();

    let stashes = list_stashes(&mut repo).unwrap();
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].index, 0);
    assert!(stashes[0].message.contains("wip changes"));

    // Stashing restores the working tree to the clean HEAD state.
    assert!(status(&repo).unwrap().is_empty());
}

#[test]
fn untracked_file_is_included_in_stash_by_default() {
    let (dir, mut repo) = init_repo();
    commit_initial(&dir, &mut repo);
    write_file(&dir, "untracked.txt", "new file\n");

    create_stash(&mut repo, None).unwrap();

    // The untracked file must be swept up by the stash (INCLUDE_UNTRACKED
    // default) and removed from the working tree...
    assert!(!path_exists(&dir, "untracked.txt"));
    assert!(status(&repo).unwrap().is_empty());

    // ...and come back on apply.
    apply_stash(&mut repo, 0).unwrap();
    assert!(path_exists(&dir, "untracked.txt"));
}

#[test]
fn apply_stash_restores_changes_without_removing_the_stash() {
    let (dir, mut repo) = init_repo();
    commit_initial(&dir, &mut repo);
    write_file(&dir, "a.txt", "changed\n");
    create_stash(&mut repo, None).unwrap();

    apply_stash(&mut repo, 0).unwrap();

    assert!(!status(&repo).unwrap().is_empty());
    assert_eq!(list_stashes(&mut repo).unwrap().len(), 1);
}

#[test]
fn pop_stash_applies_and_removes_the_entry() {
    let (dir, mut repo) = init_repo();
    commit_initial(&dir, &mut repo);
    write_file(&dir, "a.txt", "changed\n");
    create_stash(&mut repo, None).unwrap();

    pop_stash(&mut repo, 0).unwrap();

    assert!(!status(&repo).unwrap().is_empty());
    assert!(list_stashes(&mut repo).unwrap().is_empty());
}

#[test]
fn drop_stash_removes_entry_without_applying_it() {
    let (dir, mut repo) = init_repo();
    commit_initial(&dir, &mut repo);
    write_file(&dir, "a.txt", "changed\n");
    create_stash(&mut repo, None).unwrap();

    drop_stash(&mut repo, 0).unwrap();

    assert!(list_stashes(&mut repo).unwrap().is_empty());
    // Dropping (not applying) leaves the working tree at the clean state
    // stashing left it in.
    assert!(status(&repo).unwrap().is_empty());
}

#[test]
fn apply_with_conflicting_working_tree_changes_surfaces_an_error() {
    let (dir, mut repo) = init_repo();
    commit_initial(&dir, &mut repo);
    write_file(&dir, "a.txt", "stashed change\n");
    create_stash(&mut repo, None).unwrap();

    // A different, conflicting uncommitted change now sits in the working
    // tree where the stash would apply.
    write_file(&dir, "a.txt", "conflicting local change\n");

    let result = apply_stash(&mut repo, 0);
    assert!(result.is_err());
    // The stash must not have been silently dropped/clobbered.
    assert_eq!(list_stashes(&mut repo).unwrap().len(), 1);
}
