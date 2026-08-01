mod commit;
mod diff;
mod log;
mod repo;
mod stage;
mod status;

pub use commit::create_commit;
pub use diff::{DiffLine, FileDiff, LineKind, staged_file_diff, unstaged_file_diff, word_diff};
pub use log::{CommitInfo, commit_log};
pub use repo::{GitError, Result, open};
pub use stage::{stage_path, unstage_path};
pub use status::{FileState, FileStatus, status};

pub use git2::{Oid, Repository};
