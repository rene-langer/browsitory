//! What the commit details add to the graph row (#641): the author and the committer with their
//! own times, and the branches and tags that contain the commit.

use std::collections::{BinaryHeap, HashMap};
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
    /// `false` when `budget` ran out before the walk that checks every branch and tag at once
    /// (#842) was done: the three lists are then empty.
    pub complete: bool,
}

/// The details of `commit_id`. Which refs contain the commit is answered by one walk over the
/// history above it (#842), so an old commit in a long history could take long: the walk
/// stops once `budget` has passed, and `complete` says so.
pub fn commit_details(
    repo: &Repository,
    commit_id: &str,
    budget: Duration,
) -> Result<CommitDetails, CommitDetailsError> {
    let commit = repo.find_commit(Oid::from_str(commit_id)?)?;
    let deadline = Instant::now() + budget;
    let refs = containment_refs(repo)?;
    let tips: Vec<Oid> = refs.iter().map(|named| named.tip).collect();
    let (branches, remote_branches, tags, complete) =
        match containing_tips(repo, &commit, &tips, deadline)? {
            Some(contains) => {
                let mut lists = (Vec::new(), Vec::new(), Vec::new());
                for (named, _) in refs.into_iter().zip(contains).filter(|(_, hit)| *hit) {
                    match named.kind {
                        RefKind::Local => lists.0.push(named.name),
                        RefKind::Remote => lists.1.push(named.name),
                        RefKind::Tag => lists.2.push(named.name),
                    }
                }
                lists.0.sort();
                lists.1.sort();
                lists.2.sort();
                (lists.0, lists.1, lists.2, true)
            }
            None => (Vec::new(), Vec::new(), Vec::new(), false),
        };
    let author = person(&commit.author());
    let committer = person(&commit.committer());
    Ok(CommitDetails {
        author,
        committer,
        branches,
        remote_branches,
        tags,
        complete,
    })
}

fn person(signature: &git2::Signature<'_>) -> Person {
    Person {
        name: String::from_utf8_lossy(signature.name_bytes()).into_owned(),
        email: String::from_utf8_lossy(signature.email_bytes()).into_owned(),
        timestamp: signature.when().seconds(),
    }
}

#[derive(Clone, Copy)]
enum RefKind {
    Local,
    Remote,
    Tag,
}

/// A ref the details list when it contains the commit, with the commit it names.
struct NamedTip {
    kind: RefKind,
    name: String,
    tip: Oid,
}

/// The local branches, the remote-tracking branches (without symbolic ones such as
/// `origin/HEAD`) and the tags that name a commit, each peeled to that commit.
fn containment_refs(repo: &Repository) -> Result<Vec<NamedTip>, git2::Error> {
    let mut refs = Vec::new();
    for (kind, ref_kind) in [
        (BranchType::Local, RefKind::Local),
        (BranchType::Remote, RefKind::Remote),
    ] {
        let mut names = Vec::new();
        for entry in repo.branches(Some(kind))? {
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
            names.push((name.to_string(), tip.id()));
        }
        refs.extend(names.into_iter().map(|(name, tip)| NamedTip {
            kind: ref_kind,
            name,
            tip,
        }));
    }
    let tag_names = repo.tag_names(None)?;
    for name in tag_names.iter() {
        let Ok(Some(name)) = name else {
            continue;
        };
        let Ok(reference) = repo.find_reference(&format!("refs/tags/{name}")) else {
            continue;
        };
        let Ok(tip) = reference.peel_to_commit() else {
            continue;
        };
        refs.push(NamedTip {
            kind: RefKind::Tag,
            name: name.to_string(),
            tip: tip.id(),
        });
    }
    Ok(refs)
}

/// How far before the target's commit time the walk still follows parents: commits dated
/// more than this before it are taken to be older than it, as `git name-rev` does
/// (`CUTOFF_DATE_SLOP`). Only a history whose clock ran back by more than this between the
/// target and a descendant could make a ref that contains the target look like it doesn't.
const CLOCK_SKEW_SLOP: i64 = 24 * 60 * 60;

/// For each of `tips`, whether it is `target` or descends from it; `None` when `deadline`
/// passed first.
///
/// One walk for all tips (#842) instead of one reachability walk per tip. Every commit carries
/// the set of tips it is reached from (a bitset, one bit per distinct tip) and hands it on to
/// its parents; the target's set is the answer. The walk goes newest commit first and stops
/// below the target's commit time (less [`CLOCK_SKEW_SLOP`]): a commit older than the target
/// can't descend from it, so a recent target costs a few commits, and an old one the history
/// above it. A commit that gets more tips after it was handed on (a clock that ran back) is
/// handed on again with them, so the order only costs time, never a tip.
pub(crate) fn containing_tips(
    repo: &Repository,
    target: &git2::Commit<'_>,
    tips: &[Oid],
    deadline: Instant,
) -> Result<Option<Vec<bool>>, git2::Error> {
    if tips.is_empty() {
        return Ok(Some(Vec::new()));
    }
    if Instant::now() >= deadline {
        return Ok(None);
    }
    let target_id = target.id();
    let cutoff = target.time().seconds().saturating_sub(CLOCK_SKEW_SLOP);
    // Refs often share a tip: one bit per distinct tip commit.
    let mut bit_of_tip: HashMap<Oid, usize> = HashMap::new();
    let tip_bits: Vec<usize> = tips
        .iter()
        .map(|tip| {
            let next = bit_of_tip.len();
            *bit_of_tip.entry(*tip).or_insert(next)
        })
        .collect();
    let words = bit_of_tip.len().div_ceil(64);

    // The tips not yet handed on, per commit queued, newest first by commit time. An entry is
    // taken when its commit is handed on, so only the walk's frontier is held.
    let mut pending: HashMap<Oid, Vec<u64>> = HashMap::new();
    let mut queue: BinaryHeap<(i64, Oid)> = BinaryHeap::new();
    let mut reached = vec![0u64; words];
    for (&tip, &bit) in &bit_of_tip {
        let time = repo.find_commit(tip)?.time().seconds();
        // The target itself is never older than the cut-off.
        if time < cutoff {
            continue;
        }
        let bits = pending.entry(tip).or_insert_with(|| {
            queue.push((time, tip));
            vec![0; words]
        });
        bits[bit / 64] |= 1 << (bit % 64);
    }
    while let Some((_, oid)) = queue.pop() {
        if Instant::now() >= deadline {
            return Ok(None);
        }
        let Some(bits) = pending.remove(&oid) else {
            continue;
        };
        if oid == target_id {
            for (word, add) in reached.iter_mut().zip(&bits) {
                *word |= add;
            }
            continue;
        }
        let commit = repo.find_commit(oid)?;
        for parent in commit.parents() {
            let time = parent.time().seconds();
            let id = parent.id();
            if time < cutoff {
                continue;
            }
            let into = pending.entry(id).or_insert_with(|| {
                queue.push((time, id));
                vec![0; words]
            });
            for (word, add) in into.iter_mut().zip(&bits) {
                *word |= add;
            }
        }
    }
    Ok(Some(
        tip_bits
            .into_iter()
            .map(|bit| reached[bit / 64] & (1 << (bit % 64)) != 0)
            .collect(),
    ))
}
