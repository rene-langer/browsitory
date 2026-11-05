#[cfg(unix)]
use std::path::Path;
use std::sync::mpsc;

use std::path::PathBuf;

use config::{OpenRepoEntry, Workspace};
use git_core::graph::GraphCommit;
use git_core::reflog::ReflogEntry;
use git_core::remote::{TransferErrorKind, TransferOperation, TransferPhase, TransferProgress};
use git_core::submodule::SubmoduleInfo;
use git_core::worktree::WorktreeInfo;

use repo_service::worker::{HookEvent, HookSource, TransferEvent};

use super::transfer_event_payload;
use repo_service::wire::{
    hook_progress_dto, BranchInfoDto, ChangedFileDto, CommitMessageSettingsDto, CommitOutcomeDto,
    CommitSignatureDto, CompareRangeDto, ConfirmedAbortDto, ConflictSideDto, DiffOptionsDto,
    DiffSettingsDto, FileHistoryEntryDto, FileSidesDto, ForgeProviderDto, GraphCommitDto,
    HeadInfoDto, MergeOptionsDto, MergeOutcomeDto, OpenRepoEntryDto, OpenRepoEntryInput,
    PickOutcomeDto, PullOutcomeDto, RebaseAbortDto, ReflogEntryDto, RemoteAuthModeDto,
    RemoteInfoDto, ResetModeDto, SearchPageDto, SequenceAbortDto, SequenceOutcomeDto,
    SignatureInputDto, StashApplyOutcomeDto, StashFilesDto, StashOptionsDto, StatusEntryDto,
    SubmoduleInfoDto, TagInfoDto, TrailerDto, TransferProgressPhaseDto, UndoOutcomeDto,
    UndoStateDto, UpstreamInfoDto, WorkspaceDto, WorktreeInfoDto,
};

mod dto_basics;
mod dto_history;
mod dto_operations;
mod dto_remote;
mod gate;
