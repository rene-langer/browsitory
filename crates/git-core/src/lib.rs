mod branch;
mod commit;
mod diff;
mod log;
mod repo;
mod stage;
mod stash;
mod status;

pub use branch::{
    BranchInfo, create_branch, delete_branch, list_branches, rename_branch, switch_branch,
};
pub use commit::create_commit;
pub use diff::{DiffLine, FileDiff, LineKind, staged_file_diff, unstaged_file_diff, word_diff};
pub use log::{CommitInfo, commit_log};
pub use repo::{GitError, Result, open};
pub use stage::{stage_path, unstage_path};
pub use stash::{StashEntry, apply_stash, create_stash, drop_stash, list_stashes, pop_stash};
pub use status::{FileState, FileStatus, status};

pub use git2::{Oid, Repository};
