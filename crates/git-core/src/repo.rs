use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("failed to open repository: {0}")]
    Open(#[from] git2::Error),
    #[error("This folder is already a Git repository.")]
    AlreadyRepository,
    #[error("failed to create repository: {0}")]
    Init(#[source] git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Process-wide libgit2 settings for every binary that opens repositories (#840). Call once at
/// startup, before the first repository is opened.
///
/// Turns off libgit2's strict hash verification: by default it hashes every object it reads
/// again and compares it with its id, up to two thirds of a history walk's time. git checks
/// hashes in `git fsck` and when it receives objects, not on ordinary reads. Objects the app
/// writes are still hashed to make their ids. See `docs/ARCHITECTURE.md`.
pub fn configure_libgit2() {
    git2::opts::strict_hash_verification(false);
}

pub fn open(path: &Path) -> Result<git2::Repository, RepoError> {
    Ok(git2::Repository::discover(path)?)
}

/// Creates an empty repository at `path` (and any missing parents). The initial branch honours
/// the user's global `init.defaultBranch`, else `main`.
pub fn init_repo(path: &Path) -> Result<(), RepoError> {
    if !path.is_absolute() {
        return Err(RepoError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Destination must be an absolute path.",
        )));
    }
    if path.join(".git").exists() {
        return Err(RepoError::AlreadyRepository);
    }
    std::fs::create_dir_all(path)?;
    let branch = git2::Config::open_default()
        .ok()
        .and_then(|config| config.get_string("init.defaultBranch").ok())
        .filter(|branch| !branch.is_empty())
        .unwrap_or_else(|| "main".to_string());
    let mut options = git2::RepositoryInitOptions::new();
    options.initial_head(&branch);
    git2::Repository::init_opts(path, &options).map_err(RepoError::Init)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum OperationKind {
    Merge,
    CherryPick,
    Revert,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadInfo {
    pub head_id: Option<String>,
    pub detached_at: Option<String>,
    pub head_pushed: bool,
    pub operation: Option<OperationKind>,
    /// `operation`'s changes are already in HEAD and only git's state files are left: its own
    /// commit's cleanup failed, or a duplicate pick stopped as empty (`commit::leftover_operation`,
    /// #240). The next commit or amend clears them; Abort does too.
    pub operation_leftover: bool,
    /// A pending squash merge's message (`SQUASH_MSG`), which pre-fills the commit box.
    pub squash_message: Option<String>,
    /// Installed commit hooks that `--no-verify` skips (`pre-commit`, `commit-msg`); drives the
    /// "Skip hooks" checkbox.
    pub commit_hooks: Vec<String>,
    /// A stopped multi-commit cherry-pick/revert. Filled by `repo-service` from its stored
    /// sequence state; `git-core` has no persistence and always reports `None`.
    pub sequence: Option<crate::pick::SequenceProgress>,
    /// A paused interactive rebase. Filled by `repo-service` from its stored rebase state, like
    /// `sequence`; `git-core` always reports `None`.
    pub rebase: Option<RebaseInfo>,
    /// A `git bisect` started in a terminal. Independent of `operation`: a merge paused during a
    /// bisect has both.
    pub bisect: Option<BisectInfo>,
    /// A `git cherry-pick A B C` / `git revert A B` started in a terminal and not finished
    /// (`.git/sequencer/todo`), paused on a step or between steps (#258).
    pub terminal_sequence: Option<crate::pick::TerminalSequence>,
    /// A `git rebase` or `git am` started in a terminal and not finished (`.git/rebase-merge` or
    /// `.git/rebase-apply`, #263). Browsitory's own rebase never writes those folders.
    pub terminal_rebase: Option<TerminalRebase>,
}

/// Which command a terminal rebase folder belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TerminalRebaseKind {
    /// `git rebase` (`rebase-merge`, or `rebase-apply` with the apply backend).
    Rebase,
    /// `git am` (`rebase-apply/applying`).
    Am,
}

/// A `git rebase` or `git am` in progress in this worktree's git folder, as git's `wt_status`
/// tells them apart. Continued or aborted in the terminal only: Browsitory can't do exactly what
/// `git rebase --abort` does (autostash, `--update-refs`, the apply backend), so it refuses
/// whatever git would refuse and says so (#263).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRebase {
    pub kind: TerminalRebaseKind,
    /// The branch being rebased (short name, from `head-name`); empty for a detached HEAD, for
    /// `git am`, or when the file can't be read.
    pub branch: String,
    /// Full id of the commit the branch is rebased onto (`onto`); empty for `git am` or when
    /// the file can't be read.
    pub onto: String,
    /// 1-based step the rebase is at, and how many there are (`msgnum`/`end`, or `next`/`last`
    /// with `rebase-apply`); both 0 when unknown.
    pub current_step: usize,
    pub total_steps: usize,
}

/// The refusal while a terminal rebase or `git am` is in progress (#263): it names the command
/// and how to go on with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("{}", terminal_rebase_refusal(*.0))]
pub struct TerminalRebaseInProgress(pub TerminalRebaseKind);

fn terminal_rebase_refusal(kind: TerminalRebaseKind) -> String {
    let (what, command) = match kind {
        TerminalRebaseKind::Rebase => ("A rebase", "git rebase"),
        TerminalRebaseKind::Am => ("A git am", "git am"),
    };
    format!(
        "{what} started in a terminal isn't finished. Continue it there with {command} \
         --continue, or abort it with {command} --abort."
    )
}

/// The refusal to delete, rename or check out in a new worktree a branch that a terminal rebase
/// (in this or any other worktree of the repository) is rebasing, as git refuses it (#263).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "{0} is being rebased by a rebase started in a terminal. Continue it there with git rebase \
     --continue, or abort it with git rebase --abort."
)]
pub struct BranchBeingRebased(pub String);

/// `Err` when `branch` (short name) is the branch of a terminal rebase in any worktree of the
/// repository: its git folder's `rebase-merge/head-name` or `rebase-apply/head-name` names it
/// (git's `wt_status_check_rebase` for every worktree, as `branch -D` checks). HEAD is detached
/// during the rebase, so libgit2's "checked out" checks don't see it.
pub(crate) fn check_branch_not_rebased(
    repo: &git2::Repository,
    branch: &str,
) -> Result<(), BranchBeingRebased> {
    let full = format!("refs/heads/{branch}");
    if rebased_branches(repo).any(|(_, name)| name == full) {
        return Err(BranchBeingRebased(branch.to_string()));
    }
    Ok(())
}

/// [`check_branch_not_rebased`] for a terminal rebase in another worktree only: switching to
/// the branch being rebased in this worktree is what git allows (the UI disables it anyway).
pub(crate) fn check_branch_not_rebased_elsewhere(
    repo: &git2::Repository,
    branch: &str,
) -> Result<(), BranchBeingRebased> {
    let full = format!("refs/heads/{branch}");
    let this = std::fs::canonicalize(repo.path()).ok();
    let elsewhere = rebased_branches(repo)
        .any(|(git_dir, name)| name == full && std::fs::canonicalize(&git_dir).ok() != this);
    if elsewhere {
        return Err(BranchBeingRebased(branch.to_string()));
    }
    Ok(())
}

/// Every branch a terminal rebase is rebasing (full ref name), with the git folder of the
/// worktree it runs in: the main one and each linked one (`worktrees/<name>`).
pub(crate) fn rebased_branches(
    repo: &git2::Repository,
) -> impl Iterator<Item = (std::path::PathBuf, String)> {
    worktree_git_dirs(repo).into_iter().flat_map(|dir| {
        ["rebase-merge", "rebase-apply"]
            .into_iter()
            .filter_map(|folder| std::fs::read_to_string(dir.join(folder).join("head-name")).ok())
            .map(|name| name.trim().to_string())
            .filter(|name| name.starts_with("refs/heads/"))
            .map(|name| (dir.clone(), name))
            .collect::<Vec<_>>()
    })
}

/// The git folder of every worktree of the repository: the main one (the common folder) and
/// each linked one (`worktrees/<name>`), whether or not its work tree still exists.
fn worktree_git_dirs(repo: &git2::Repository) -> Vec<std::path::PathBuf> {
    let common = repo.commondir().to_path_buf();
    let mut git_dirs = vec![common.clone()];
    if let Ok(entries) = std::fs::read_dir(common.join("worktrees")) {
        git_dirs.extend(
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.is_dir()),
        );
    }
    git_dirs
}

/// How a worktree uses a branch, so that it can't be moved, deleted or checked out elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchUse {
    /// The worktree's HEAD is on it.
    CheckedOut,
    /// A rebase paused there is rebasing it, or will update it (`--update-refs`): a terminal
    /// one, or Browsitory's own (#291).
    Rebased,
    /// A terminal `git bisect` started from it there (`BISECT_START`); `git bisect reset`
    /// returns to it (#291).
    Bisected,
    /// Browsitory's own cherry-pick or revert sequence paused there runs on it; its Abort
    /// rewinds it (#293).
    Sequence(crate::pick::SequenceKind),
}

/// Whether the folder of another worktree that holds a branch is still a worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeFolder {
    Present,
    /// The folder is gone. `git worktree prune` removes the worktree unless it is `locked`.
    Missing {
        locked: bool,
    },
    /// The folder exists, but its `.git` file is gone. Pruned like a missing folder.
    NotAWorktree {
        locked: bool,
    },
}

/// Which worktree holds a branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holder {
    ThisWorktree,
    /// Another worktree, at `path` (as git recorded it).
    Other {
        path: std::path::PathBuf,
        folder: WorktreeFolder,
    },
}

/// The refusal to move, delete, rename or check out a branch a worktree holds (#287, #291). It
/// names the other worktree, with a hint when its folder is no longer a worktree.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub struct BranchHeld {
    /// Short name.
    pub branch: String,
    pub usage: BranchUse,
    pub holder: Holder,
}

impl std::fmt::Display for BranchHeld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::pick::SequenceKind;
        let how = match self.usage {
            BranchUse::CheckedOut => "is checked out",
            BranchUse::Rebased => "is being rebased",
            BranchUse::Bisected => "is being bisected",
            BranchUse::Sequence(SequenceKind::CherryPick) => "has a paused cherry-pick",
            BranchUse::Sequence(SequenceKind::Revert) => "has a paused revert",
        };
        let branch = &self.branch;
        let (path, folder) = match &self.holder {
            Holder::ThisWorktree => return write!(f, "branch '{branch}' {how} in this worktree"),
            Holder::Other { path, folder } => (path.display(), folder),
        };
        write!(f, "branch '{branch}' {how} in another worktree at '{path}'")?;
        let (what, locked) = match *folder {
            WorktreeFolder::Present => return Ok(()),
            WorktreeFolder::Missing { locked } => ("Its folder no longer exists", locked),
            WorktreeFolder::NotAWorktree { locked } => (
                "That folder is no longer a worktree (its .git file is missing)",
                locked,
            ),
        };
        if locked {
            write!(
                f,
                ". {what}, but that worktree is locked, so git keeps it. If it is gone for good, \
                 run git worktree unlock and then git worktree prune."
            )
        } else {
            write!(
                f,
                ". {what}: run git worktree prune to remove that worktree, then try again."
            )
        }
    }
}

/// Which of git's rules decides whether a worktree holds a branch.
///
/// Browsitory's own paused cherry-pick/revert in another worktree counts under every rule
/// (#293). That worktree's HEAD is on its branch, so git's rules hold the branch already, except
/// `branch -m`, which git allows (its `cherry-pick --abort` rewinds commits, not a name).
/// Browsitory refuses that rename too, as the owning window does: the stored sequence keeps the
/// old name, which that window protects, and Abort finds a renamed branch only through its
/// reflog (`pick::abort_sequence`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HoldRule {
    /// git's `branch_checked_out` (`branch -f`, `branch -D`): every worktree, this one too, holds
    /// the branch its HEAD is on (except this worktree's own: Restore moves it, and libgit2
    /// refuses to delete it), the branch a rebase there rebases or updates (`update-refs`), and
    /// the branch a bisect there started from, whether or not its HEAD is detached.
    MoveOrDelete,
    /// git's `find_shared_symref` (`switch`, `worktree add`): a worktree holds the branch its
    /// HEAD is on, and while its HEAD is detached, the branch a rebase there rebases or a bisect
    /// there started from. `include_this`: `worktree add` counts this worktree too, Switch
    /// doesn't.
    CheckOut { include_this: bool },
    /// `branch -m` (`reject_rebase_or_bisect_branch`): rebases and bisects in worktrees whose
    /// HEAD is detached, this one too, and (Browsitory only) a paused cherry-pick/revert in
    /// another worktree. HEAD moves along with a renamed branch.
    Rename,
}

/// What a worktree's git folder says about the branches it uses (full names).
struct WorktreeBranches {
    holder: Holder,
    /// The branch HEAD is on; `None` while HEAD is detached.
    head: Option<String>,
    /// A terminal rebase's branch (`head-name`).
    rebasing: Option<String>,
    /// The refs a terminal rebase with `--update-refs` will update.
    update_refs: Vec<String>,
    /// Browsitory's own paused rebase's branch: only read for other worktrees, only while
    /// their HEAD is detached, and only when that worktree would load the file itself.
    stored_rebase: Option<String>,
    /// Browsitory's own paused cherry-pick/revert's branch and kind: only read for other
    /// worktrees, only while their HEAD is on that branch, and only when that worktree would
    /// load the file itself.
    stored_sequence: Option<(String, crate::pick::SequenceKind)>,
    /// The branch a bisect started from.
    bisecting: Option<String>,
}

impl WorktreeBranches {
    /// The branches this worktree holds under `rule`.
    fn held(&self, rule: HoldRule) -> Vec<(&str, BranchUse)> {
        let this = self.holder == Holder::ThisWorktree;
        let detached = self.head.is_none();
        // First, so the refusal names the sequence rather than HEAD on the same branch.
        let mut held: Vec<(&str, BranchUse)> = self
            .stored_sequence
            .iter()
            .map(|(name, kind)| (name.as_str(), BranchUse::Sequence(*kind)))
            .collect();
        let counts_head = match rule {
            HoldRule::MoveOrDelete => !this,
            HoldRule::CheckOut { include_this } => include_this || !this,
            HoldRule::Rename => false,
        };
        let counts_operations = match rule {
            HoldRule::MoveOrDelete => true,
            HoldRule::CheckOut { include_this } => (include_this || !this) && detached,
            HoldRule::Rename => detached,
        };
        if counts_head {
            held.extend(
                self.head
                    .as_deref()
                    .map(|name| (name, BranchUse::CheckedOut)),
            );
        }
        if counts_operations {
            held.extend(
                self.rebasing
                    .as_deref()
                    .map(|name| (name, BranchUse::Rebased)),
            );
            held.extend(
                self.stored_rebase
                    .as_deref()
                    .map(|name| (name, BranchUse::Rebased)),
            );
            held.extend(
                self.bisecting
                    .as_deref()
                    .map(|name| (name, BranchUse::Bisected)),
            );
        }
        if rule == HoldRule::MoveOrDelete {
            held.extend(
                self.update_refs
                    .iter()
                    .map(|name| (name.as_str(), BranchUse::Rebased)),
            );
        }
        held
    }
}

/// Removes `.` and `..` components without touching the file system: a relative `gitdir`
/// (`git worktree add --relative-paths`) is relative to the worktree's git folder.
fn normalize_lexically(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component);
                }
            }
            other => normalized.push(other),
        }
    }
    normalized
}

/// A linked worktree's folder and state, from its git folder's `gitdir` file (which names the
/// worktree's `.git` file), as `git worktree prune` decides it. `None` when that file is missing
/// or empty: git doesn't list such a worktree, so it holds nothing.
fn linked_worktree_folder(
    git_dir: &std::path::Path,
) -> Option<(std::path::PathBuf, WorktreeFolder)> {
    let recorded = std::fs::read_to_string(git_dir.join("gitdir")).ok()?;
    let recorded = recorded.trim();
    if recorded.is_empty() {
        return None;
    }
    let dot_git = normalize_lexically(&git_dir.join(recorded));
    let folder = dot_git.parent()?.to_path_buf();
    let locked = git_dir.join("locked").exists();
    let state = if dot_git.exists() {
        WorktreeFolder::Present
    } else if folder.is_dir() {
        WorktreeFolder::NotAWorktree { locked }
    } else {
        WorktreeFolder::Missing { locked }
    };
    Some((folder, state))
}

/// Whether two paths name the same folder: equal, or equal once canonicalized.
fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    use std::fs::canonicalize;
    a == b || matches!((canonicalize(a), canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

/// Reads every worktree of the repository from its git folder, as git's `get_worktrees` and
/// `wt_status_check_rebase`/`_bisect` do, so a worktree whose folder is gone still counts
/// (#287). A bare main repository has none of these.
fn worktree_branches(repo: &git2::Repository) -> Result<Vec<WorktreeBranches>, git2::Error> {
    use std::fs::read_to_string;
    let common = repo.commondir().to_path_buf();
    let main = git2::Repository::open(&common)?;
    let mut worktrees = Vec::new();
    for git_dir in worktree_git_dirs(repo) {
        let is_main = git_dir == common;
        if is_main && main.is_bare() {
            continue;
        }
        let holder = if same_dir(&git_dir, repo.path()) {
            Holder::ThisWorktree
        } else if is_main {
            let workdir = main.workdir().unwrap_or(&common);
            Holder::Other {
                path: workdir.components().collect(),
                folder: WorktreeFolder::Present,
            }
        } else {
            let Some((path, folder)) = linked_worktree_folder(&git_dir) else {
                continue;
            };
            Holder::Other { path, folder }
        };
        let read = |name: &str| read_to_string(git_dir.join(name)).ok();
        let branch = |name: String| name.starts_with("refs/heads/").then_some(name);
        let head = read("HEAD")
            .and_then(|head| {
                head.strip_prefix("ref:")
                    .map(|name| name.trim().to_string())
            })
            .and_then(branch);
        let rebasing = ["rebase-merge/head-name", "rebase-apply/head-name"]
            .into_iter()
            .find_map(&read)
            .map(|name| name.trim().to_string())
            .and_then(branch);
        // git's `sequencer_get_update_refs_state`: refname, old id and new id per ref.
        let update_refs = read("rebase-merge/update-refs")
            .map(|text| {
                text.lines()
                    .step_by(3)
                    .map(|name| name.trim().to_string())
                    .filter_map(branch)
                    .collect()
            })
            .unwrap_or_default();
        // This worktree's own stored rebase is the worker's gate's (it knows a file it couldn't
        // delete is over). Elsewhere, a paused rebase always has HEAD detached.
        let stored_rebase = (holder != Holder::ThisWorktree && head.is_none())
            .then(|| std::fs::read(crate::rebase_file::file(&git_dir)).ok())
            .flatten()
            .and_then(|bytes| crate::rebase_file::decode(&bytes, repo).ok())
            .map(|state| state.original_branch_ref);
        // Likewise for a stored cherry-pick/revert (#293), which runs on its branch. Once that
        // worktree's HEAD left it, the sequence no longer touches it: Abort doesn't rewind it,
        // and Continue, which checks only the commit HEAD is on, goes on on HEAD's new branch.
        let stored_sequence = (holder != Holder::ThisWorktree)
            .then(|| std::fs::read(crate::sequence_file::file(&git_dir)).ok())
            .flatten()
            .and_then(|bytes| crate::sequence_file::decode(&bytes, repo).ok())
            .and_then(|state| match state.original_ref {
                crate::pick::OriginalRef::Branch(name) if head.as_ref() == Some(&name) => {
                    Some((name, state.kind))
                }
                _ => None,
            });
        // git's `wt_status_check_bisect`: BISECT_LOG marks the bisect, BISECT_START names the
        // branch it started from (or a commit id, which names no branch). BISECT_LOG holds raw
        // commit subjects, which needn't be UTF-8, so only its existence is checked.
        let bisecting = git_dir
            .join("BISECT_LOG")
            .exists()
            .then(|| read("BISECT_START"))
            .flatten()
            .map(|start| format!("refs/heads/{}", start.trim()));
        worktrees.push(WorktreeBranches {
            holder,
            head,
            rebasing,
            update_refs,
            stored_rebase,
            stored_sequence,
            bisecting,
        });
    }
    Ok(worktrees)
}

/// Every branch a worktree holds under `rule` (#287, #291). A branch may be listed more than
/// once.
pub(crate) fn branches_held(
    repo: &git2::Repository,
    rule: HoldRule,
) -> Result<Vec<BranchHeld>, git2::Error> {
    Ok(worktree_branches(repo)?
        .iter()
        .flat_map(|worktree| {
            worktree
                .held(rule)
                .into_iter()
                .filter_map(|(name, usage)| {
                    Some(BranchHeld {
                        branch: name.strip_prefix("refs/heads/")?.to_string(),
                        usage,
                        holder: worktree.holder.clone(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect())
}

/// The refusal for `branch` (short name) when a worktree holds it under `rule`.
pub(crate) fn branch_held(
    repo: &git2::Repository,
    branch: &str,
    rule: HoldRule,
) -> Result<Option<BranchHeld>, git2::Error> {
    Ok(branches_held(repo, rule)?
        .into_iter()
        .find(|held| held.branch == branch))
}

/// `Err` while a terminal rebase or `git am` is in progress.
pub(crate) fn check_no_terminal_rebase(
    repo: &git2::Repository,
) -> Result<(), TerminalRebaseInProgress> {
    match terminal_rebase_kind(repo) {
        Some(kind) => Err(TerminalRebaseInProgress(kind)),
        None => Ok(()),
    }
}

/// The folder of a terminal rebase or `git am`, and which it is: `rebase-merge` is always a
/// rebase; `rebase-apply` is `git am` when it holds `applying` (git's `wt_status_check_rebase`).
fn terminal_rebase_folder(
    repo: &git2::Repository,
) -> Option<(std::path::PathBuf, TerminalRebaseKind)> {
    let merge = repo.path().join("rebase-merge");
    if merge.is_dir() {
        return Some((merge, TerminalRebaseKind::Rebase));
    }
    let apply = repo.path().join("rebase-apply");
    if !apply.is_dir() {
        return None;
    }
    let kind = if apply.join("applying").exists() {
        TerminalRebaseKind::Am
    } else {
        TerminalRebaseKind::Rebase
    };
    Some((apply, kind))
}

fn terminal_rebase_kind(repo: &git2::Repository) -> Option<TerminalRebaseKind> {
    terminal_rebase_folder(repo).map(|(_, kind)| kind)
}

/// The terminal rebase or `git am` in progress, if any.
pub fn terminal_rebase(repo: &git2::Repository) -> Option<TerminalRebase> {
    let (folder, kind) = terminal_rebase_folder(repo)?;
    let read = |name: &str| {
        std::fs::read_to_string(folder.join(name))
            .map(|text| text.trim().to_string())
            .unwrap_or_default()
    };
    let number = |name: &str| read(name).parse::<usize>().unwrap_or(0);
    let (branch, onto) = match kind {
        TerminalRebaseKind::Rebase => {
            let head_name = read("head-name");
            let branch = head_name
                .strip_prefix("refs/heads/")
                .map(str::to_string)
                .unwrap_or_default();
            (branch, read("onto"))
        }
        TerminalRebaseKind::Am => (String::new(), String::new()),
    };
    let (current, total) = if folder.ends_with("rebase-merge") {
        ("msgnum", "end")
    } else {
        ("next", "last")
    };
    Some(TerminalRebase {
        kind,
        branch,
        onto,
        current_step: number(current),
        total_steps: number(total),
    })
}

/// What `start_rebase` names when another operation is paused (#265).
pub(crate) fn paused_operation(repo: &git2::Repository) -> Option<OperationKind> {
    use git2::RepositoryState;
    match repo.state() {
        RepositoryState::Merge => Some(OperationKind::Merge),
        RepositoryState::CherryPick | RepositoryState::CherryPickSequence => {
            Some(OperationKind::CherryPick)
        }
        RepositoryState::Revert | RepositoryState::RevertSequence => Some(OperationKind::Revert),
        _ => None,
    }
}

/// A `git bisect` in progress (`BISECT_LOG` exists in this worktree's git folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BisectInfo {
    /// What `git bisect reset` returns to (`BISECT_START`): a branch name, or a commit id when
    /// the bisect started on a detached HEAD. Empty if the file is missing.
    pub started_from: String,
}

/// The bisect started in a terminal, if any.
fn bisect_info(repo: &git2::Repository) -> Option<BisectInfo> {
    if !repo.path().join("BISECT_LOG").exists() {
        return None;
    }
    let started_from = std::fs::read_to_string(repo.path().join("BISECT_START"))
        .map(|text| text.trim().to_string())
        .unwrap_or_default();
    Some(BisectInfo { started_from })
}

/// A paused interactive rebase as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebaseInfo {
    /// The branch being rebased (short name).
    pub branch: String,
    /// Full id of the commit the branch is rebased onto.
    pub onto: String,
    pub current_step: usize,
    pub total_steps: usize,
    pub health: crate::rebase::RebaseHealth,
    /// Full id of the commit the rebase stopped at to be edited (an `Edit` step, #882);
    /// `None` at a conflict or error stop.
    pub editing: Option<String>,
    /// Split a commit (#884): parts of the edited commit committed so far; 0 when not editing.
    pub split_parts: usize,
}

impl RebaseInfo {
    pub fn new(state: &crate::rebase::RebaseState, health: crate::rebase::RebaseHealth) -> Self {
        let progress = state.progress();
        Self {
            branch: state
                .original_branch_ref
                .strip_prefix("refs/heads/")
                .unwrap_or(&state.original_branch_ref)
                .to_string(),
            onto: state.onto.to_string(),
            current_step: progress.current_step,
            total_steps: progress.total_steps,
            health,
            editing: state
                .plan
                .get(state.cursor)
                .filter(|entry| {
                    state.stop == crate::rebase::RebaseStop::Edit
                        && entry.action == crate::rebase::RebaseAction::Edit
                })
                .map(|entry| entry.commit_id.clone()),
            split_parts: 0,
        }
    }
}

/// True when git has no merge, cherry-pick, revert or rebase paused. A `git bisect` started in
/// a terminal (libgit2's `Bisect` state) counts as nothing in progress, as it does for git: it
/// allows picks, reverts, rebases, reset, amend and commits while bisecting (#250).
///
/// libgit2 reports `Bisect` only when nothing else is paused (it checks the rebase folders,
/// `MERGE_HEAD`, `REVERT_HEAD` and `CHERRY_PICK_HEAD` first), so a merge or pick paused during a
/// bisect still counts as in progress.
pub(crate) fn no_operation(repo: &git2::Repository) -> bool {
    matches!(
        repo.state(),
        git2::RepositoryState::Clean | git2::RepositoryState::Bisect
    )
}

/// Snapshot of what HEAD is and which single-commit operation (if any) is paused on it. A
/// stored sequence or rebase is filled in by `repo-service`.
pub fn head_info(repo: &git2::Repository) -> Result<HeadInfo, RepoError> {
    use git2::RepositoryState;
    let operation = paused_operation(repo);
    let operation_leftover = operation.is_some() && crate::commit::leftover_operation(repo);
    let mut names = crate::hooks::SKIPPABLE_COMMIT_HOOKS.to_vec();
    if repo.state() == RepositoryState::Merge && !operation_leftover {
        // The commit concluding a clean merge runs it (`commit::CommitHookSet::CleanMerge`).
        // After a leftover merge the next commit is an ordinary one.
        names.push("pre-merge-commit");
    }
    let commit_hooks = crate::hooks::installed_hooks(repo, &names);
    let head = match repo.head() {
        Ok(head) => head,
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {
            return Ok(HeadInfo {
                head_id: None,
                detached_at: None,
                head_pushed: false,
                operation,
                operation_leftover,
                squash_message: crate::merge::squash_message(repo),
                commit_hooks,
                sequence: None,
                rebase: None,
                bisect: bisect_info(repo),
                terminal_sequence: crate::pick::terminal_sequence(repo),
                terminal_rebase: terminal_rebase(repo),
            });
        }
        Err(error) => return Err(error.into()),
    };
    let head_oid = head.peel_to_commit()?.id();
    let detached = repo.head_detached()?;
    let detached_at = detached.then(|| head_oid.to_string()[..7].to_string());
    let head_pushed = !detached && upstream_contains(repo, &head, head_oid);
    Ok(HeadInfo {
        head_id: Some(head_oid.to_string()),
        detached_at,
        head_pushed,
        operation,
        operation_leftover,
        squash_message: crate::merge::squash_message(repo),
        commit_hooks,
        sequence: None,
        rebase: None,
        bisect: bisect_info(repo),
        terminal_sequence: crate::pick::terminal_sequence(repo),
        terminal_rebase: terminal_rebase(repo),
    })
}

fn upstream_contains(
    repo: &git2::Repository,
    head: &git2::Reference<'_>,
    head_oid: git2::Oid,
) -> bool {
    let Ok(name) = head.shorthand() else {
        return false;
    };
    let Ok(branch) = repo.find_branch(name, git2::BranchType::Local) else {
        return false;
    };
    let Ok(upstream) = branch.upstream() else {
        return false;
    };
    let Some(upstream_oid) = upstream.get().target() else {
        return false;
    };
    upstream_oid == head_oid
        || repo
            .graph_descendant_of(upstream_oid, head_oid)
            .unwrap_or(false)
}
