//! External tools: the worker only *prepares* a diff/merge tool launch (reads config, writes
//! the temp side files). The caller spawns the tool, and waits for a merge tool off this
//! thread, so a tool window that stays open never blocks the repository's command queue.
//! None of these record an undo entry.

use crate::wire::ErrorDto;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use git_core::tools::{
    DiffSource, FileVersion, GitTools, PreparedDiffTool, PreparedMergeTool, MAX_FILE_VERSION_BYTES,
};

use super::{Command, ReadHandle, WorkerHandle};

pub(super) fn prepare_diff_tool(
    repo: &git2::Repository,
    path: String,
    source: DiffSource,
    reply: Sender<Result<PreparedDiffTool, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::tools::prepare_diff_tool(repo, &path, &source).map_err(ErrorDto::from_error),
    );
}

pub(super) fn prepare_merge_tool(
    repo: &git2::Repository,
    path: String,
    reply: Sender<Result<PreparedMergeTool, ErrorDto>>,
) {
    let _ =
        reply.send(git_core::tools::prepare_merge_tool(repo, &path).map_err(ErrorDto::from_error));
}

pub(super) fn file_sides(
    repo: &git2::Repository,
    path: String,
    source: DiffSource,
    reply: Sender<Result<git_core::diff::FileSides, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::diff::file_sides(repo, &source, &path, git_core::diff::MAX_PREVIEW_BYTES)
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn git_tools(repo: &git2::Repository, reply: Sender<Result<GitTools, ErrorDto>>) {
    let _ = reply.send(git_core::tools::git_tools(repo).map_err(ErrorDto::from_error));
}

pub(super) fn read_file_version(
    repo: &git2::Repository,
    path: String,
    version: FileVersion,
    reply: Sender<Result<Option<Vec<u8>>, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::tools::file_version_contents(repo, &path, &version, MAX_FILE_VERSION_BYTES)
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn check_merge_target(
    repo: &git2::Repository,
    path: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::tools::check_merge_target(repo, &path)
            .map(|_| ())
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn create_root_file(
    repo: &git2::Repository,
    name: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let Some(workdir) = repo.workdir() else {
        let _ = reply.send(Err(NO_WORKDIR.into()));
        return;
    };
    let result = git_core::root_file::create_root_file(workdir, &name);
    let _ = reply.send(result.map_err(ErrorDto::from_error));
}

/// The variables' values for a user command (#894): the work tree root (the git folder of a bare
/// repository) and what `context` names, checked against the repository.
pub(super) fn user_command_values(
    repo: &git2::Repository,
    context: &git_core::user_command::Context,
    reply: Sender<Result<git_core::user_command::Values, ErrorDto>>,
) {
    let root = repo.workdir().unwrap_or_else(|| repo.path());
    let _ = reply
        .send(git_core::user_command::values(repo, root, context).map_err(ErrorDto::from_error));
}

const NO_WORKDIR: &str = "repository has no working directory (bare repository)";

impl WorkerHandle {
    /// Work tree root captured at spawn (the repo path may be a subdirectory). No round trip
    /// to the worker.
    pub fn workdir(&self) -> Result<PathBuf, ErrorDto> {
        self.workdir.clone().ok_or_else(|| NO_WORKDIR.into())
    }

    /// Which of `.gitignore` and `.gitattributes` exist at the work tree root (#624); `None`
    /// for a bare repository. A file system check on this thread: no round trip to the worker.
    pub fn root_files(&self) -> Option<Vec<String>> {
        let workdir = self.workdir.as_deref()?;
        Some(git_core::root_file::existing_root_files(workdir))
    }

    /// Creates a missing `.gitignore` or `.gitattributes` at the work tree root, empty; leaves an
    /// existing one as it is. No other name or path.
    pub fn create_root_file(&self, name: String) -> Result<(), ErrorDto> {
        self.request(|tx| Command::CreateRootFile { name, reply: tx })
    }

    /// Resolves the merge tool and writes its side files. The caller runs it, off this worker.
    pub fn prepare_merge_tool(&self, path: String) -> Result<PreparedMergeTool, ErrorDto> {
        self.request(|tx| Command::PrepareMergeTool { path, reply: tx })
    }
}

impl ReadHandle {
    /// The checks a merge tool launch makes, for hosts that open their own merge editor
    /// (VSCode): a text conflict, no symlink or submodule, a work tree file inside the tree.
    pub fn check_merge_target(&self, path: String) -> Result<(), ErrorDto> {
        self.read(|tx| Command::CheckMergeTarget { path, reply: tx })
    }

    /// Both sides of `path` in `source` as bytes, each capped at `MAX_PREVIEW_BYTES`.
    pub fn get_file_sides(
        &self,
        path: String,
        source: DiffSource,
    ) -> Result<git_core::diff::FileSides, ErrorDto> {
        self.read(|tx| Command::GetFileSides {
            path,
            source,
            reply: tx,
        })
    }

    /// Resolves the diff tool and writes its side files. The caller launches it.
    pub fn prepare_diff_tool(
        &self,
        path: String,
        source: DiffSource,
    ) -> Result<PreparedDiffTool, ErrorDto> {
        self.read(|tx| Command::PrepareDiffTool {
            path,
            source,
            reply: tx,
        })
    }

    /// The variables' values for a user command run on `context` (#894).
    pub fn user_command_values(
        &self,
        context: git_core::user_command::Context,
    ) -> Result<git_core::user_command::Values, ErrorDto> {
        self.read(|tx| Command::UserCommandValues { context, reply: tx })
    }

    /// The diff and merge tools from the user's git config, for the Preferences dialog.
    pub fn get_git_tools(&self) -> Result<GitTools, ErrorDto> {
        self.read(|tx| Command::GetGitTools { reply: tx })
    }

    /// `path` in `version`, capped at `MAX_FILE_VERSION_BYTES` (VSCode's content provider).
    pub fn read_file_version(
        &self,
        path: String,
        version: FileVersion,
    ) -> Result<Option<Vec<u8>>, ErrorDto> {
        self.read(|tx| Command::ReadFileVersion {
            path,
            version,
            reply: tx,
        })
    }
}
