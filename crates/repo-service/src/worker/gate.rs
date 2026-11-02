//! What the worker knows about a paused operation: the commands it refuses meanwhile, whether
//! one is in progress, and a marker that names it for the undo recorder.
use std::sync::mpsc::Sender;

use git2::{Repository, RepositoryState};
use git_core::rebase::RebaseState;

use super::rebase_store::RebaseStore;
use super::sequence_store::SequenceStore;
use super::Command;

/// What a stored rebase or cherry-pick/revert sequence protects beyond the commands every paused
/// operation refuses (#171).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GateScope {
    /// Short name of the branch the stored operation will return to or move: the rebased
    /// branch, or a sequence's `OriginalRef::Branch`. Delete, rename and worktree-add on it are
    /// refused.
    pub branch: Option<String>,
    /// Stash save is refused (it would carry a paused step's index away).
    pub refuse_stash_save: bool,
}

/// Replaces a command that must not run while a sequence or a rebase is stored with a no-op,
/// after replying with git-core's `OperationInProgress` text on its own reply. Refused: anything
/// that starts another operation or moves HEAD or the index under it (picks, merge, rebase,
/// commit and amend, reset, checkout, branch switch or create (it checks the branch out), pull,
/// stash apply/pop, reflog restore).
/// Read-only commands, fetch, staging and conflict resolution stay available.
///
/// Deleting or renaming `scope.branch` (the rebased branch, or the branch a sequence runs on),
/// or checking it out in a new worktree, is refused too: git would let it go under the
/// operation once it isn't checked out, and a renamed branch can't be rewound to by Abort. So is
/// a stash save while `scope.refuse_stash_save` (it would carry the paused step away from the
/// index). Other branches stay manageable.
pub(super) fn gate_during_operation(command: Command, scope: &GateScope) -> Command {
    let protected = scope.branch.as_deref();
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
        | Command::CreateBranch { reply, .. }
        | Command::CheckoutCommit { reply, .. }
        | Command::SwitchBranch { reply, .. }
        | Command::RestoreReflogEntry { reply, .. } => refuse(reply),
        Command::PullCurrentUpstream { reply, .. } => refuse(reply),
        Command::ApplyStash { reply, .. } | Command::PopStash { reply, .. } => refuse(reply),
        Command::DeleteBranch { name, reply, .. } if protected == Some(name.as_str()) => {
            refuse(reply)
        }
        Command::RenameBranch {
            old_name, reply, ..
        } if protected == Some(old_name.as_str()) => refuse(reply),
        Command::CreateWorktree { branch, reply, .. } if protected == Some(branch.as_str()) => {
            refuse(reply)
        }
        Command::SaveStash { reply, .. } if scope.refuse_stash_save => refuse(reply),
        other => other,
    }
}

/// What this worker remembers about the stored rebase beyond `rebase.json`, for when the file
/// can't be written: `fallback` is a newer state whose save failed; `forgotten` is a state the
/// rebase ended with but whose file couldn't be deleted, so it must not count any more.
#[derive(Default)]
pub(super) struct RebaseMemory {
    pub fallback: Option<RebaseState>,
    pub forgotten: Option<RebaseState>,
}

/// The stored rebase: this session's copy when saving it failed (newer than the file), else
/// `rebase.json`, unless that is a rebase this session already ended.
pub(super) fn stored_rebase(
    repo: &Repository,
    store: &RebaseStore,
    memory: &RebaseMemory,
) -> Option<RebaseState> {
    if let Some(state) = &memory.fallback {
        return Some(state.clone());
    }
    store
        .load(repo)
        .filter(|state| memory.forgotten.as_ref() != Some(state))
}

/// Looks at the stores and git's own state, fresh on every call (another process may share the
/// repository).
pub(super) struct OperationProbe<'a> {
    pub rebase: &'a RebaseStore,
    pub memory: &'a RebaseMemory,
    pub sequence: &'a SequenceStore,
}

impl OperationProbe<'_> {
    pub fn rebase(&self, repo: &Repository) -> Option<RebaseState> {
        stored_rebase(repo, self.rebase, self.memory)
    }

    /// The short name of the branch a stored rebase is rebasing.
    pub fn rebased_branch(&self, repo: &Repository) -> Option<String> {
        self.rebase(repo).map(|state| {
            state
                .original_branch_ref
                .strip_prefix("refs/heads/")
                .unwrap_or(&state.original_branch_ref)
                .to_string()
        })
    }

    /// What the stored rebase (first) or sequence protects; `None` when neither is stored.
    pub fn gate_scope(&self, repo: &Repository) -> Option<GateScope> {
        if let Some(branch) = self.rebased_branch(repo) {
            return Some(GateScope {
                branch: Some(branch),
                refuse_stash_save: true,
            });
        }
        let sequence = self.sequence.load(repo)?;
        let branch = match &sequence.original_ref {
            git_core::pick::OriginalRef::Branch(name) => {
                Some(name.strip_prefix("refs/heads/").unwrap_or(name).to_string())
            }
            git_core::pick::OriginalRef::Detached | git_core::pick::OriginalRef::Unknown => None,
        };
        Some(GateScope {
            branch,
            refuse_stash_save: true,
        })
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
        // git writes these files once per operation and deletes them when it ends, so their
        // mtime tells an operation aborted outside and started again with the same heads apart
        // from the first one (#172), while staying constant through the pause (staging and
        // resolving don't touch them). Both processes stat the same file, so they agree.
        let written = |name: &str| {
            std::fs::metadata(repo.path().join(name))
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or_else(String::new, |since| format!(" @{}", since.as_nanos()))
        };
        // A terminal `git rebase` or `git am` (#263) moves HEAD at every step, so its marker is
        // what names the operation in git's files instead (#264): the kind, branch, start and
        // onto, and the mtime of a file git writes once when the operation starts
        // (`rebase-merge/orig-head`, `rebase-apply/last`), which tells it from the same rebase
        // aborted and started again. A merge, pick or revert started during it counts as the
        // rebase (libgit2 reports the rebase's state then too).
        if let Some(rebase) = git_core::repo::terminal_rebase(repo) {
            let (folder, identity) = if repo.path().join("rebase-merge").is_dir() {
                ("rebase-merge", "orig-head")
            } else {
                ("rebase-apply", "last")
            };
            // `git am` writes no `rebase-apply/orig-head` (nor `onto`/`head-name`): its marker
            // is the kind and `last`'s mtime alone.
            let started_from = std::fs::read_to_string(repo.path().join(folder).join("orig-head"))
                .map(|text| text.trim().to_string())
                .unwrap_or_default();
            return Some(format!(
                "terminal {:?} {} {} {started_from}{}",
                rebase.kind,
                rebase.branch,
                rebase.onto,
                written(&format!("{folder}/{identity}"))
            ));
        }
        match repo.state() {
            // A terminal `git bisect` alone is nothing in progress, as for git-core (#250).
            // libgit2 reports `Bisect` only when nothing else is paused.
            RepositoryState::Clean | RepositoryState::Bisect => None,
            RepositoryState::Merge => {
                let heads = std::fs::read_to_string(repo.path().join("MERGE_HEAD"))
                    .unwrap_or_default()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(",");
                Some(format!("merge {} {heads}{}", head(), written("MERGE_HEAD")))
            }
            RepositoryState::CherryPick | RepositoryState::CherryPickSequence => Some(format!(
                "cherry-pick {} {}{}",
                head(),
                target("CHERRY_PICK_HEAD"),
                written("CHERRY_PICK_HEAD")
            )),
            RepositoryState::Revert | RepositoryState::RevertSequence => Some(format!(
                "revert {} {}{}",
                head(),
                target("REVERT_HEAD"),
                written("REVERT_HEAD")
            )),
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
            memory: &RebaseMemory::default(),
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
        assert_eq!(
            merge,
            format!("merge {head} {a} @{}", mtime_ns(&repo, "MERGE_HEAD"))
        );
        std::fs::write(repo.workdir().unwrap().join("c.txt"), "c\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("c.txt")).unwrap();
        index.write().unwrap();
        assert_eq!(marker(&repo, &rebase_store, &sequence_store), Some(merge));
        repo.cleanup_state().unwrap();
        assert_eq!(marker(&repo, &rebase_store, &sequence_store), None);
    }

    fn git(repo: &Repository, args: &[&str]) -> std::process::Output {
        test_support::git(repo.workdir().unwrap(), args)
    }

    /// A terminal `git bisect` alone is not an operation in progress (#250): commands are
    /// recorded for undo and Undo/Redo run, as with nothing paused.
    #[test]
    fn a_bisect_alone_has_no_marker() {
        let (_dir, repo) = init();
        commit(&repo, "a.txt", "a\n", "a");
        assert!(git(&repo, &["bisect", "start"]).status.success());
        assert_eq!(repo.state(), RepositoryState::Bisect);
        let (rebase_store, sequence_store) = (RebaseStore::open(&repo), SequenceStore::open(&repo));

        assert_eq!(marker(&repo, &rebase_store, &sequence_store), None);
    }

    /// A merge paused during a bisect is still an operation in progress, with the merge's marker.
    #[test]
    fn a_merge_during_a_bisect_has_the_merge_marker() {
        let (_dir, repo) = init();
        let a = commit(&repo, "a.txt", "a\n", "a");
        commit(&repo, "b.txt", "b\n", "b");
        assert!(git(&repo, &["bisect", "start"]).status.success());
        std::fs::write(repo.path().join("MERGE_HEAD"), format!("{a}\n")).unwrap();
        assert_eq!(repo.state(), RepositoryState::Merge);
        let (rebase_store, sequence_store) = (RebaseStore::open(&repo), SequenceStore::open(&repo));

        let merge = marker(&repo, &rebase_store, &sequence_store).unwrap();

        let head = repo.head().unwrap().target().unwrap();
        assert_eq!(
            merge,
            format!("merge {head} {a} @{}", mtime_ns(&repo, "MERGE_HEAD"))
        );
    }

    fn probe<'a>(
        rebase: &'a RebaseStore,
        memory: &'a RebaseMemory,
        sequence: &'a SequenceStore,
    ) -> OperationProbe<'a> {
        OperationProbe {
            rebase,
            memory,
            sequence,
        }
    }

    /// A sequence of two real commits of `repo`, stopped at its second step.
    fn stored_sequence(repo: &Repository, original_ref: OriginalRef) -> SequenceState {
        let a = commit(repo, "a.txt", "a\n", "a");
        let b = commit(repo, "b.txt", "b\n", "b");
        SequenceState {
            kind: SequenceKind::CherryPick,
            commits: vec![a.to_string(), b.to_string()],
            next: 1,
            skipped: vec![],
            original_head: a.to_string(),
            expected_head: b.to_string(),
            stopped_on_conflict: true,
            original_ref,
        }
    }

    #[test]
    fn gate_scope_for_a_sequence_on_a_branch_names_it_and_refuses_stash_save() {
        let (_dir, repo) = init();
        let (rebase, sequence) = (RebaseStore::open(&repo), SequenceStore::open(&repo));
        sequence
            .save(&stored_sequence(
                &repo,
                OriginalRef::Branch("refs/heads/main".into()),
            ))
            .unwrap();
        let memory = RebaseMemory::default();
        assert_eq!(
            probe(&rebase, &memory, &sequence).gate_scope(&repo),
            Some(GateScope {
                branch: Some("main".into()),
                refuse_stash_save: true
            })
        );
    }

    #[test]
    fn gate_scope_for_a_detached_sequence_names_no_branch() {
        let (_dir, repo) = init();
        let (rebase, sequence) = (RebaseStore::open(&repo), SequenceStore::open(&repo));
        let memory = RebaseMemory::default();
        for original_ref in [OriginalRef::Detached, OriginalRef::Unknown] {
            sequence
                .save(&stored_sequence(&repo, original_ref))
                .unwrap();
            assert_eq!(
                probe(&rebase, &memory, &sequence).gate_scope(&repo),
                Some(GateScope {
                    branch: None,
                    refuse_stash_save: true
                })
            );
        }
    }

    #[test]
    fn gate_scope_for_a_rebase_is_unchanged() {
        let (_dir, repo) = init();
        let onto = commit(&repo, "base.txt", "base\n", "base");
        let a = commit(&repo, "a.txt", "a\n", "a");
        let plan = vec![RebasePlanEntry {
            commit_id: a.to_string(),
            action: RebaseAction::Edit,
            combined_message: None,
        }];
        let (state, _) = git_core::rebase::start_rebase(&repo, &onto.to_string(), plan).unwrap();
        let (rebase, sequence) = (RebaseStore::open(&repo), SequenceStore::open(&repo));
        rebase.save(&state).unwrap();
        let memory = RebaseMemory::default();
        let branch = state
            .original_branch_ref
            .strip_prefix("refs/heads/")
            .unwrap()
            .to_string();
        assert_eq!(
            probe(&rebase, &memory, &sequence).gate_scope(&repo),
            Some(GateScope {
                branch: Some(branch),
                refuse_stash_save: true
            })
        );
    }

    #[test]
    fn gate_scope_is_none_when_nothing_is_stored() {
        let (_dir, repo) = init();
        let (rebase, sequence) = (RebaseStore::open(&repo), SequenceStore::open(&repo));
        let memory = RebaseMemory::default();
        assert_eq!(probe(&rebase, &memory, &sequence).gate_scope(&repo), None);
    }

    fn mtime_ns(repo: &Repository, file: &str) -> u128 {
        std::fs::metadata(repo.path().join(file))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    fn set_mtime(repo: &Repository, file: &str, mtime: std::time::SystemTime) {
        std::fs::File::options()
            .write(true)
            .open(repo.path().join(file))
            .unwrap()
            .set_modified(mtime)
            .unwrap();
    }

    /// A terminal `git rebase` keeps one marker through all its steps, though HEAD moves at each
    /// (#264); aborted and started again, or finished, it no longer matches.
    #[test]
    fn a_terminal_rebase_keeps_its_marker_through_its_steps() {
        let (_dir, repo) = init();
        commit(&repo, "f", "base\n", "base");
        let base = repo.head().unwrap().target().unwrap();
        repo.branch("topic", &repo.find_commit(base).unwrap(), false)
            .unwrap();
        commit(&repo, "f", "main\n", "main");
        let main_branch = repo.head().unwrap().shorthand().unwrap().to_string();
        assert!(git(&repo, &["checkout", "-q", "topic"]).status.success());
        commit(&repo, "f", "topic 1\n", "t1");
        commit(&repo, "f", "topic 2\n", "t2");
        let (rebase_store, sequence_store) = (RebaseStore::open(&repo), SequenceStore::open(&repo));
        let workdir = repo.workdir().unwrap().to_path_buf();
        let resolve = || {
            std::fs::write(workdir.join("f"), "resolved\n").unwrap();
            assert!(git(&repo, &["add", "f"]).status.success());
        };

        assert!(!git(&repo, &["rebase", &main_branch]).status.success());
        let first = marker(&repo, &rebase_store, &sequence_store).unwrap();
        assert!(first.starts_with("terminal Rebase topic "), "{first}");
        let first_head = repo.head().unwrap().target();
        resolve();
        let output = git(&repo, &["-c", "core.editor=true", "rebase", "--continue"]);
        assert!(!output.status.success(), "t2 must conflict too: {output:?}");
        assert_ne!(repo.head().unwrap().target(), first_head);
        assert_eq!(
            marker(&repo, &rebase_store, &sequence_store),
            Some(first.clone())
        );

        // Aborted and started again: the same branch, start and onto, a new rebase.
        let started = std::fs::metadata(repo.path().join("rebase-merge/orig-head"))
            .unwrap()
            .modified()
            .unwrap();
        assert!(git(&repo, &["rebase", "--abort"]).status.success());
        assert!(!git(&repo, &["rebase", &main_branch]).status.success());
        set_mtime(
            &repo,
            "rebase-merge/orig-head",
            started + std::time::Duration::from_secs(2),
        );
        let again = marker(&repo, &rebase_store, &sequence_store).unwrap();
        assert!(again.starts_with("terminal Rebase topic "), "{again}");
        assert_ne!(again, first);

        // Finished: nothing in progress.
        resolve();
        assert!(
            !git(&repo, &["-c", "core.editor=true", "rebase", "--continue"])
                .status
                .success()
        );
        resolve();
        assert!(
            git(&repo, &["-c", "core.editor=true", "rebase", "--continue"])
                .status
                .success()
        );
        assert_eq!(marker(&repo, &rebase_store, &sequence_store), None);
    }

    /// `git am`'s marker is likewise its own, the same through its patches (#264).
    #[test]
    fn a_terminal_am_keeps_its_marker_through_its_patches() {
        let (dir, repo) = init();
        commit(&repo, "f", "base\n", "base");
        let base = repo.head().unwrap().target().unwrap();
        commit(&repo, "f", "one\n", "one");
        commit(&repo, "f", "two\n", "two");
        let patches = tempfile::TempDir::new().unwrap();
        let output = git(
            &repo,
            &[
                "format-patch",
                "-q",
                "-2",
                "-o",
                patches.path().to_str().unwrap(),
            ],
        );
        assert!(output.status.success(), "{output:?}");
        assert!(git(&repo, &["reset", "-q", "--hard", &base.to_string()])
            .status
            .success());
        std::fs::write(dir.path().join("f"), "other\n").unwrap();
        assert!(git(&repo, &["commit", "-qam", "other"]).status.success());
        let mut files: Vec<String> = std::fs::read_dir(patches.path())
            .unwrap()
            .map(|entry| entry.unwrap().path().to_str().unwrap().to_string())
            .collect();
        files.sort();
        let mut args = vec!["am", "-3"];
        args.extend(files.iter().map(String::as_str));
        let (rebase_store, sequence_store) = (RebaseStore::open(&repo), SequenceStore::open(&repo));

        assert!(!git(&repo, &args).status.success());
        let first = marker(&repo, &rebase_store, &sequence_store).unwrap();
        assert!(first.starts_with("terminal Am "), "{first}");
        std::fs::write(dir.path().join("f"), "resolved\n").unwrap();
        assert!(git(&repo, &["add", "f"]).status.success());
        let head = repo.head().unwrap().target();
        let output = git(&repo, &["am", "--continue"]);
        assert!(
            !output.status.success(),
            "patch two must conflict: {output:?}"
        );
        assert_ne!(repo.head().unwrap().target(), head);
        assert_eq!(marker(&repo, &rebase_store, &sequence_store), Some(first));
    }

    #[test]
    fn a_rewritten_merge_head_changes_the_marker() {
        // `git merge --abort` and the same merge again: same heads, a new MERGE_HEAD.
        let (_dir, repo) = init();
        commit(&repo, "base.txt", "base\n", "base");
        let a = commit(&repo, "a.txt", "a\n", "a");
        let (rebase_store, sequence_store) = (RebaseStore::open(&repo), SequenceStore::open(&repo));
        let now = std::time::SystemTime::now();
        std::fs::write(repo.path().join("MERGE_HEAD"), format!("{a}\n")).unwrap();
        set_mtime(&repo, "MERGE_HEAD", now);
        let first = marker(&repo, &rebase_store, &sequence_store).unwrap();
        std::fs::write(repo.path().join("MERGE_HEAD"), format!("{a}\n")).unwrap();
        set_mtime(&repo, "MERGE_HEAD", now + std::time::Duration::from_secs(2));
        let second = marker(&repo, &rebase_store, &sequence_store).unwrap();
        assert_ne!(first, second);
        assert!(first.starts_with("merge ") && second.starts_with("merge "));
    }

    #[test]
    fn pick_and_revert_markers_carry_their_state_files_mtime() {
        let (_dir, repo) = init();
        commit(&repo, "base.txt", "base\n", "base");
        let a = commit(&repo, "a.txt", "a\n", "a");
        let (rebase_store, sequence_store) = (RebaseStore::open(&repo), SequenceStore::open(&repo));
        for (file, verb) in [
            ("CHERRY_PICK_HEAD", "cherry-pick"),
            ("REVERT_HEAD", "revert"),
        ] {
            std::fs::write(repo.path().join(file), format!("{a}\n")).unwrap();
            let found = marker(&repo, &rebase_store, &sequence_store).unwrap();
            assert_eq!(
                found,
                format!("{verb} {a} {a} @{}", mtime_ns(&repo, file)),
                "{file}"
            );
            std::fs::remove_file(repo.path().join(file)).unwrap();
        }
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
        let memory = RebaseMemory {
            fallback: Some(state),
            forgotten: None,
        };
        let probe = OperationProbe {
            rebase: &rebase_store,
            memory: &memory,
            sequence: &sequence_store,
        };
        assert!(probe.marker(&repo).unwrap().starts_with("rebase "));
        assert!(probe.rebased_branch(&repo).is_some());
    }
}
