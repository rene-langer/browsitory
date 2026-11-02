use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("failed to open repository: {0}")]
    Open(#[from] git2::Error),
    #[error("This folder is already a Git repository.")]
    AlreadyRepository,
    #[error("failed to create repository: {0}")]
    Init(git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Full names of the branches every other worktree of this repository has checked out, and of
/// the branches a terminal rebase is rebasing in another worktree: git counts those as checked
/// out there, though HEAD is detached meanwhile (#263).
///
/// Like git's `find_shared_symref`, each worktree's `HEAD` is read from its git folder, so a
/// worktree whose folder is gone (a locked one on an unmounted drive, or one not pruned yet)
/// still counts (#287). Only a bare main repository has no branch checked out.
pub(crate) fn branches_checked_out_elsewhere(
    repo: &git2::Repository,
) -> Result<std::collections::BTreeSet<String>, git2::Error> {
    use std::fs::canonicalize;
    let this =
        canonicalize(repo.path()).map_err(|error| git2::Error::from_str(&error.to_string()))?;
    let common = repo.commondir().to_path_buf();
    let main_is_bare = git2::Repository::open(&common)?.is_bare();
    let mut branches = std::collections::BTreeSet::new();
    for git_dir in worktree_git_dirs(repo) {
        if (git_dir == common && main_is_bare)
            || canonicalize(&git_dir).is_ok_and(|path| path == this)
        {
            continue;
        }
        let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD")) else {
            continue;
        };
        if let Some(name) = head.strip_prefix("ref:").map(str::trim) {
            if name.starts_with("refs/heads/") {
                branches.insert(name.to_string());
            }
        }
    }
    for (git_dir, name) in rebased_branches(repo) {
        if canonicalize(&git_dir).is_ok_and(|path| path != this) {
            branches.insert(name);
        }
    }
    Ok(branches)
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
    let mut names = crate::hooks::SKIPPABLE_COMMIT_HOOKS.to_vec();
    if repo.state() == RepositoryState::Merge {
        // The commit concluding a clean merge runs it (`commit::CommitHookSet::CleanMerge`).
        names.push("pre-merge-commit");
    }
    let commit_hooks = crate::hooks::installed_hooks(repo, &names);
    let operation = paused_operation(repo);
    let head = match repo.head() {
        Ok(head) => head,
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {
            return Ok(HeadInfo {
                head_id: None,
                detached_at: None,
                head_pushed: false,
                operation,
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
