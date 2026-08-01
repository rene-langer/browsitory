mod branches;
mod commit;
mod conflict;
mod diff;
mod log;
mod merge;
mod rebase;
mod repo;
mod stage;
mod status;

pub use branches::list_local_branch_names;
pub use commit::create_commit;
pub use conflict::{ConflictSides, conflicted_paths, diff_blob_sides, read_conflict};
pub use diff::{DiffLine, FileDiff, LineKind, staged_file_diff, unstaged_file_diff, word_diff};
pub use log::{CommitInfo, commit_log};
pub use merge::{MergeOutcome, abort_merge, merge_branch};
pub use rebase::{
    RebaseAction, RebaseStatus, RebaseStep, abort_rebase, continue_rebase_step, drive_rebase_step,
    plan_rebase, start_rebase,
};
pub use repo::{GitError, Result, open};
pub use stage::{stage_path, unstage_path};
pub use status::{FileState, FileStatus, status};

pub use git2::{Oid, Rebase, Repository};
