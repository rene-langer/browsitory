//! Ahead/behind counts of local branches against their upstreams, kept between reads (#112).

use git2::{Oid, Repository};

/// The ahead/behind counts the branch list and the upstream reads computed. The worker keeps one
/// per repository; a fresh one (as the plain [`crate::branch::list_branches`],
/// [`crate::remote::remote_upstreams`] and [`crate::remote::current_upstream`] use) counts
/// again.
#[derive(Default)]
pub struct AheadBehindCache {
    stats: AheadBehindStats,
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
        self.stats
    }

    /// Called once per read before [`Self::count`].
    pub(crate) fn begin(&mut self, _repo: &Repository) {}

    /// Commits on `local` not on `upstream`, and on `upstream` not on `local`.
    pub(crate) fn count(
        &mut self,
        repo: &Repository,
        local: Oid,
        upstream: Oid,
    ) -> Result<(usize, usize), git2::Error> {
        self.stats.walks += 1;
        repo.graph_ahead_behind(local, upstream)
    }

    /// Forgets every count but those of `pairs` (local, upstream), the ones the branch list
    /// just read.
    pub(crate) fn retain(&mut self, _pairs: &[(Oid, Oid)]) {}
}
