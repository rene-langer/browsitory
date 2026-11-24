//! Reachability counts over many refs (#1067).
use std::collections::{BinaryHeap, HashMap, HashSet};

use git2::{Oid, Repository};

/// How many more commits the walk takes once only hidden ones are queued, as git's and
/// libgit2's `SLOP`: a commit dated before its parent (a skewed clock) can still reach an
/// interesting one a few steps later.
const SLOP: u32 = 5;

struct Node {
    time: i64,
    /// Set once the commit is taken from the queue; its parents are then in `nodes` too.
    parents: Option<Vec<Oid>>,
    hidden: bool,
    queued: bool,
}

/// The walk's state: git's `limit_list`, with a binary heap for its date-ordered list. libgit2's
/// revwalk keeps that list as a sorted linked list and inserts every hidden tip into it, which
/// grows with the square of the number of refs.
struct Walk<'r> {
    repo: &'r Repository,
    nodes: HashMap<Oid, Node>,
    /// Newest first.
    queue: BinaryHeap<(i64, Oid)>,
    /// Queued commits not (yet) hidden.
    interesting_queued: usize,
}

impl Walk<'_> {
    fn add(&mut self, id: Oid, hidden: bool) -> Result<(), git2::Error> {
        if let Some(node) = self.nodes.get(&id) {
            if hidden && !node.hidden {
                self.hide(id);
            }
            return Ok(());
        }
        let time = self.repo.find_commit(id)?.time().seconds();
        self.nodes.insert(
            id,
            Node {
                time,
                parents: None,
                hidden,
                queued: true,
            },
        );
        self.queue.push((time, id));
        if !hidden {
            self.interesting_queued += 1;
        }
        Ok(())
    }

    /// Hides `id` and every ancestor already walked through it (git's
    /// `mark_parents_uninteresting`); ancestors not walked yet are hidden when taken.
    fn hide(&mut self, id: Oid) {
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get_mut(&id) else {
                continue;
            };
            if node.hidden {
                continue;
            }
            node.hidden = true;
            if node.queued {
                self.interesting_queued -= 1;
            }
            if let Some(parents) = &node.parents {
                stack.extend(parents.iter().copied());
            }
        }
    }

    /// Whether to go on after a hidden commit (git's and libgit2's `still_interesting`):
    /// `date` is the time of the last interesting commit taken.
    fn still_interesting(&self, date: i64, slop: u32) -> u32 {
        let Some(&(newest, _)) = self.queue.peek() else {
            return 0;
        };
        if date <= newest || self.interesting_queued > 0 {
            SLOP
        } else {
            slop - 1
        }
    }
}

/// How many commits `tip` reaches that none of `hidden` does: `git rev-list --count <tip> --not
/// <hidden>…`, with the same stopping rule, so a skewed clock is followed as far as git follows
/// it. Each commit is taken once from a heap, so thousands of hidden refs cost a log factor each,
/// not a pass over all of them.
pub(crate) fn count_unreachable_from(
    repo: &Repository,
    tip: Oid,
    hidden: HashSet<Oid>,
) -> Result<usize, git2::Error> {
    let mut walk = Walk {
        repo,
        nodes: HashMap::new(),
        queue: BinaryHeap::new(),
        interesting_queued: 0,
    };
    walk.add(tip, false)?;
    for id in hidden {
        walk.add(id, true)?;
    }
    let mut taken = Vec::new();
    let mut date = i64::MAX;
    let mut slop = SLOP;
    while let Some((time, id)) = walk.queue.pop() {
        let node = walk.nodes.get_mut(&id).expect("queued commits have a node");
        node.queued = false;
        let hidden = node.hidden;
        if !hidden {
            walk.interesting_queued -= 1;
        }
        let parents: Vec<Oid> = repo.find_commit(id)?.parent_ids().collect();
        node.parents = Some(parents.clone());
        for parent in parents {
            walk.add(parent, hidden)?;
        }
        if hidden {
            slop = walk.still_interesting(date, slop);
            if slop == 0 {
                break;
            }
            continue;
        }
        date = time;
        taken.push(id);
    }
    Ok(taken
        .into_iter()
        .filter(|id| !walk.nodes[id].hidden)
        .count())
}
