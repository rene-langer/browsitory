//! What the worker knows about a paused operation: the commands it refuses meanwhile, whether
//! one is in progress, and a marker that names it for the undo recorder.
use std::sync::mpsc::Sender;

use git2::{Repository, RepositoryState};
use git_core::rebase::RebaseState;

use super::rebase_store::RebaseStore;
use super::sequence_store::SequenceStore;
use super::Command;

/// Replaces a command that must not run while a sequence or a rebase is stored with a no-op,
/// after replying with git-core's `OperationInProgress` text on its own reply. Refused: anything
/// that starts another operation or moves HEAD or the index under it (picks, merge, rebase,
/// commit and amend, reset, checkout, branch switch, pull, stash apply/pop, reflog restore).
/// Read-only commands, fetch, staging and conflict resolution stay available.
pub(super) fn gate_during_operation(command: Command) -> Command {
    fn refuse<T>(reply: Sender<Result<T, String>>) -> Command {
        let _ = reply.send(Err(
            git_core::pick::PickError::OperationInProgress.to_string()
        ));
        let (reply, _) = std::sync::mpsc::channel();
        Command::Ping { reply }
    }
    match command {
        Command::CherryPick { reply, .. } | Command::RevertCommit { reply, .. } => refuse(reply),
        Command::CherryPickMany { reply, .. } | Command::RevertMany { reply, .. } => refuse(reply),
        Command::StartMerge { reply, .. } => refuse(reply),
        Command::StartRebase { reply, .. } => refuse(reply),
        Command::Commit { reply, .. } | Command::AmendCommit { reply, .. } => refuse(reply),
        Command::ResetToCommit { reply, .. }
        | Command::CheckoutCommit { reply, .. }
        | Command::SwitchBranch { reply, .. }
        | Command::RestoreReflogEntry { reply, .. } => refuse(reply),
        Command::PullCurrentUpstream { reply, .. } => refuse(reply),
        Command::ApplyStash { reply, .. } | Command::PopStash { reply, .. } => refuse(reply),
        other => other,
    }
}

/// The stored rebase: this session's copy when saving it failed (newer than the file), else
/// `rebase.json`.
pub(super) fn stored_rebase(
    repo: &Repository,
    store: &RebaseStore,
    fallback: &Option<RebaseState>,
) -> Option<RebaseState> {
    fallback.clone().or_else(|| store.load(repo))
}

/// Looks at the stores and git's own state, fresh on every call (another process may share the
/// repository).
pub(super) struct OperationProbe<'a> {
    pub rebase: &'a RebaseStore,
    pub fallback: &'a Option<RebaseState>,
    pub sequence: &'a SequenceStore,
}

impl OperationProbe<'_> {
    pub fn rebase(&self, repo: &Repository) -> Option<RebaseState> {
        stored_rebase(repo, self.rebase, self.fallback)
    }

    /// A rebase or a sequence is stored, so `gate_during_operation` applies.
    pub fn stored_operation(&self, repo: &Repository) -> bool {
        self.rebase(repo).is_some() || self.sequence.load(repo).is_some()
    }

    /// Names the paused operation, constant for its whole life, so a pending undo entry is only
    /// ever recorded by the operation it was taken for (across restarts and processes). The
    /// stored rebase and sequence come first: their `CHERRY_PICK_HEAD` changes from step to step.
    /// `None` when nothing is in progress (so `marker(..).is_some()` is "in progress").
    pub fn marker(&self, repo: &Repository) -> Option<String> {
        if let Some(rebase) = self.rebase(repo) {
            return Some(format!(
                "rebase {} {} {}",
                rebase.original_branch_ref, rebase.original_tip, rebase.onto
            ));
        }
        if let Some(sequence) = self.sequence.load(repo) {
            return Some(format!(
                "sequence {:?} {} {} {}",
                sequence.kind,
                sequence.original_head,
                sequence.commits.first().map(String::as_str).unwrap_or(""),
                sequence.commits.len()
            ));
        }
        let head = || {
            repo.head()
                .ok()
                .and_then(|head| head.target())
                .map_or_else(|| "unborn".to_string(), |oid| oid.to_string())
        };
        let target = |name: &str| {
            repo.find_reference(name)
                .ok()
                .and_then(|reference| reference.target())
                .map_or_else(String::new, |oid| oid.to_string())
        };
        match repo.state() {
            RepositoryState::Clean => None,
            RepositoryState::Merge => {
                let heads = std::fs::read_to_string(repo.path().join("MERGE_HEAD"))
                    .unwrap_or_default()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(",");
                Some(format!("merge {} {heads}", head()))
            }
            RepositoryState::CherryPick | RepositoryState::CherryPickSequence => Some(format!(
                "cherry-pick {} {}",
                head(),
                target("CHERRY_PICK_HEAD")
            )),
            RepositoryState::Revert | RepositoryState::RevertSequence => {
                Some(format!("revert {} {}", head(), target("REVERT_HEAD")))
            }
            other => Some(format!("git {other:?} {}", head())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::pick::{OriginalRef, SequenceKind, SequenceState};
    use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};

    fn init() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
            config.set_bool("commit.gpgSign", false).unwrap();
        }
        (dir, repo)
    }

    fn commit(repo: &Repository, path: &str, contents: &str, message: &str) -> git2::Oid {
        std::fs::write(repo.workdir().unwrap().join(path), contents).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(path)).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap()
    }

    fn marker(repo: &Repository, rebase: &RebaseStore, sequence: &SequenceStore) -> Option<String> {
        OperationProbe {
            rebase,
            fallback: &None,
            sequence,
        }
        .marker(repo)
    }

    #[test]
    fn marker_is_stable_for_the_whole_operation() {
        // A rebase paused at two edit steps in turn.
        let (_dir, repo) = init();
        let onto = commit(&repo, "base.txt", "base\n", "base");
        let a = commit(&repo, "a.txt", "a\n", "a");
        let b = commit(&repo, "b.txt", "b\n", "b");
        let edit = |id: git2::Oid| RebasePlanEntry {
            commit_id: id.to_string(),
            action: RebaseAction::Edit,
            combined_message: None,
        };
        let (rebase_store, sequence_store) = (RebaseStore::open(&repo), SequenceStore::open(&repo));
        assert_eq!(marker(&repo, &rebase_store, &sequence_store), None);
        let (mut state, step) =
            git_core::rebase::start_rebase(&repo, &onto.to_string(), vec![edit(a), edit(b)])
                .unwrap();
        assert_eq!(step, RebaseStepResult::PausedForEdit);
        rebase_store.save(&state).unwrap();
        let first = marker(&repo, &rebase_store, &sequence_store).unwrap();
        assert!(first.starts_with("rebase refs/heads/"), "{first}");
        assert_eq!(
            git_core::rebase::rebase_continue(&repo, &mut state).unwrap(),
            RebaseStepResult::PausedForEdit
        );
        rebase_store.save(&state).unwrap();
        assert_eq!(marker(&repo, &rebase_store, &sequence_store), Some(first));
        git_core::rebase::abort_rebase(&repo, &state).unwrap();
        rebase_store.clear().unwrap();

        // A sequence at two steps.
        let sequence = |next: usize, expected_head: git2::Oid| SequenceState {
            kind: SequenceKind::CherryPick,
            commits: vec![a.to_string(), b.to_string()],
            next,
            skipped: vec![],
            original_head: onto.to_string(),
            expected_head: expected_head.to_string(),
            stopped_on_conflict: true,
            original_ref: OriginalRef::Branch("refs/heads/main".into()),
        };
        sequence_store.save(&sequence(0, onto)).unwrap();
        let first = marker(&repo, &rebase_store, &sequence_store).unwrap();
        assert!(first.starts_with("sequence CherryPick"), "{first}");
        sequence_store.save(&sequence(1, a)).unwrap();
        assert_eq!(marker(&repo, &rebase_store, &sequence_store), Some(first));
        sequence_store.clear().unwrap();

        // A merge: staging during it doesn't change the marker.
        std::fs::write(repo.path().join("MERGE_HEAD"), format!("{a}\n")).unwrap();
        let head = repo.head().unwrap().target().unwrap();
        let merge = marker(&repo, &rebase_store, &sequence_store).unwrap();
        assert_eq!(merge, format!("merge {head} {a}"));
        std::fs::write(repo.workdir().unwrap().join("c.txt"), "c\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("c.txt")).unwrap();
        index.write().unwrap();
        assert_eq!(marker(&repo, &rebase_store, &sequence_store), Some(merge));
        repo.cleanup_state().unwrap();
        assert_eq!(marker(&repo, &rebase_store, &sequence_store), None);
    }

    #[test]
    fn marker_prefers_the_stored_rebase_over_its_cherry_pick_state() {
        let (_dir, repo) = init();
        let onto = commit(&repo, "base.txt", "base\n", "base");
        let a = commit(&repo, "a.txt", "a\n", "a");
        let plan = vec![RebasePlanEntry {
            commit_id: a.to_string(),
            action: RebaseAction::Edit,
            combined_message: None,
        }];
        let (state, _) = git_core::rebase::start_rebase(&repo, &onto.to_string(), plan).unwrap();
        let (rebase_store, sequence_store) = (RebaseStore::open(&repo), SequenceStore::open(&repo));
        assert_eq!(repo.state(), RepositoryState::CherryPick);
        assert!(marker(&repo, &rebase_store, &sequence_store)
            .unwrap()
            .starts_with("cherry-pick "));

        rebase_store.save(&state).unwrap();

        assert_eq!(
            marker(&repo, &rebase_store, &sequence_store),
            Some(format!(
                "rebase {} {} {}",
                state.original_branch_ref, state.original_tip, onto
            ))
        );
        // The session copy of a rebase whose save failed counts too.
        rebase_store.clear().unwrap();
        let probe = OperationProbe {
            rebase: &rebase_store,
            fallback: &Some(state),
            sequence: &sequence_store,
        };
        assert!(probe.marker(&repo).unwrap().starts_with("rebase "));
        assert!(probe.stored_operation(&repo));
    }
}
