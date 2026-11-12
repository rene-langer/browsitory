use std::collections::HashMap;
use std::sync::Arc;

use git2::{BranchType, ObjectType, Oid, ReferenceType, Repository, Sort};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("failed to read commit graph: {0}")]
    Read(#[from] git2::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCommit {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub parent_ids: Vec<String>,
    pub branch_refs: Vec<String>,
    pub remote_branch_refs: Vec<String>,
    /// Tag names (without "refs/tags/") whose target peels to this commit, sorted.
    pub tags: Vec<String>,
    /// Only on a merge commit of a first-parent walk ([`GraphSelection::walks_first_parent`]):
    /// how many commits its other parents bring in, the rows [`merged_commits`] lists (#617).
    /// `None` everywhere else, also on the rows of search, file history and `merged_commits`.
    pub merged_count: Option<usize>,
}

/// Local/remote branch names and tag names keyed by the commit they point at — the badges on
/// graph rows (and file history / search results).
pub(crate) struct RefDecorations {
    local: HashMap<Oid, Vec<String>>,
    remote: HashMap<Oid, Vec<String>>,
    tags: HashMap<Oid, Vec<String>>,
}

impl RefDecorations {
    pub(crate) fn load(repo: &Repository) -> Result<Self, git2::Error> {
        let mut local: HashMap<Oid, Vec<String>> = HashMap::new();
        for entry in repo.branches(Some(BranchType::Local))? {
            let (branch, _) = entry?;
            let Ok(Some(name)) = branch.name() else {
                continue;
            };
            if let Some(oid) = branch.get().target() {
                local.entry(oid).or_default().push(name.to_string());
            }
        }

        let mut remote: HashMap<Oid, Vec<String>> = HashMap::new();
        for entry in repo.branches(Some(BranchType::Remote))? {
            let (branch, _) = entry?;
            // Symbolic refs like `origin/HEAD` point at another ref rather than a commit
            // directly and would otherwise duplicate whatever branch they alias — skip them so
            // they don't produce a fake "origin/HEAD" badge on every default-branch commit.
            if branch.get().kind() != Some(ReferenceType::Direct) {
                continue;
            }
            let Ok(Some(name)) = branch.name() else {
                continue;
            };
            if let Some(oid) = branch.get().target() {
                remote.entry(oid).or_default().push(name.to_string());
            }
        }
        let mut tags: HashMap<Oid, Vec<String>> = HashMap::new();
        for reference in repo.references_glob("refs/tags/*")? {
            let reference = reference?;
            let Ok(full_name) = reference.name() else {
                continue;
            };
            let Some(name) = full_name.strip_prefix("refs/tags/") else {
                continue;
            };
            // Lightweight and (nested) annotated tags alike; a tag on a tree or blob has no row.
            let Ok(commit) = reference.peel_to_commit() else {
                continue;
            };
            tags.entry(commit.id()).or_default().push(name.to_string());
        }
        for names in tags.values_mut() {
            names.sort();
        }
        Ok(Self {
            local,
            remote,
            tags,
        })
    }

    /// The badges of `oid`: local branches, remote branches, tags.
    fn badges(&self, oid: &Oid) -> [Option<&Vec<String>>; 3] {
        [
            self.local.get(oid),
            self.remote.get(oid),
            self.tags.get(oid),
        ]
    }

    pub(crate) fn graph_commit(&self, commit: &git2::Commit) -> GraphCommit {
        let oid = commit.id();
        let id = oid.to_string();
        GraphCommit {
            short_id: id[..7].to_string(),
            summary: commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string(),
            author_name: commit.author().name().ok().unwrap_or_default().to_string(),
            author_email: commit.author().email().ok().unwrap_or_default().to_string(),
            timestamp: commit.time().seconds(),
            parent_ids: commit.parent_ids().map(|p| p.to_string()).collect(),
            branch_refs: self.local.get(&oid).cloned().unwrap_or_default(),
            remote_branch_refs: self.remote.get(&oid).cloned().unwrap_or_default(),
            tags: self.tags.get(&oid).cloned().unwrap_or_default(),
            merged_count: None,
            id,
        }
    }
}

/// What the graph, commit search and "Go to" keep between calls (#835): the order of the
/// walks asked for most recently and the ref badges. The worker keeps one per repository; a
/// fresh one (as the plain [`graph_page`], [`crate::search::search_commits`] and
/// [`crate::locate::locate_commit`] use) walks and loads again.
///
/// A cached order is exactly the walk libgit2 would sort again: a walk depends only on its
/// start commits, whether it follows first parents, and the parents git reads for each commit,
/// which only the shallow and graft files can change (commits never change). Those make up its
/// key, with the repository's folder. The badges are keyed by every branch, remote branch and
/// tag with what it points at (a tag object never changes either).
#[derive(Default)]
pub struct WalkCache {
    /// Most recently used first, at most [`WALKS_KEPT`], each with its cursor fingerprint.
    walks: Vec<(WalkKey, String, Arc<[Oid]>)>,
    /// The badges, with their [`GraphPage::badges`] version.
    decorations: Option<(RefsKey, String, Arc<RefDecorations>)>,
    /// The badges before them, which a refresh compares rows it keeps against (#835).
    previous_decorations: Option<(String, Arc<RefDecorations>)>,
    stats: WalkCacheStats,
}

/// How many walk orders a [`WalkCache`] keeps: the graph's, a search's with other start points
/// (a `branch:` term, or "Collapse merges", which the search doesn't follow), and the graph's
/// before a ref moved, which a refresh continues from (#835). About 20 bytes a commit each.
const WALKS_KEPT: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
struct WalkKey {
    repository: std::path::PathBuf,
    starts: Vec<Oid>,
    first_parent: bool,
    /// The shallow and the graft file (`None`: no such file), which change parents.
    grafts: [Option<Vec<u8>>; 2],
}

impl WalkKey {
    fn new(repo: &Repository, starts: &[Oid], first_parent: bool) -> Self {
        let common = repo.commondir();
        let read = |path: std::path::PathBuf| std::fs::read(path).ok();
        Self {
            repository: common.to_path_buf(),
            starts: starts.to_vec(),
            first_parent,
            grafts: [
                read(common.join("shallow")),
                read(common.join("info").join("grafts")),
            ],
        }
    }
}

/// How often a [`WalkCache`] did the work it saves: for tests and measurements.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WalkCacheStats {
    /// Full walks sorted (`TOPOLOGICAL | TIME`).
    pub walks: usize,
    /// Times every tag was peeled for the badges.
    pub decorations: usize,
}

impl WalkCache {
    pub fn stats(&self) -> WalkCacheStats {
        self.stats
    }

    /// Every commit of the walk from `starts` (`TOPOLOGICAL | TIME`, first parents only with
    /// `first_parent`), in order; `None` when `cancel` returned `true`. `cancel` is polled
    /// once every [`CANCEL_INTERVAL`] commits while a walk that isn't cached is sorted.
    pub(crate) fn order(
        &mut self,
        repo: &Repository,
        starts: &[Oid],
        first_parent: bool,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Option<Arc<[Oid]>>, git2::Error> {
        let key = WalkKey::new(repo, starts, first_parent);
        if let Some(index) = self.walks.iter().position(|(cached, _, _)| *cached == key) {
            let entry = self.walks.remove(index);
            let order = Arc::clone(&entry.2);
            self.walks.insert(0, entry);
            return Ok(Some(order));
        }
        self.stats.walks += 1;
        let Some(order) = walk_order(repo, starts, first_parent, cancel)? else {
            return Ok(None);
        };
        let fingerprint = walk_fingerprint(starts, first_parent)?;
        self.walks.insert(0, (key, fingerprint, Arc::clone(&order)));
        self.walks.truncate(WALKS_KEPT);
        Ok(Some(order))
    }

    /// The ref badges as they are now; read again only when a branch, remote branch or tag
    /// was added, removed or moved.
    pub(crate) fn decorations(
        &mut self,
        repo: &Repository,
    ) -> Result<Arc<RefDecorations>, git2::Error> {
        Ok(self.versioned_decorations(repo)?.1)
    }

    /// The cached walk whose cursor fingerprint is `fingerprint`, with whether it follows first
    /// parents only.
    fn walk_of(&self, fingerprint: &str) -> Option<(bool, Arc<[Oid]>)> {
        self.walks
            .iter()
            .find(|(_, print, _)| print == fingerprint)
            .map(|(key, _, order)| (key.first_parent, Arc::clone(order)))
    }

    /// The cached badges of version `version`.
    fn decorations_of(&self, version: &str) -> Option<Arc<RefDecorations>> {
        let current = self
            .decorations
            .as_ref()
            .map(|(_, cached, decorations)| (cached, decorations));
        let previous = self
            .previous_decorations
            .as_ref()
            .map(|(cached, decorations)| (cached, decorations));
        [current, previous]
            .into_iter()
            .flatten()
            .find(|(cached, _)| *cached == version)
            .map(|(_, decorations)| Arc::clone(decorations))
    }

    /// [`Self::decorations`] with their version: a hash of the refs they were read from.
    fn versioned_decorations(
        &mut self,
        repo: &Repository,
    ) -> Result<(String, Arc<RefDecorations>), git2::Error> {
        let key = refs_key(repo)?;
        if let Some((cached, version, decorations)) = &self.decorations {
            if *cached == key {
                return Ok((version.clone(), Arc::clone(decorations)));
            }
        }
        self.stats.decorations += 1;
        let version = refs_version(&key);
        let decorations = Arc::new(RefDecorations::load(repo)?);
        let replaced = self
            .decorations
            .replace((key, version.clone(), Arc::clone(&decorations)));
        if let Some((_, version, decorations)) = replaced {
            self.previous_decorations = Some((version, decorations));
        }
        Ok((version, decorations))
    }
}

/// Every reference under `refs/heads/`, `refs/remotes/` and `refs/tags/` (name, target id,
/// symbolic target), sorted.
type RefsKey = Vec<(Vec<u8>, Option<Oid>, Option<Vec<u8>>)>;

/// A version of the badges read from `key`: its hash, as 16 hex digits.
fn refs_version(key: &RefsKey) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Every reference under `refs/heads/`, `refs/remotes/` and `refs/tags/` with
/// what it points at (an id, or a ref for a symbolic one): what [`RefDecorations`] reads.
fn refs_key(repo: &Repository) -> Result<RefsKey, git2::Error> {
    let mut references = Vec::new();
    for reference in repo.references()? {
        let reference = reference?;
        let name = reference.name_bytes();
        if !(name.starts_with(b"refs/heads/")
            || name.starts_with(b"refs/remotes/")
            || name.starts_with(b"refs/tags/"))
        {
            continue;
        }
        references.push((
            name.to_vec(),
            reference.target(),
            reference.symbolic_target_bytes().map(<[u8]>::to_vec),
        ));
    }
    references.sort();
    Ok(references)
}

/// While a walk is sorted into a [`WalkCache`], its `cancel` is polled once every this many
/// commits.
pub(crate) const CANCEL_INTERVAL: usize = 1024;

/// The whole walk from `starts`, as [`WalkCache::order`] describes it.
fn walk_order(
    repo: &Repository,
    starts: &[Oid],
    first_parent: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<Arc<[Oid]>>, git2::Error> {
    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    if first_parent {
        revwalk.simplify_first_parent()?;
    }
    for oid in starts {
        revwalk.push(*oid)?;
    }
    let mut order = Vec::new();
    for (index, oid) in revwalk.enumerate() {
        if index % CANCEL_INTERVAL == 0 && cancel() {
            return Ok(None);
        }
        order.push(oid?);
    }
    Ok(Some(order.into()))
}

/// The branches the graph walks, and a commit search without a `branch:` term (#385).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphSelection {
    /// Local branch names: `None` (nothing saved) walks every local branch, `Some(vec![])`
    /// none of them.
    pub local: Option<Vec<String>>,
    /// Remote-tracking branches as `<remote>/<branch>`, walked on top of `local`. Empty (also
    /// what a never-saved remote selection maps to) walks no remote branch, so remote-only
    /// commits stay out of the default graph.
    pub remote: Vec<String>,
    /// "Collapse merges" (#617): the graph and "Go to" follow first parents from every selected
    /// tip, whatever the selection. Commit search ignores it (see [`Self::isolates_one_branch`]).
    pub first_parent: bool,
}

impl GraphSelection {
    /// True when the graph walk follows first parents only: "Collapse merges" is on, or exactly
    /// one branch is selected, local and remote together (what "Isolate branch" produces, #380).
    /// Each selected branch's line is then its merge commits plus the commits made directly on
    /// it; the commits of merged side branches are left out, and each merge row carries
    /// [`GraphCommit::merged_count`]. Otherwise every ancestor is walked.
    pub(crate) fn walks_first_parent(&self) -> bool {
        self.first_parent || self.isolates_one_branch()
    }

    /// Exactly one branch is selected (local and remote together). Commit search follows first
    /// parents only then, not for "Collapse merges": a search still finds the commits a
    /// collapsed merge holds, and "Go to" one of them expands its merge (#617).
    pub(crate) fn isolates_one_branch(&self) -> bool {
        matches!(&self.local, Some(local) if local.len() + self.remote.len() == 1)
    }

    /// The commits the walk starts from, sorted and deduplicated: the selected branches plus a
    /// detached HEAD (#467). A persisted selection can
    /// outlive its branches (deleted or renamed, possibly outside the app, or a remote branch
    /// pruned by a fetch): missing names are skipped instead of failing the walk (#97).
    pub(crate) fn start_oids(&self, repo: &Repository) -> Result<Vec<Oid>, git2::Error> {
        let named = |refname: &str| -> Result<Option<Oid>, git2::Error> {
            match repo.find_reference(refname) {
                Ok(reference) => Ok(reference.peel_to_commit().ok().map(|c| c.id())),
                // A name that isn't a valid ref name can't resolve either.
                Err(error)
                    if matches!(
                        error.code(),
                        git2::ErrorCode::NotFound | git2::ErrorCode::InvalidSpec
                    ) =>
                {
                    Ok(None)
                }
                Err(error) => Err(error),
            }
        };
        let mut oids = Vec::new();
        match &self.local {
            Some(names) => {
                for name in names {
                    oids.extend(named(&format!("refs/heads/{name}"))?);
                }
            }
            // A repo with no commits yet has no local branches: the walk is simply empty, no
            // special case for an unborn HEAD.
            None => {
                for reference in repo.references_glob("refs/heads/*")? {
                    if let Ok(commit) = reference?.peel_to_commit() {
                        oids.push(commit.id());
                    }
                }
            }
        }
        for name in &self.remote {
            oids.extend(named(&format!("refs/remotes/{name}"))?);
        }
        // A detached HEAD is what the user is on, whatever the selection: its commits may be on
        // no branch at all (a commit made after checking out a tag or a commit, a submodule's
        // checkout, a rebase in progress), and would otherwise be missing (#467). An attached
        // HEAD is a branch, walked when that branch is selected.
        if repo.head_detached().unwrap_or(false) {
            if let Ok(commit) = repo.head().and_then(|head| head.peel_to_commit()) {
                oids.push(commit.id());
            }
        }
        oids.sort();
        oids.dedup();
        Ok(oids)
    }
}

/// The commits of `selection`, children before parents (`TOPOLOGICAL | TIME`), at most
/// `limit`: the first page of [`graph_page`].
pub fn graph_log(
    repo: &Repository,
    limit: usize,
    selection: &GraphSelection,
) -> Result<Vec<GraphCommit>, GraphError> {
    Ok(graph_page(repo, selection, None, limit)?.commits)
}

/// One page of the graph (#685).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphPage {
    pub commits: Vec<GraphCommit>,
    /// Resumes right after the last commit of this page; `None` once the walk is done.
    pub next_cursor: Option<String>,
    /// The cursor came from another walk (a start point moved, or the selection or "Collapse
    /// merges" changed): `commits` is empty, and the caller loads the graph from the start.
    pub stale: bool,
    /// The walk the rows come from (its cursor fingerprint), for [`LoadedGraph::walk`].
    pub walk: String,
    /// The version of the rows' badges (empty on a stale page), for [`LoadedGraph::badges`].
    pub badges: String,
    /// Only from [`graph_refresh_cached`], when it could continue the rows already loaded:
    /// how the refreshed rows are made of `commits` and those rows. `None`: `commits` are the
    /// rows.
    pub refresh: Option<GraphRefresh>,
}

/// The rows a client has loaded from the graph (#835), as a refresh passes them back: the
/// first `rows` rows of the walk `walk`, with the badges of version `badges` (both from the
/// [`GraphPage`]s they came from).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedGraph {
    pub walk: String,
    pub badges: String,
    pub rows: usize,
}

/// How a refresh's rows are made (#835): `layout` in order, each segment either the next rows
/// of [`GraphPage::commits`] or a run of the rows already loaded; then `badges` replaces the
/// badges of kept rows whose branches or tags changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRefresh {
    pub layout: Vec<GraphSegment>,
    pub badges: Vec<RowBadges>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphSegment {
    /// The next `count` rows of [`GraphPage::commits`].
    New { count: usize },
    /// The loaded rows `from..from + count`, unchanged but for [`GraphRefresh::badges`].
    Kept { from: usize, count: usize },
}

/// A kept row's badges as they are now (see [`GraphCommit`]'s fields of the same names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowBadges {
    pub id: String,
    pub branch_refs: Vec<String>,
    pub remote_branch_refs: Vec<String>,
    pub tags: Vec<String>,
}

/// The commits of `selection`, children before parents (`TOPOLOGICAL | TIME`), at most `limit`,
/// from `cursor` on (`None`: from the first commit). With "Collapse merges" or exactly one
/// selected branch the walk follows first parents only (see
/// [`GraphSelection::walks_first_parent`]): a merge commit still lists all its `parent_ids`, so
/// its other parents are usually not in the result, and it carries its
/// [`GraphCommit::merged_count`].
///
/// The cursor is `<offset>:<fingerprint>`, as commit search's (#685): resuming starts at
/// `offset` in the walk's order (sorted again here; [`graph_page_cached`] keeps it), so only the
/// page's own rows are built and sent. A cursor whose
/// fingerprint isn't this walk's, or that doesn't parse, answers [`GraphPage::stale`].
pub fn graph_page(
    repo: &Repository,
    selection: &GraphSelection,
    cursor: Option<&str>,
    limit: usize,
) -> Result<GraphPage, GraphError> {
    graph_page_cached(repo, &mut WalkCache::default(), selection, cursor, limit)
}

/// [`graph_page`], with the walk order and the badges kept in `cache` (#835): a later page of
/// the same walk only builds its own rows.
pub fn graph_page_cached(
    repo: &Repository,
    cache: &mut WalkCache,
    selection: &GraphSelection,
    cursor: Option<&str>,
    limit: usize,
) -> Result<GraphPage, GraphError> {
    let starts = selection.start_oids(repo)?;
    let first_parent = selection.walks_first_parent();
    let fingerprint = walk_fingerprint(&starts, first_parent)?;
    let offset = match cursor {
        None => 0,
        Some(cursor) => match parse_cursor(cursor, &fingerprint) {
            Some(offset) => offset,
            None => {
                return Ok(GraphPage {
                    commits: Vec::new(),
                    next_cursor: None,
                    stale: true,
                    walk: fingerprint,
                    badges: String::new(),
                    refresh: None,
                })
            }
        },
    };
    let (badges, decorations) = cache.versioned_decorations(repo)?;
    let order = cache
        .order(repo, &starts, first_parent, &|| false)?
        .expect("a walk that is never cancelled");

    let page = order.get(offset..).unwrap_or_default();
    let page = &page[..page.len().min(limit)];
    let mut commits = Vec::with_capacity(page.len());
    for oid in page {
        commits.push(graph_row(repo, &decorations, *oid, first_parent)?);
    }
    let next = offset + commits.len();
    let next_cursor = (next < order.len()).then(|| format!("{next}:{fingerprint}"));
    Ok(GraphPage {
        commits,
        next_cursor,
        stale: false,
        walk: fingerprint,
        badges,
        refresh: None,
    })
}

/// The graph row of `oid`; with `first_parent`, a merge's carries its merged count.
fn graph_row(
    repo: &Repository,
    decorations: &RefDecorations,
    oid: Oid,
    first_parent: bool,
) -> Result<GraphCommit, git2::Error> {
    let commit = repo.find_commit(oid)?;
    let mut row = decorations.graph_commit(&commit);
    if first_parent && commit.parent_count() > 1 {
        row.merged_count = Some(merged_walk(repo, &commit)?.count());
    }
    Ok(row)
}

/// The first page of `selection` for a client that has `loaded` rows already (#835): when
/// `cache` still holds the walk and the badges those rows came from, only the rows that are new
/// to the client are sent, with [`GraphPage::refresh`] saying where the loaded rows go and
/// which of their badges changed. The refreshed rows are the walk's first rows, as many as
/// reach past the last loaded row that is still in the walk, and at least `limit`. Otherwise
/// (another worker, a cache that moved on) it is an ordinary first page of at least
/// `loaded.rows` rows.
pub fn graph_refresh_cached(
    repo: &Repository,
    cache: &mut WalkCache,
    selection: &GraphSelection,
    loaded: &LoadedGraph,
    limit: usize,
) -> Result<GraphPage, GraphError> {
    let starts = selection.start_oids(repo)?;
    let first_parent = selection.walks_first_parent();
    // Looked up before this read can push them out of the cache.
    let old_walk = cache.walk_of(&loaded.walk);
    let old_decorations = cache.decorations_of(&loaded.badges);
    let (Some((old_first_parent, old)), Some(old_decorations)) = (old_walk, old_decorations) else {
        return graph_page_cached(repo, cache, selection, None, limit.max(loaded.rows));
    };
    if old_first_parent != first_parent {
        // The kept rows' merge counts would be another walk's.
        return graph_page_cached(repo, cache, selection, None, limit.max(loaded.rows));
    }
    let fingerprint = walk_fingerprint(&starts, first_parent)?;
    let (badges, decorations) = cache.versioned_decorations(repo)?;
    let order = cache
        .order(repo, &starts, first_parent, &|| false)?
        .expect("a walk that is never cancelled");

    let loaded_rows = &old[..loaded.rows.min(old.len())];
    let loaded_at: HashMap<Oid, usize> = loaded_rows
        .iter()
        .enumerate()
        .map(|(row, oid)| (*oid, row))
        .collect();
    let last_loaded = order.iter().rposition(|oid| loaded_at.contains_key(oid));
    let rows = last_loaded
        .map_or(0, |last| last + 1)
        .max(limit.min(order.len()));

    let mut layout: Vec<GraphSegment> = Vec::new();
    let mut commits = Vec::new();
    let mut changed = Vec::new();
    for oid in &order[..rows] {
        match loaded_at.get(oid) {
            Some(&row) => {
                match layout.last_mut() {
                    Some(GraphSegment::Kept { from, count }) if *from + *count == row => {
                        *count += 1;
                    }
                    _ => layout.push(GraphSegment::Kept {
                        from: row,
                        count: 1,
                    }),
                }
                let now = decorations.badges(oid);
                if old_decorations.badges(oid) != now {
                    let [local, remote, tags] = now.map(|names| names.cloned().unwrap_or_default());
                    changed.push(RowBadges {
                        id: oid.to_string(),
                        branch_refs: local,
                        remote_branch_refs: remote,
                        tags,
                    });
                }
            }
            None => {
                match layout.last_mut() {
                    Some(GraphSegment::New { count }) => *count += 1,
                    _ => layout.push(GraphSegment::New { count: 1 }),
                }
                commits.push(graph_row(repo, &decorations, *oid, first_parent)?);
            }
        }
    }
    let next_cursor = (rows < order.len()).then(|| format!("{rows}:{fingerprint}"));
    Ok(GraphPage {
        commits,
        next_cursor,
        stale: false,
        walk: fingerprint,
        badges,
        refresh: Some(GraphRefresh {
            layout,
            badges: changed,
        }),
    })
}

/// The offset of `cursor` when it belongs to the walk `fingerprint` names.
fn parse_cursor(cursor: &str, fingerprint: &str) -> Option<usize> {
    let (offset, print) = cursor.rsplit_once(':')?;
    if print != fingerprint {
        return None;
    }
    offset.parse().ok()
}

/// First 16 hex chars of the blob hash of the sorted, comma-joined start oids.
/// Identifies a walk: its start oids and whether it follows first parents only, so a cursor from
/// one walk never resumes the other (one selected branch and two branches at the same commit
/// share their start oids but not their walk). Start oids and first-parent mode determine the
/// walk, so a remote branch added to or dropped from the selection (#385) changes the
/// fingerprint exactly when it changes the walk. Graph and search cursors both use it.
pub(crate) fn walk_fingerprint(starts: &[Oid], first_parent: bool) -> Result<String, git2::Error> {
    let mut joined = starts
        .iter()
        .map(|o| o.to_string())
        .collect::<Vec<_>>()
        .join(",");
    if first_parent {
        joined.push_str(";first-parent");
    }
    let hash = Oid::hash_object(ObjectType::Blob, joined.as_bytes())?;
    Ok(hash.to_string()[..16].to_string())
}

/// The commits a merge brought in, children before parents (`TOPOLOGICAL | TIME`), at most
/// `limit`: those reachable from its other parents but not from its first parent
/// (`first..other`, every other parent of an octopus merge together), side branches merged
/// into the side branch included (#617). What expanding a collapsed merge shows below it; their
/// number is the merge's [`GraphCommit::merged_count`]. Empty for a commit that is no merge.
pub fn merged_commits(
    repo: &Repository,
    merge_id: &str,
    limit: usize,
) -> Result<Vec<GraphCommit>, GraphError> {
    merged_commits_cached(repo, &mut WalkCache::default(), merge_id, limit)
}

/// [`merged_commits`], with the badges kept in `cache` (#835).
pub fn merged_commits_cached(
    repo: &Repository,
    cache: &mut WalkCache,
    merge_id: &str,
    limit: usize,
) -> Result<Vec<GraphCommit>, GraphError> {
    let merge = repo.find_commit(Oid::from_str(merge_id)?)?;
    if merge.parent_count() < 2 {
        return Ok(Vec::new());
    }
    let decorations = cache.decorations(repo)?;
    let mut revwalk = merged_walk(repo, &merge)?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    let mut commits = Vec::new();
    for oid_result in revwalk.take(limit) {
        let commit = repo.find_commit(oid_result?)?;
        commits.push(decorations.graph_commit(&commit));
    }
    Ok(commits)
}

/// An unsorted walk of `first..other` for `merge` (see [`merged_commits`]).
pub(crate) fn merged_walk<'r>(
    repo: &'r Repository,
    merge: &git2::Commit,
) -> Result<git2::Revwalk<'r>, git2::Error> {
    let mut revwalk = repo.revwalk()?;
    let mut parents = merge.parent_ids();
    if let Some(first) = parents.next() {
        revwalk.hide(first)?;
    }
    for parent in parents {
        revwalk.push(parent)?;
    }
    Ok(revwalk)
}

/// Full raw message (subject plus body) of `commit_id`, for the commit header.
pub fn commit_message(repo: &Repository, commit_id: &str) -> Result<String, GraphError> {
    let commit = repo.find_commit(Oid::from_str(commit_id)?)?;
    Ok(String::from_utf8_lossy(commit.message_bytes()).into_owned())
}
