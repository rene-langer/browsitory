// Compile-time contract between the hand-written wire types in `RepoClient.ts` and the types
// ts-rs generates from the Rust DTOs (#306). `pnpm build` (`tsc -b`) fails when a Rust DTO and
// its TypeScript type differ in any field name, optionality or type. Both transports use the
// shared DTOs in `repo_service::wire` (`generated/wire-dto.ts`); the few that only one sends are
// in `generated/tauri-dto.ts` (desktop app) and `generated/sidecar-dto.ts` (VSCode sidecar).
// The generated files name the Rust test that rewrites them. A new DTO type needs a line here
// (`dtoContract.test.ts` fails until it has one).
import type * as Hand from "./RepoClient";
import type * as Wire from "./generated/wire-dto";
import type * as Tauri from "./generated/tauri-dto";
import type * as Sidecar from "./generated/sidecar-dto";

/** `true` only when `A` and `B` are the same type (not merely assignable either way). */
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;
/** Fails to compile, on that line, when `T` is not `true`. */
type Expect<T extends true> = T;
type Flat<T> = { [K in keyof T]: T[K] };
/** `T` with the fields `K` optional. */
type Loosen<T, K extends keyof T> = Flat<Omit<T, K> & Partial<Pick<T, K>>>;

// Deliberate differences, each written out below:
// - `BranchInfo`, `GraphCommit`, `RemoteInfo`, `UpstreamInfo`: both backends always send these
//   fields; `RepoClient.ts` keeps them optional so older test fixtures stay valid (`Loosen`).
//   `FileHistoryEntry`, `LineHistoryEntry`, `SearchPage` and `GraphPage` hold a `GraphCommit`, so
//   they are compared with the hand-written one in its place (`GraphCommit` itself is compared on
//   its own line).
// - `MergeToolOutcome` (desktop only): `"HandedOff"` comes from the VSCode extension host, never
//   from Tauri.
// - `listOpenRepos`: the sidecar answers `OpenRepos`, compared with the method's return type.
//   The desktop app's command returns an `(entries, activePath)` tuple, not a DTO, which
//   `tauriRepoClient.ts` turns into the same shape; only its `OpenRepoEntry` is compared.

export type DtoContract = [
  Expect<Equal<Hand.BisectInfo, Wire.BisectInfo>>,
  Expect<Equal<Hand.BlameLine, Wire.BlameLine>>,
  Expect<Equal<Hand.BranchInfo, Loosen<Wire.BranchInfo, "upstream" | "ahead" | "behind">>>,
  Expect<Equal<Hand.ChangedFile, Wire.ChangedFile>>,
  Expect<Equal<Hand.CloneOutcome, Tauri.CloneOutcome>>,
  Expect<Equal<Hand.CloneRequest, Tauri.CloneRequest>>,
  Expect<Equal<Hand.CommitChangeRequest, Wire.CommitChangeRequest>>,
  Expect<Equal<Hand.CommitHunk, Wire.CommitHunk>>,
  Expect<Equal<Hand.CommitMessageSettings, Wire.CommitMessageSettings>>,
  Expect<Equal<Hand.FetchSettings, Wire.FetchSettings>>,
  Expect<Equal<Hand.AppearanceSettings, Wire.AppearanceSettings>>,
  Expect<Equal<Hand.UiSize, Wire.UiSize>>,
  Expect<Equal<Hand.UpdateSettings, Wire.UpdateSettings>>,
  Expect<Equal<Hand.CommitOutcome, Wire.CommitOutcome>>,
  Expect<Equal<Hand.CommitSignature, Wire.CommitSignature>>,
  Expect<Equal<Hand.CompareRange, Wire.CompareRange>>,
  Expect<Equal<Hand.CommitLocation, Wire.CommitLocation>>,
  Expect<Equal<Hand.CollapsedMerge, Wire.CollapsedMerge>>,
  Expect<Equal<Hand.CommitStats, Wire.CommitStats>>,
  Expect<Equal<Hand.CommitStatsEntry, Wire.CommitStatsEntry>>,
  Expect<Equal<Hand.CommitDetails, Wire.CommitDetails>>,
  Expect<Equal<Hand.CommitPerson, Wire.CommitPerson>>,
  Expect<Equal<Hand.FileStats, Wire.FileStats>>,
  Expect<Equal<Hand.GraphSelection, Wire.GraphSelection>>,
  Expect<Equal<Hand.ConfirmedAbort, Wire.ConfirmedAbort>>,
  Expect<Equal<Hand.ConflictSegment, Wire.ConflictSegment>>,
  Expect<Equal<Hand.ConflictSide, Wire.ConflictSide>>,
  Expect<Equal<Hand.CreatePullRequest, Wire.CreatePullRequest>>,
  Expect<Equal<Hand.DiffHunk, Wire.DiffHunk>>,
  Expect<Equal<Hand.DiffLine, Wire.DiffLine>>,
  Expect<Equal<Hand.DiffLineOrigin, Wire.DiffLineOrigin>>,
  Expect<Equal<Hand.DiffAlgorithm, Wire.DiffAlgorithm>>,
  Expect<Equal<Hand.DiffOptions, Wire.DiffOptions>>,
  Expect<Equal<Hand.DiffSettings, Wire.DiffSettings>>,
  Expect<Equal<Hand.DiffSource, Wire.DiffSource>>,
  Expect<Equal<Hand.ErrorKind, Wire.ErrorKind>>,
  Expect<Equal<Hand.ExternalToolSettings, Wire.ExternalToolSettings>>,
  Expect<Equal<Hand.FastForwardMode, Wire.FastForwardMode>>,
  Expect<Equal<Hand.FileConflictChoice, Wire.FileConflictChoice>>,
  Expect<Equal<Hand.FileHistoryEntry, Flat<Omit<Wire.FileHistoryEntry, "commit"> & { commit: Hand.GraphCommit }>>>,
  Expect<Equal<Hand.FileSide, Wire.FileSide>>,
  Expect<Equal<Hand.FileSides, Wire.FileSides>>,
  Expect<Equal<Hand.TreeEntryKind, Wire.TreeEntryKind>>,
  Expect<Equal<Hand.TreeEntry, Wire.TreeEntry>>,
  Expect<Equal<Hand.TreeListing, Wire.TreeListing>>,
  Expect<Equal<Hand.FileContent, Wire.FileContent>>,
  Expect<Equal<Hand.ForgeProvider, Wire.ForgeProvider>>,
  Expect<Equal<Hand.ForgeRepository, Wire.ForgeRepository>>,
  Expect<Equal<Hand.GitConfigSource, Wire.GitConfigSource>>,
  Expect<Equal<Hand.GitToolInfo, Wire.GitToolInfo>>,
  Expect<Equal<Hand.GitTools, Wire.GitTools>>,
  Expect<Equal<Hand.GraphCommit, Loosen<Wire.GraphCommit, "tags">>>,
  Expect<Equal<Hand.HeadInfo, Wire.HeadInfo>>,
  Expect<Equal<Hand.HistoryEditKind, Wire.HistoryEditKind>>,
  Expect<Equal<Hand.HistoryEditTarget, Wire.HistoryEditTarget>>,
  Expect<Equal<Hand.HookProgress, Wire.HookProgress>>,
  Expect<Equal<Hand.RepoWatch, Wire.RepoWatch>>,
  Expect<Equal<Hand.RepoWatchKind, Wire.RepoWatchKind>>,
  Expect<Equal<Hand.HookSource, Wire.HookSource>>,
  Expect<Equal<Hand.MarkResolvedOutcome, Wire.MarkResolvedOutcome>>,
  Expect<Equal<Hand.MergeOptions, Wire.MergeOptions>>,
  Expect<Equal<Hand.MergeOutcome, Wire.MergeOutcome>>,
  Expect<Equal<Hand.MergeToolOutcome, Tauri.MergeToolOutcome | "HandedOff">>,
  Expect<Equal<Hand.OpenRepoEntry, Wire.OpenRepoEntry>>,
  Expect<Equal<Hand.OperationKind, Wire.OperationKind>>,
  Expect<Equal<Hand.PatchContent, Wire.PatchContent>>,
  Expect<Equal<Hand.PatchApply, Wire.PatchApply>>,
  Expect<Equal<Hand.PatchPreview, Wire.PatchPreview>>,
  Expect<Equal<Hand.PatchPreviewPart, Wire.PatchPreviewPart>>,
  Expect<Equal<Hand.PatchMail, Wire.PatchMail>>,
  Expect<Equal<Hand.PatchPreviewFile, Wire.PatchPreviewFile>>,
  Expect<Equal<Hand.PatchFileStatus, Wire.PatchFileStatus>>,
  Expect<Equal<Hand.PatchSource, Wire.PatchSource>>,
  Expect<Equal<Hand.PickOutcome, Wire.PickOutcome>>,
  Expect<Equal<Hand.PullOutcome, Wire.PullOutcome>>,
  Expect<Equal<Hand.PullStrategy, Wire.PullStrategy>>,
  Expect<Equal<Hand.PullRequest, Wire.PullRequest>>,
  Expect<Equal<Hand.PullRequestList, Wire.PullRequestList>>,
  Expect<Equal<Hand.RebaseAbort, Wire.RebaseAbort>>,
  Expect<Equal<Hand.RebaseAction, Wire.RebaseAction>>,
  Expect<Equal<Hand.RebaseDivergence, Wire.RebaseDivergence>>,
  Expect<Equal<Hand.RebaseHealth, Wire.RebaseHealth>>,
  Expect<Equal<Hand.RebaseInfo, Wire.RebaseInfo>>,
  Expect<Equal<Hand.RebasePlanCommit, Wire.RebasePlanCommit>>,
  Expect<Equal<Hand.RebasePlanEntry, Wire.RebasePlanEntry>>,
  Expect<Equal<Hand.RebaseStepResult, Wire.RebaseStepResult>>,
  Expect<Equal<Hand.LineHistoryEntry, Flat<Omit<Wire.LineHistoryEntry, "commit"> & { commit: Hand.GraphCommit }>>>,
  Expect<Equal<Hand.LineHistory, Flat<Omit<Wire.LineHistory, "entries"> & { entries: Hand.LineHistoryEntry[] }>>>,
  Expect<Equal<Hand.ReflogEntry, Wire.ReflogEntry>>,
  Expect<Equal<Hand.RepoErrorPayload, Wire.RepoErrorPayload>>,
  Expect<Equal<Hand.RemoteAuthMode, Wire.RemoteAuthMode>>,
  Expect<Equal<Hand.RemoteInfo, Loosen<Wire.RemoteInfo, "prune">>>,
  Expect<Equal<Hand.ResetMode, Wire.ResetMode>>,
  Expect<Equal<Hand.RepoAlias, Wire.RepoAlias>>,
  Expect<Equal<Hand.SearchPage, Flat<Omit<Wire.SearchPage, "commits"> & { commits: Hand.GraphCommit[] }>>>,
  Expect<Equal<Hand.GraphPage, Flat<Omit<Wire.GraphPage, "commits"> & { commits: Hand.GraphCommit[] }>>>,
  Expect<Equal<Hand.SequenceAbort, Wire.SequenceAbort>>,
  Expect<Equal<Hand.SequenceKind, Wire.SequenceKind>>,
  Expect<Equal<Hand.SequenceOutcome, Wire.SequenceOutcome>>,
  Expect<Equal<Hand.SequenceProgress, Wire.SequenceProgress>>,
  Expect<Equal<Hand.SignatureInput, Wire.SignatureInput>>,
  Expect<Equal<Hand.SignatureStatus, Wire.SignatureStatus>>,
  Expect<Equal<Hand.StashApplyOutcome, Wire.StashApplyOutcome>>,
  Expect<Equal<Hand.StashSwitchOutcome, Wire.StashSwitchOutcome>>,
  Expect<Equal<Hand.StashEntry, Wire.StashEntry>>,
  Expect<Equal<Hand.StashFiles, Wire.StashFiles>>,
  Expect<Equal<Hand.StashOptions, Wire.StashOptions>>,
  Expect<Equal<Hand.StatusEntry, Wire.StatusEntry>>,
  Expect<Equal<Hand.StatusKind, Wire.StatusKind>>,
  Expect<Equal<Hand.SubmoduleInfo, Wire.SubmoduleInfo>>,
  Expect<Equal<Hand.TagInfo, Wire.TagInfo>>,
  Expect<Equal<Hand.TerminalRebase, Wire.TerminalRebase>>,
  Expect<Equal<Hand.TerminalSequence, Wire.TerminalSequence>>,
  Expect<Equal<Hand.Trailer, Wire.Trailer>>,
  Expect<Equal<Hand.TransferErrorKind, Wire.TransferErrorKind>>,
  Expect<Equal<Hand.TransferOperation, Wire.TransferOperation>>,
  Expect<Equal<Hand.TransferProgress, Wire.TransferProgress>>,
  Expect<Equal<Hand.UndoOutcome, Wire.UndoOutcome>>,
  Expect<Equal<Hand.UndoState, Wire.UndoState>>,
  Expect<Equal<Hand.ChangeStamp, Wire.ChangeStamp>>,
  Expect<Equal<Hand.UpstreamInfo, Loosen<Wire.UpstreamInfo, "ahead" | "behind" | "pushBranch">>>,
  Expect<Equal<Hand.WordWrap, Wire.WordWrap>>,
  Expect<Equal<Hand.Workspace, Wire.Workspace>>,
  Expect<Equal<Hand.WorktreeInfo, Wire.WorktreeInfo>>,
  // `persistOpenRepos` sends `OpenRepoEntry`s; the backends read them as `OpenRepoEntryInput`.
  Expect<Equal<Hand.OpenRepoEntry, Wire.OpenRepoEntryInput>>,
  Expect<Equal<Awaited<ReturnType<Hand.RepoClient["listOpenRepos"]>>, Sidecar.OpenRepos>>,
];
