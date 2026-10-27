use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

use git_core::search::{PageLimits, SearchPage};

use super::{Command, SearchGeneration, WorkerHandle};

/// Parses `query` and runs one page of the search. The walk stops with `Search cancelled.` as
/// soon as `current` no longer equals `generation` (a newer search or a cancel claimed it).
///
/// `PageLimits::DEFAULT` time-boxes the page (~250 ms), so a queued commit/stage command never
/// waits long behind an expensive `path:`/`contents:` scan; the frontend pages on through
/// `next_cursor`.
pub(super) fn search_commits(
    repo: &git2::Repository,
    current: &SearchGeneration,
    generation: u64,
    query: String,
    refs: Option<Vec<String>>,
    cursor: Option<String>,
    reply: Sender<Result<SearchPage, String>>,
) {
    let result = git_core::search::parse_query(&query).and_then(|parsed| {
        git_core::search::search_commits(
            repo,
            &parsed,
            refs.as_deref(),
            cursor.as_deref(),
            PageLimits::DEFAULT,
            &|| current.load(Ordering::SeqCst) != generation,
        )
    });
    let _ = reply.send(result.map_err(|error| error.to_string()));
}

impl WorkerHandle {
    /// Runs one page of a commit search. Claims a new `SearchGeneration` before queueing, which
    /// cancels any older search through the shared counter — never through `self.tx`.
    pub fn search_commits(
        &self,
        query: String,
        refs: Option<Vec<String>>,
        cursor: Option<String>,
    ) -> Result<SearchPage, String> {
        let generation = self.search_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::SearchCommits {
                query,
                refs,
                cursor,
                generation,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// Stops the in-flight (or queued) search, if any, by bumping the shared
    /// `SearchGeneration` — never through `self.tx`, which would queue behind the search.
    pub fn cancel_search(&self) {
        self.search_generation.fetch_add(1, Ordering::SeqCst);
    }
}
