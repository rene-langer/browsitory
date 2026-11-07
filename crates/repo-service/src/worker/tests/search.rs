use super::*;
use git_core::graph::GraphSelection;

fn two_commit_repo() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "alpha commit");
    write_file(dir.path(), "b.txt", "b");
    commit_all(&repo, "beta commit");
    (dir, repo)
}

/// Every page of a search, following `next_cursor` the way the frontend does: the worker's
/// pages are time-boxed (`PageLimits::DEFAULT`, 250 ms), so on a slow runner the first page
/// can end after the newest commit, before the match (#182). Returns the matches' summaries
/// and the last page's `scanned`.
fn search_all_pages(handle: &super::super::WorkerHandle, query: &str) -> (Vec<String>, usize) {
    let mut summaries = Vec::new();
    let mut cursor = None;
    loop {
        let page = handle
            .search_commits(query.into(), GraphSelection::default(), cursor.take())
            .unwrap();
        summaries.extend(page.commits.into_iter().map(|c| c.summary));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => return (summaries, page.scanned),
        }
    }
}

#[test]
fn search_commits_round_trips_through_the_worker() {
    let (dir, _repo) = two_commit_repo();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

    let (summaries, scanned) = search_all_pages(&worker.handle(), "alpha");

    assert_eq!(summaries, ["alpha commit"]);
    assert_eq!(scanned, 2);
}

#[test]
fn an_invalid_search_query_replies_its_message() {
    let (dir, _repo) = two_commit_repo();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

    assert_eq!(
        worker
            .handle()
            .search_commits("author:".into(), GraphSelection::default(), None)
            .unwrap_err(),
        "author: needs a value."
    );
}

#[test]
fn a_search_superseded_before_it_runs_replies_cancelled() {
    let (dir, _repo) = two_commit_repo();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    // Claim a generation the way `search_commits` does, then let a newer search/cancel
    // supersede it before the command is even queued.
    let stale = handle.claim_search_generation();
    handle.cancel_search();

    assert_eq!(
        handle
            .search_commits_claimed(stale, "alpha".into(), GraphSelection::default(), None)
            .unwrap_err(),
        "Search cancelled."
    );
    // A fresh search after the cancel runs normally.
    assert_eq!(search_all_pages(&handle, "alpha").0, ["alpha commit"]);
}
