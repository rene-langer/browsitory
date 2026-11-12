//! Ahead/behind counts of local branches against their upstreams, kept between reads (#112).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use git2::{Oid, Repository};

/// The ahead/behind counts the branch list and the upstream reads computed, by the pair of tips
/// (local, upstream) they compare. The worker keeps one per repository, so a refresh counts
/// again only the pairs whose branch or upstream moved, and the upstream reads that follow the
/// branch list count nothing. A fresh one (as the plain [`crate::branch::list_branches`],
/// [`crate::remote::remote_upstreams`] and [`crate::remote::current_upstream`] use) counts
/// every pair.
///
/// A count depends only on the two commits and the parents git reads for the commits between
/// them, which only the shallow and graft files change (commits never change): a change to
/// either, or another repository, forgets every count.
#[derive(Default)]
pub struct AheadBehindCache {
    counts: HashMap<(Oid, Oid), (usize, usize)>,
    parents: Option<ParentsKey>,
    walks: usize,
}

/// The repository and the files that change commits' parents, as an [`AheadBehindCache`]'s
/// counts saw them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParentsKey {
    repository: PathBuf,
    /// The shallow and the graft file (`None`: no such file).
    grafts: [Option<Vec<u8>>; 2],
}

impl ParentsKey {
    fn new(repo: &Repository) -> Self {
        let common = repo.commondir();
        let read = |path: PathBuf| std::fs::read(path).ok();
        Self {
            repository: common.to_path_buf(),
            grafts: [
                read(common.join("shallow")),
                read(common.join("info").join("grafts")),
            ],
        }
    }
}

/// How often an [`AheadBehindCache`] did the work it saves, and what it keeps: for tests and
/// measurements.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AheadBehindStats {
    /// Ahead/behind walks run (`graph_ahead_behind`).
    pub walks: usize,
    /// Counts kept.
    pub kept: usize,
}

impl AheadBehindCache {
    pub fn stats(&self) -> AheadBehindStats {
        AheadBehindStats {
            walks: self.walks,
            kept: self.counts.len(),
        }
    }

    /// Called once per read before [`Self::count`]: forgets every count when the repository's
    /// shallow or graft file changed since.
    pub(crate) fn begin(&mut self, repo: &Repository) {
        let key = ParentsKey::new(repo);
        if self.parents.as_ref() != Some(&key) {
            self.counts.clear();
            self.parents = Some(key);
        }
    }

    /// Commits on `local` not on `upstream`, and on `upstream` not on `local`.
    pub(crate) fn count(
        &mut self,
        repo: &Repository,
        local: Oid,
        upstream: Oid,
    ) -> Result<(usize, usize), git2::Error> {
        if let Some(counts) = self.counts.get(&(local, upstream)) {
            return Ok(*counts);
        }
        self.walks += 1;
        let counts = repo.graph_ahead_behind(local, upstream)?;
        self.counts.insert((local, upstream), counts);
        Ok(counts)
    }

    /// Forgets every count but those of `pairs` (local, upstream), the ones the branch list
    /// just read.
    pub(crate) fn retain(&mut self, pairs: &[(Oid, Oid)]) {
        let pairs: HashSet<&(Oid, Oid)> = pairs.iter().collect();
        self.counts.retain(|pair, _| pairs.contains(pair));
    }
}
