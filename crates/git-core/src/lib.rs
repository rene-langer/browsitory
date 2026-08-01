mod blame;
mod branch;
mod commit;
mod conflict;
mod credentials;
mod diff;
mod graph;
mod log;
mod merge;
mod rebase;
mod remote;
mod repo;
mod stage;
mod stash;
mod status;
mod transfer;

pub use blame::{BlameLine, blame_file};
pub use branch::{
    BranchInfo, create_branch, delete_branch, list_branches, rename_branch, switch_branch,
};
pub use commit::create_commit;
pub use conflict::{ConflictSides, conflicted_paths, diff_blob_sides, read_conflict};
pub use credentials::make_callbacks;
pub use diff::{DiffLine, FileDiff, LineKind, staged_file_diff, unstaged_file_diff, word_diff};
pub use graph::{GraphCommit, graph_log};
pub use log::{CommitInfo, commit_log};
pub use merge::{MergeOutcome, abort_merge, merge_branch};
pub use rebase::{
    RebaseAction, RebaseStatus, RebaseStep, abort_rebase, continue_rebase_step, drive_rebase_step,
    plan_rebase, start_rebase,
};
pub use remote::{
    RemoteInfo, add_remote, list_remotes, remove_remote, rename_remote, set_remote_url,
};
pub use repo::{GitError, Result, open};
pub use stage::{stage_path, unstage_path};
pub use stash::{StashEntry, apply_stash, create_stash, drop_stash, list_stashes, pop_stash};
pub use status::{FileState, FileStatus, status};
pub use transfer::{ProgressUpdate, TransferStage, push, push_tag};

pub use git2::{Oid, Rebase, Repository};
