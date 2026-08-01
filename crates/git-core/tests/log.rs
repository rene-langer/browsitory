mod common;

use common::{init_repo, write_file};
use git_core::{commit_log, create_commit, stage_path};

fn commit_n_files(repo: &mut git2::Repository, dir: &tempfile::TempDir, n: usize) {
    for i in 0..n {
        let name = format!("file{i}.txt");
        write_file(dir, &name, "content\n");
        stage_path(repo, &name).unwrap();
        create_commit(repo, &format!("commit {i}")).unwrap();
    }
}

#[test]
fn returns_commits_newest_first() {
    let (dir, mut repo) = init_repo();
    commit_n_files(&mut repo, &dir, 3);

    let commits = commit_log(&repo, None, 0, 10).unwrap();

    assert_eq!(commits.len(), 3);
    assert_eq!(commits[0].summary, "commit 2");
    assert_eq!(commits[1].summary, "commit 1");
    assert_eq!(commits[2].summary, "commit 0");
}

#[test]
fn limit_and_skip_page_through_history() {
    let (dir, mut repo) = init_repo();
    commit_n_files(&mut repo, &dir, 5);

    let first_page = commit_log(&repo, None, 0, 2).unwrap();
    let second_page = commit_log(&repo, None, 2, 2).unwrap();

    assert_eq!(
        first_page.iter().map(|c| &c.summary).collect::<Vec<_>>(),
        vec!["commit 4", "commit 3"]
    );
    assert_eq!(
        second_page.iter().map(|c| &c.summary).collect::<Vec<_>>(),
        vec!["commit 2", "commit 1"]
    );
}

#[test]
fn root_commit_has_no_parents() {
    let (dir, mut repo) = init_repo();
    commit_n_files(&mut repo, &dir, 1);

    let commits = commit_log(&repo, None, 0, 10).unwrap();

    assert!(commits[0].parent_ids.is_empty());
}
