mod common;

use git_core::commit::{recent_authors, SignatureInput};

fn commit_as(repo: &git2::Repository, dir: &std::path::Path, name: &str, email: &str, file: &str) {
    common::write_file(dir, file, name);
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(file)).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now(name, email).unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, file, &tree, &parents)
        .unwrap();
}

fn pairs(authors: &[SignatureInput]) -> Vec<(&str, &str)> {
    authors
        .iter()
        .map(|a| (a.name.as_str(), a.email.as_str()))
        .collect()
}

#[test]
fn recent_authors_are_deduped_by_email_newest_first_without_me() {
    let (dir, repo) = common::init_repo();
    commit_as(&repo, dir.path(), "Ada", "ada@x.org", "1");
    commit_as(&repo, dir.path(), "Test User", "TEST@example.com", "2");
    commit_as(&repo, dir.path(), "Bob", "bob@x.org", "3");
    commit_as(&repo, dir.path(), "Ada L.", "ADA@x.org", "4");
    let authors = recent_authors(&repo, 20).unwrap();
    assert_eq!(
        pairs(&authors),
        [("Ada L.", "ADA@x.org"), ("Bob", "bob@x.org")]
    );
    assert_eq!(
        pairs(&recent_authors(&repo, 1).unwrap()),
        [("Ada L.", "ADA@x.org")]
    );
    assert!(recent_authors(&repo, 0).unwrap().is_empty());
}

#[test]
fn recent_authors_of_an_unborn_repo_is_empty() {
    let (_dir, repo) = common::init_repo();
    assert!(recent_authors(&repo, 20).unwrap().is_empty());
}

#[test]
fn recent_authors_only_look_at_the_newest_commits() {
    let (dir, repo) = common::init_repo();
    commit_as(&repo, dir.path(), "Ancient", "ancient@x.org", "old");
    for i in 0..git_core::commit::RECENT_AUTHORS_WALK_LIMIT {
        commit_as(
            &repo,
            dir.path(),
            "Bob",
            "bob@x.org",
            &format!("f{}", i % 2),
        );
    }
    assert_eq!(
        pairs(&recent_authors(&repo, 20).unwrap()),
        [("Bob", "bob@x.org")]
    );
}
