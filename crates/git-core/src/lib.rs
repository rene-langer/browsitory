pub mod blame;
pub mod branch;
pub mod clone;
pub mod commit;
pub mod diff;
mod file_folder;
pub mod forge;
pub mod graph;
mod hardlink;
pub mod history;
pub mod hooks;
pub mod launch;
pub mod line_history;
mod line_patch;
pub mod locate;
pub mod merge;
mod partial_checkout;
pub mod pick;
pub mod process;
pub mod rebase;
pub mod rebase_file;
pub mod reflog;
pub mod remote;
pub mod repo;
pub mod reset;
pub mod search;
pub mod sequence_file;
pub mod signing;
pub mod stage;
pub mod stash;
pub mod status;
pub mod submodule;
pub mod tools;
pub mod tree;
pub mod undo;
pub mod worktree;
pub(crate) mod worktree_guard;
pub use file_folder::FileFolderConflict;
pub use worktree_guard::{
    CaseCollision, CaseConfigUnreadable, CasePair, CasePairBroughtIn, HeldDuring, HeldOpen,
    LinkedFolder,
};

// Hide the developer's / CI runner's global and system git config from every test (#111).
#[cfg(test)]
test_support::hermetic_git!();
