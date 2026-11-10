//! What the commit details add to the graph row (#641): the author and the committer with their
//! own times, and the branches and tags that contain the commit.

use std::time::{Duration, Instant};

use git2::{BranchType, Oid, ReferenceType, Repository};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommitDetailsError {
    #[error("failed to read commit details: {0}")]
    Git(#[from] git2::Error),
}

/// A name, an email and a Unix-seconds time, as git records them for the author and the committer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub name: String,
    pub email: String,
    pub timestamp: i64,
}

/// The details of one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDetails {
    pub author: Person,
    pub committer: Person,
    /// Local branches whose tip is the commit or descends from it, sorted.
    pub branches: Vec<String>,
    /// Like `branches`, the remote-tracking branches as `<remote>/<branch>`; symbolic refs
    /// such as `origin/HEAD` are skipped.
    pub remote_branches: Vec<String>,
    /// Tags (lightweight or annotated) whose commit is the commit or descends from it, sorted.
    /// A tag that names no commit (a tree or a blob) is skipped.
    pub tags: Vec<String>,
    /// `false` when `budget` ran out before every branch and tag was checked: the lists then
    /// hold only the refs checked so far.
    pub complete: bool,
}

/// The details of `commit_id`. Checking whether a ref contains the commit walks history, so
/// a repository with many refs could take long: no ref is checked once `budget` has passed,
/// and `complete` says so.
pub fn commit_details(
    repo: &Repository,
    commit_id: &str,
    budget: Duration,
) -> Result<CommitDetails, CommitDetailsError> {
    let commit = repo.find_commit(Oid::from_str(commit_id)?)?;
    let target = commit.id();
    let mut scan = Scan {
        repo,
        target,
        deadline: Instant::now() + budget,
        complete: true,
    };
    let branches = scan.branches(BranchType::Local)?;
    let remote_branches = scan.branches(BranchType::Remote)?;
    let tags = scan.tags()?;
    let author = person(&commit.author());
    let committer = person(&commit.committer());
    Ok(CommitDetails {
        author,
        committer,
        branches,
        remote_branches,
        tags,
        complete: scan.complete,
    })
}

fn person(signature: &git2::Signature<'_>) -> Person {
    Person {
        name: String::from_utf8_lossy(signature.name_bytes()).into_owned(),
        email: String::from_utf8_lossy(signature.email_bytes()).into_owned(),
        timestamp: signature.when().seconds(),
    }
}

struct Scan<'r> {
    repo: &'r Repository,
    target: Oid,
    deadline: Instant,
    complete: bool,
}

impl Scan<'_> {
    /// Whether `tip` is or contains the target; `None` once the budget is spent.
    fn contains(&mut self, tip: Oid) -> Result<Option<bool>, git2::Error> {
        if !self.complete || Instant::now() >= self.deadline {
            self.complete = false;
            return Ok(None);
        }
        Ok(Some(
            tip == self.target || self.repo.graph_descendant_of(tip, self.target)?,
        ))
    }

    fn branches(&mut self, kind: BranchType) -> Result<Vec<String>, git2::Error> {
        let mut names = Vec::new();
        for entry in self.repo.branches(Some(kind))? {
            let (branch, _) = entry?;
            if branch.get().kind() != Some(ReferenceType::Direct) {
                continue;
            }
            let Ok(Some(name)) = branch.name() else {
                continue;
            };
            let Ok(tip) = branch.get().peel_to_commit() else {
                continue;
            };
            match self.contains(tip.id())? {
                Some(true) => names.push(name.to_string()),
                Some(false) => {}
                None => break,
            }
        }
        names.sort();
        Ok(names)
    }

    fn tags(&mut self) -> Result<Vec<String>, git2::Error> {
        let mut names = Vec::new();
        let tag_names = self.repo.tag_names(None)?;
        for name in tag_names.iter() {
            let Ok(Some(name)) = name else {
                continue;
            };
            let Ok(reference) = self.repo.find_reference(&format!("refs/tags/{name}")) else {
                continue;
            };
            let Ok(tip) = reference.peel_to_commit() else {
                continue;
            };
            match self.contains(tip.id())? {
                Some(true) => names.push(name.to_string()),
                Some(false) => {}
                None => break,
            }
        }
        names.sort();
        Ok(names)
    }
}
