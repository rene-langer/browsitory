//! External tools: the worker only *prepares* a diff/merge tool launch (reads config, writes
//! the temp side files). The caller spawns the tool, and waits for a merge tool off this
//! thread, so a tool window that stays open never blocks the repository's command queue.
//! None of these record an undo entry.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use git_core::tools::{
    DiffSource, FileVersion, GitTools, PreparedDiffTool, PreparedMergeTool, MAX_FILE_VERSION_BYTES,
};

use super::{Command, WorkerHandle};

pub(super) fn prepare_diff_tool(
    repo: &git2::Repository,
    path: String,
    source: DiffSource,
    reply: Sender<Result<PreparedDiffTool, String>>,
) {
    let _ = reply
        .send(git_core::tools::prepare_diff_tool(repo, &path, &source).map_err(|e| e.to_string()));
}

pub(super) fn prepare_merge_tool(
    repo: &git2::Repository,
    path: String,
    reply: Sender<Result<PreparedMergeTool, String>>,
) {
    let _ = reply.send(git_core::tools::prepare_merge_tool(repo, &path).map_err(|e| e.to_string()));
}

pub(super) fn file_sides(
    repo: &git2::Repository,
    path: String,
    source: DiffSource,
    reply: Sender<Result<git_core::diff::FileSides, String>>,
) {
    let _ = reply.send(
        git_core::diff::file_sides(repo, &source, &path, git_core::diff::MAX_PREVIEW_BYTES)
            .map_err(|e| e.to_string()),
    );
}

pub(super) fn git_tools(repo: &git2::Repository, reply: Sender<Result<GitTools, String>>) {
    let _ = reply.send(git_core::tools::git_tools(repo).map_err(|e| e.to_string()));
}

pub(super) fn read_file_version(
    repo: &git2::Repository,
    path: String,
    version: FileVersion,
    reply: Sender<Result<Option<Vec<u8>>, String>>,
) {
    let _ = reply.send(
        git_core::tools::file_version_contents(repo, &path, &version, MAX_FILE_VERSION_BYTES)
            .map_err(|e| e.to_string()),
    );
}

pub(super) fn check_merge_target(
    repo: &git2::Repository,
    path: String,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::tools::check_merge_target(repo, &path)
            .map(|_| ())
            .map_err(|e| e.to_string()),
    );
}

impl WorkerHandle {
    /// The checks a merge tool launch makes, for hosts that open their own merge editor
    /// (VSCode): a text conflict, no symlink or submodule, a work tree file inside the tree.
    pub fn check_merge_target(&self, path: String) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::CheckMergeTarget { path, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// Work tree root captured at spawn (the repo path may be a subdirectory). No round trip
    /// to the worker.
    pub fn workdir(&self) -> Result<PathBuf, String> {
        self.workdir
            .clone()
            .ok_or_else(|| "repository has no working directory (bare repository)".to_string())
    }

    /// Resolves the diff tool and writes its side files. The caller launches it.
    /// Both sides of `path` in `source` as bytes, each capped at `MAX_PREVIEW_BYTES`.
    pub fn get_file_sides(
        &self,
        path: String,
        source: DiffSource,
    ) -> Result<git_core::diff::FileSides, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetFileSides {
                path,
                source,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn prepare_diff_tool(
        &self,
        path: String,
        source: DiffSource,
    ) -> Result<PreparedDiffTool, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::PrepareDiffTool {
                path,
                source,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// Resolves the merge tool and writes its side files. The caller runs it, off this worker.
    pub fn prepare_merge_tool(&self, path: String) -> Result<PreparedMergeTool, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::PrepareMergeTool { path, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// The diff and merge tools from the user's git config, for the Preferences dialog.
    pub fn get_git_tools(&self) -> Result<GitTools, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetGitTools { reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// `path` in `version`, capped at `MAX_FILE_VERSION_BYTES` (VSCode's content provider).
    pub fn read_file_version(
        &self,
        path: String,
        version: FileVersion,
    ) -> Result<Option<Vec<u8>>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ReadFileVersion {
                path,
                version,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
