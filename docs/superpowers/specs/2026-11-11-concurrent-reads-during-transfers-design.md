# Reads during a fetch, pull or push (#734): design

Status: decided 2026-11-11 (see "Decisions"). Part of 0.11.0 (#755).

## Problem

A repository's worker runs one command at a time, and a Fetch, Fetch all, Pull, Push (all
variants), Delete remote branch/tag and the automatic background fetch hold it for the whole
transfer. Every read sent meanwhile waits until the transfer ends: a commit's files and diff,
blame, file and line history, search, compare, the stash file list, the next page of history, a
Refresh. Since #659 the transfer shows no modal dialog, so the window looks usable, but whatever
isn't loaded yet shows its loading state until the transfer finishes or is cancelled. On a slow
or hanging remote that can be minutes. The background fetch (every N minutes, up to its connect
limit against an unreachable remote) causes the same stall without the user having started
anything.

## The current model

### Worker, queue and command steps

- One OS thread per repository owns the only `git2::Repository` the worker uses
  (`crates/repo-service/src/worker/mod.rs:946-985`, `Worker::spawn_paused`): one
  `mpsc::channel::<Command>()`, `for command in rx { worker.step(command) }`.
- `WorkerHandle::request` (`mod.rs:1106-1116`) sends a command with a fresh reply channel and
  blocks on the reply. `is_alive` (`mod.rs:1126-1129`) relies on `send` failing once the worker's
  `Receiver` dropped (a panic). `cancel_transfer` and the search generation are side channels
  that bypass the queue (`mod.rs:1142-1144`, `SearchGeneration`).
- `WorkerLoop::step` (`worker_loop.rs:144-160`): `acquire` the state lock if the policy says so
  (`:165-207`), `before` (`:210-244`: undo `settle` for **every** command, reads included,
  `begin`, the operation gate), `dispatch` (`worker_loop/dispatch.rs`), `after` (`:247-264`:
  undo `finish` when tracking, which includes any command while an entry is pending; lock
  release; held reply release).
- `policy::of` (`policy.rs:152-421`) gives every `Command` a `CommandPolicy`; the reads
  (`policy.rs:161-219`) are all `P::free()`: no lock, no undo plan, `Gate::Open`. The transfers
  are `P::free()` too (`policy.rs:240-251`), except `PullCurrentUpstream`, which is
  `P::recorded("Pull", Full).gated(Refused)` (`policy.rs:393`): it holds the state lock and its
  undo "before" snapshot from before the fetch until after the merge.

### Transfers

- Every network transfer runs on a helper thread with its **own** `Repository`, opened from the
  worker's path (`transfer_thread.rs:50-116`, #547, #462). The worker polls the helper's message
  channel every `POLL` = 25 ms (`:29`, `:98-115`), forwards progress, and gives up on a helper
  that is still connecting when the user cancels or a background fetch's connect limit runs out
  (`give_up`, `:121-140`). The module doc states the invariant this issue is about:
  "The worker waits for the helper whatever happens, so the helper's writes never overlap another
  command" (`:15-16`).
- While the worker waits, its own `Repository` is idle: `run` uses it only for the path
  (`transfer_thread.rs:75`).
- Fetch, Fetch all and the pushes reply with the operation ID **before** the transfer starts
  (`remote.rs:287`, `:464`, `:499`, `:531`, `:568`, `:608`, `:646`; `fetch_all.rs:55`), then the
  worker stays busy until `Completed`. Pull replies only at the very end (`remote.rs:391`).
- Pull = fetch on the helper (`remote.rs:325-349`), then `pull_after_fetch` (merge or
  fast-forward: index, work tree, HEAD) and a `post-merge` hook on the worker (`:350-389`).
- What a helper writes while the worker waits: a fetch writes a new pack (indexer writes a
  temporary file, then renames `.pack`/`.idx` into `objects/pack/`), then updates the
  remote-tracking refs one by one with their reflogs (and auto-followed tags), `FETCH_HEAD`, and
  with prune deletes refs (loose files and a `packed-refs` rewrite). A push writes nothing locally
  until the remote accepted, then updates the pushed branch's remote-tracking ref; its `pre-push`
  hook runs on the helper after `connected`. No transfer writes the index, the work tree, HEAD,
  local branches or configuration (`git_core::remote::push_current_branch` sets no upstream;
  `set_branch_upstream` is its own command).
- Credentials are read on the helper (`RemoteCredentialProvider`; the OS store may prompt for a
  manual transfer). There is no askpass round trip through the worker queue.

### Transports

- **Tauri**: every command goes through `RepoWorker::call` (`crates/tauri-app/src/commands/mod.rs:493-505`),
  which first awaits the repository's gate (one async mutex per repository, #181) and then holds
  a blocking-pool thread for the round trip. Fetch and push release the gate after their early
  reply, so later reads queue in the worker. **Pull holds the gate for the whole pull**
  (`commands/remote.rs:390-400`), so during a pull Tauri reads don't even reach the worker.
- **VS Code sidecar**: each JSON-RPC request runs on its own thread
  (`crates/vscode-sidecar/src/main.rs:46-68`), no gate; reads queue in the worker.

### Frontend

- `transferSubject` (`frontend/src/state/useAppState.ts:180-183`) and `pending` mark a running
  transfer; `busyReason` (`frontend/src/lib/operationStatus.ts:291-297`) disables every mutation
  with "<transfer> is running" (#659). Reads are not disabled; they just wait.
- Refresh is never disabled (`frontend/src/lib/commands.ts:591-642`). The tab's reads (refreshes
  and history pages) run one after another, at most one refresh running and one waiting (#222,
  #774; `useAppState.ts:584-612`), so a refresh stuck behind a transfer also holds "load more".
- The change check (`checkForChanges`, `useAppState.ts:619-637`) and the watcher's reports
  (#756/#779) are suppressed while a transfer runs (`useExternalChanges`, `busy: pending ||
  transfer !== null`, `RepoWorkspace.tsx:393`); the transfer's own refresh follows `Completed`.

## What may run during a transfer

Safe beside a helper's transfer: commands that **only read** objects, refs, the index, the work
tree, the configuration or the app's state files, and write nothing in the repository. They can
observe a fetch half done (some remote-tracking refs moved, others not yet). That is transient,
self-healing (the transfer's own refresh follows `Completed`) and already happens today when a
terminal `git fetch` runs beside the app. They can't observe a missing object: the pack is renamed
into place before any ref names its objects, and libgit2 re-scans `objects/pack` when a lookup
misses (`git_odb_read` refreshes and retries).

Must wait (or, where it already is, be refused):

- Anything writing refs, the index, the work tree, HEAD or the configuration: staging, commits,
  branch/tag/remote changes, checkouts, resets, merges, rebases, picks, stashes, discards,
  `SetIdentity`, `SetCurrentUpstream`, worktree and submodule changes. A fetch's ref updates and
  prune race with branch/remote changes; a pull's merge races with everything touching the index
  or work tree.
- Undo/Redo and everything that takes the state lock or records undo (Pull holds the lock for its
  whole run).
- Another transfer (the cancel registry and `allocate_if_idle` already keep transfers apart).
- Commands that need worker state a transfer may be mid-way through, or that go to the network
  themselves: `ListPullRequests`, `CreatePullRequest`, credential and token commands.
- `PrepareMergeTool`: it leads to the external tool writing the conflicted file, then
  `MarkResolved`; it belongs with the conflict commands.
- Everything during the **merge phase of a pull** and its `post-merge` hook: the worker is then
  itself writing the index and work tree.

### The read set (proposed `Lane::Read`)

All of `policy.rs:161-219` except `GetHeadInfo`'s and `GetUndoState`'s special needs (handled
below, still in the set) and the exclusions above:

`GetStatus`, `GetChangeStamp`, `GetCommitGraph`, `LocateCommit`, `GetMergedCommits`,
`GetWorkingDiff`, `GetCommitDiff`, `GetCommitFiles`, `GetCommitStats`, `GetPatch`,
`PreviewPatch`, `GetCommitMessage`, `GetCommitSignature`, `GetCommitDetails`, `GetBlame`,
`GetFileHistory`, `GetLineHistory`, `SearchCommits`, `GetCommitRenameDiff`, `GetRangeFiles`,
`GetRangeDiff`, `GetWorkingTreeCompareFiles`, `GetWorkingTreeCompareDiff`,
`GetBranchCompareRange`, `ListBranches`, `ListWorktrees`, `ListSubmodules`, `ListReflogRefs`,
`GetReflog`, `ListRemotes`, `ListRemoteBranches`, `GetCurrentUpstream`, `GetRemoteUpstreams`,
`GetRemoteAuthMode`, `ListTags`, `ListStashes`, `GetStashFiles`, `GetMergeDefaults`,
`GetConflictHunks`, `GetMergeMessage`, `PrepareDiffTool` (temp files outside the repository),
`GetGitTools`, `GetFileSides`, `ListRecentAuthors`, `CheckMergeTarget`, `ReadFileVersion`,
`GetCommitTree`, `GetFileAtCommit`, `SaveFileVersion` (writes only outside the repository),
`CommitsSince`, `GetHeadInfo`, `DetectForgeRepository` (local config only),
`GetSwitchConflicts`, `GetPullDefaults`, `GetUndoState`.

That covers everything a refresh reads, so a Refresh during a transfer completes.

Stays on the main lane: `Ping`, `ListPullRequests`, `PrepareMergeTool` and every non-read.

## Options

### Option A: the worker serves reads while it waits for a transfer helper (recommended)

While `transfer_thread::run` waits for its helper, the worker thread's own `Repository` is idle.
Let the wait loop take queued **read-lane** commands and run them on that handle, between its
polls. Everything else stays queued until the transfer ends.

Pieces:

1. **Policy.** `CommandPolicy` gets `lane: Lane { Main, Read }`, set only by a new `P::read()`
   constructor (`free()` + `Read`), used for the list above. `policy_tests.rs` gets a lane column
   per row, and an invariant test: every `Lane::Read` policy is `free()` otherwise (no lock, no
   undo plan, no busy reply, `Gate::Open`, no hold, no abort).
2. **Two queues, one order.** `WorkerHandle::request` asks `policy::of` for the lane. Read-lane
   commands go into a second channel `reads`, plus a payload-free `Command::ReadReady` nudge into
   the main channel; the rest go into the main channel as today. Keeping `std::sync::mpsc` keeps
   `is_alive`, the "stopped worker" errors (#527) and the panic behaviour unchanged, and adds no
   dependency.
   - **Idle**: on `ReadReady` the loop takes one command from `reads` (if any) and runs it
     through the ordinary `step`, with today's undo `settle`/`finish` bookkeeping. With nothing
     running, the order of execution is the order of sending, as today.
   - **During a transfer wait**: each poll iteration takes at most one command from `reads`,
     runs it, forwards progress, checks `give_up`, and loops without sleeping while reads remain
     (`try_recv` instead of `recv_timeout` then). A read then answers within one poll (≤ 25 ms)
     plus its own run time. The `ReadReady` nudges these reads left in the main queue later find
     `reads` empty (or take a later read): no-ops.
   - Reads may overtake commands that are queued but not answered yet. That is linearizable: a
     caller only gets an ordering guarantee for a command whose reply it already has, and every
     caller that needs "my write, then my read" (`runMutation` → `refresh`) awaits the write's
     reply first. Fetch and push reply early, but they are the transfer itself, and their outcome
     arrives as `Completed`, after which the frontend refreshes.
3. **A read dispatcher without bookkeeping.** The read arms move out of `worker_loop/dispatch.rs`
   into `worker_loop/read.rs` (`ReadLane::dispatch(&mut self, repo: &mut Repository, Command)`,
   matching only lane commands; `step`'s `dispatch` delegates to it for the idle case). A read
   served during a wait skips `acquire`/`before`/`after`: it takes no lock, doesn't `settle` or
   `finish` undo (a stale pending entry is settled by the next main-lane command, as when no read
   came at all), and isn't gated (reads are `Gate::Open`). `ReadLane` borrows only what reads use:
   `&mut UndoRecorder` (for `GetUndoState`, whose `state()` only reloads `undo.json` if it
   changed), `&Operations` and `&Option<CleanMerge>` (for `GetHeadInfo`, whose `matches` needs
   `&mut Repository` because `mergehead_foreach` does), `&SearchGeneration`, `&Receiver` of
   `reads`. These are disjoint fields of `WorkerLoop`, so the transfer arms build it next to the
   credential service.
4. **Lend the handle to the wait.** `transfer_thread::run` takes `&mut Repository` and
   `&mut ReadLane` instead of `&Repository`; `on_helper`, `remote::{fetch, pull, push_*,
   delete_remote_*}`, `fetch_all::fetch_one` and the background fetch pass them through. Pull
   gets the repository back when `run` returns and merges as today.
5. **Tauri: a read gate.** `RepoWorker` gets a second per-repository gate and a
   `read(|r: &ReadHandle| ...)` entry, where `ReadHandle` exposes only the read-lane methods of
   `WorkerHandle` (so a mutation can't be sent through the read gate by accident; a read left on
   `call` is merely slower). A repository then holds at most two blocking-pool threads (#181's
   bound becomes 2, still independent of how many calls queue). This is what lets reads pass a
   running Pull, whose `call` holds the main gate. The VS Code sidecar needs no change.

What it buys and what it doesn't:

- Reads answer during every network phase: Fetch, Fetch all (not in the few milliseconds between
  two remotes), Pull's fetch, every push, Delete remote branch/tag, the background fetch, and the
  `pre-push` hook (it runs on the helper).
- Reads still wait during a pull's merge and `post-merge`, a commit's hooks or signing, a long
  checkout or rebase step: the worker is writing then. That keeps today's guarantee that a read
  never sees the worker's own half-done write.

Correctness:

- **git2 thread safety**: unchanged. The worker's handle stays on the worker thread; the helper
  has its own, as today. No `Repository` is shared.
- **libgit2 caches**: the worker's handle already reads refs and objects the helper wrote (after
  every transfer, today). Loose refs are read from disk per lookup, `packed-refs` is reloaded when
  its stat changes, and an object miss re-scans the pack folder, so a ref naming an object of the
  new pack resolves. The object cache and the mapped-window limits are process-wide in libgit2,
  so nothing doubles.
- **Packfile writes**: the indexer writes a temporary pack and renames it into place before any
  ref update; a read never sees refs naming objects it can't find.
- **Index lock**: no transfer writes the index, and no lane command writes it (libgit2's status
  doesn't write the index unless asked to update it; the change stamp ignores stat data).
- **State lock and undo**: a Pull holds the state lock and its "before" snapshot across the
  fetch; lane reads take neither and change nothing the snapshot covers. `GetUndoState` may still
  show the stack from before the pull, as it would have before the pull started.
- **Cancel latency**: a long read (blame on a huge file, a search) delays the next poll, so a
  Cancel while still connecting is honoured after that read, and progress arrives in a burst.
  Search stays cancellable by its generation. Acceptable for a first step; Option B removes it.

Windows file locking: the only new overlap is a read on the worker's handle while the helper
renames a new pack in, rewrites `packed-refs` (prune) or writes loose refs and reflogs. A terminal
`git fetch` beside the app produces exactly this overlap today. libgit2 opens files with
`FILE_SHARE_DELETE` and retries renames on Windows, and doesn't keep `packed-refs` mapped there;
the Windows CI leg of the tests below (fetch with prune while reads run) confirms this for the
pinned libgit2.

Memory: none (no second handle, no thread).

Complexity: moderate and local to repo-service plus one Tauri helper. The invariant "no read
overlaps the worker's own write" holds by construction (one thread), and "a read only overlaps a
helper's network phase" is a property of where `ReadLane::serve` is called.

### Option B: a read thread per repository with its own `Repository`

A second thread per worker opens its own `Repository` and runs every read-lane command, always,
not just during transfers.

- **Wins over A**: reads also pass hooks, signing, a long merge or rebase step and the merge phase
  of a pull; long reads never delay cancel polling or progress; reads run beside each other and
  beside writes.
- **Correctness costs**: reads now overlap the worker's **own** writes too: a status read halfway
  through a checkout, a graph read between a rebase's ref updates, a diff while a discard writes
  the file. Today only external tools cause that. `GetHeadInfo` needs the clean merge
  (`clean_merge`, worker memory) and the operation stores; `GetUndoState` needs the recorder.
  They would have to move behind `Arc<Mutex<…>>` or stay on the worker, and if they stay, every
  refresh still waits for the worker, which defeats most of the point. The reader must reopen its
  repository whenever the worker does (after worktree commands), and must be told when the
  worker's spawn path changes. Ordering between lanes is lost entirely: a read sent after a write
  was answered still runs on another handle, but at least it reads the file system after the
  write, so it's still linearizable.
- **Windows**: the reader keeps packfiles open and mapped while the worker's own commands write;
  the worker never deletes packs, so risk is similar to A, but a reader's open index or
  `packed-refs` during the worker's own rename is a new, frequent overlap rather than one limited
  to transfers.
- **Memory**: a second handle per open repository (its odb pack list, refdb, config snapshot,
  index); object cache shared by limit. A few MB per repository, up to 4 kept tabs, plus a thread.
- **Complexity**: high. Every read's assumptions about "nothing changes under me" need a review,
  and the frontend's staleness rules (DiffPane's refresh generation, the stamp key) would have to
  tolerate reads that raced the tab's own action.

A restricted variant (B only while a transfer runs) gets A's safety with a second handle and a
hand-off, buying only the lower cancel latency.

### Option C: the transfer leaves the worker; a ref-update lock guards it

The helper is detached: the worker returns to its loop as soon as the transfer starts, and a
"transfer in flight" guard makes the loop defer (or refuse with a busy reply) every command that
could race it, letting everything else run, including non-conflicting writes such as staging or a
commit during a push.

- **Wins**: the biggest UX change: staging, committing and browsing during long transfers.
- **Costs**: Pull must be split into a network phase and a continuation command that the helper's
  completion enqueues; its state lock and undo "before" snapshot then span other commands'
  `settle`/`finish`, which `undo.rs` is built not to allow (#172, #175). The conflict matrix
  ("which write races which transfer") is new policy for every command and for prune. The
  frontend's `pending` model (#659) would have to say which actions a running transfer blocks.
  The cancel and "abandoned helper writes nothing" reasoning (#547) needs redoing for a helper no
  one waits for.
- **Complexity**: very high; several PRs and a new spec for the frontend.

### Comparison

| | A: serve reads in the wait | B: read thread | C: detached transfer |
|---|---|---|---|
| Reads during network phase | yes | yes | yes |
| Reads during hooks, merges, pull's merge | no | yes | partly |
| Writes during a transfer | no | no | some |
| Reads see the worker's own half-done writes | never | yes | yes |
| Extra handle/thread | none | one each per repository | none (helper exists) |
| Cancel latency while connecting | + one read | unchanged | unchanged |
| Undo/state lock changes | none | `GetUndoState`/`GetHeadInfo` state sharing | pull split across commands |
| Size | M | L | XL |

## Effect on the change stamp, the watcher and refresh coalescing

Unchanged by A in behaviour, faster in practice:

- `GetChangeStamp` answers during a transfer. Its `refs` part may reflect a fetch half done.
  `checkForChanges` still doesn't run while a transfer is shown (`busy`), and the watcher's
  reports for the fetch's ref writes are dropped as today; the refresh after `Completed` reads a
  fresh stamp.
- A Refresh the user starts during a transfer now completes instead of waiting. The refresh after
  `Completed` either starts after it or joins the waiting slot (#222): the coalescing rule
  "a call while one runs shares the waiting one, which starts after the running one ends" already
  guarantees it reads after `Completed`. A history page ("load more") no longer waits behind a
  refresh stuck at the worker.
- The tab's read queue keeps reads in order, so a mid-transfer refresh can't overwrite the
  post-transfer one.

## Frontend

No change is required for the fix itself: the reads simply answer.

- **Becomes available during a fetch, push or a pull's fetch**: opening a commit (files, diff,
  details, signature, stats), blame, file and line history, search, compare (branches, ranges,
  work tree), stash files, the reflog view, the tree browser and file at commit, Save file
  version, opening the external diff tool, Refresh and loading more history.
- **Stays disabled** (`busyReason`, #659): every mutation, Undo/Redo, another transfer, the merge
  tool, pull-request listing and creation, remote and credential settings. Cancel stays.
- **Optional polish (small)**: the commit pane's loading state today may sit for minutes during a
  transfer; after A it does so only during a pull's merge phase, so no copy change is needed.

## Both transports

- **Tauri**: needs the read gate (Option A, step 5); without it a pull still blocks every read.
- **VS Code sidecar**: per-request threads already let reads reach the worker; A works there with
  no sidecar change. The extension host doesn't serialize requests per repository.

## Test strategy (Option A)

Deterministic, no timing guesses: a test waits for an event or a reply, never for "nothing
happened within N ms".

repo-service (`worker/tests/`, and the `transfer_thread` unit tests):

1. **Blocked fake transfer**: `transfer_thread::run` with a transfer closure that blocks on a
   test channel (the closure is generic, so no network). While it blocks: send reads into the
   lane; each answers. Send a stage command; release the transfer; assert the stage's reply
   arrives after `run` returned (record the order in a shared log). Repeat with a cancel while
   connecting to check `give_up` still runs between reads.
2. **Connecting**: `FetchRemote` against the existing `SilentServer` (`worker/tests/remote.rs:802`);
   `GetCommitFiles`, `GetStatus`, `GetHeadInfo`, `GetUndoState`, `GetChangeStamp` answer while no
   `Completed` was sent; a `StageFile` sent meanwhile answers only after `Completed` (assert the
   order of the event and the reply); Cancel ends it.
3. **Transferring**: a push to a local bare remote whose `pre-push` hook waits for a marker file
   (portable shell, as the hook tests do); reads answer while it waits; the test writes the marker.
4. **Pull**: reads answer during the pull's fetch (silent server); a test-only
   `PausePoint::PullMerge` stops the worker before `pull_after_fetch`; a read sent then answers
   only after `go()`, after the merge (order log). Background fetch: same as 2 with a connect
   limit.
5. **Fetch with prune while reads run** (also on the Windows and macOS CI legs): many remote
   refs, prune, `packed-refs` rewritten; a loop of `ListBranches`/`GetCommitGraph` reads during the
   fetch never errors.
6. **Idle order unchanged**: with no transfer, a stage then a status sent without awaiting answer
   in order (existing tests cover most of this; add one through the nudge path).
7. **Policy table**: the lane column and the `Lane::Read ⇒ free()` invariant.
8. **Stopped worker**: `is_alive` and `ErrorKind::WorkerStopped` behave the same for read-lane
   requests (a crashed worker drops both receivers).

tauri-app: with `RepoWorker::with_own_gate`, a `read` call completes while a `call` holds the
main gate (a call that waits on a channel the test releases).

E2E (Tauri, `e2e/specs/remote-transfer.spec.ts`): fetch from a silent remote (a Node `net`
server that accepts and never answers), click a commit not opened before, its files show while
the toolbar still says "Fetching"; Cancel. The VS Code e2e needn't repeat it: the worker is shared
and the sidecar needs no change.

## Recommendation

**Option A.** It fixes what the issue reports (reads during fetch, pull's fetch, push and the
background fetch), keeps every existing invariant (one thread per handle, reads never see the
worker's own half-done writes, undo and state lock untouched), adds no thread, handle or
dependency, and is testable deterministically. B and C can follow later if hooks and signing
(B) or writes during transfers (C) turn out to matter; A doesn't block either.

Estimated size: **M**, about 1 200 lines with tests, three PRs:

1. **repo-service** (~700 lines, ~250 production): `Lane` in the policy and its table column,
   `reads` channel + `ReadReady` nudge, `worker_loop/read.rs`, `transfer_thread::run` serving the
   lane, the `&mut Repository` plumbing through `remote.rs`/`fetch_all.rs`/`background_fetch.rs`,
   tests 1–8, `docs/ARCHITECTURE.md` "Threading model" and the `transfer_thread` module doc
   updated, changelog fragment ("Fixed: reads no longer wait behind a fetch, pull or push",
   VS Code fully, desktop except during a pull until PR 2).
2. **tauri-app** (~250 lines): the read gate, `ReadHandle`, read commands moved to `read`, its
   test, `ARCHITECTURE.md` #181 paragraph updated. With PR 1 merged first, the desktop app gains
   reads during fetch and push already; PR 2 adds pull.
3. **E2E** (~150 lines): the silent-remote flow above.

PRs 1 and 2 can be one PR if the owner prefers a single changelog entry; mutation testing then
covers both.

## Open questions for the owner

1. **Which option?** A (recommended), B, or C. A is the smallest change that fixes the reported
   case; B adds reads during hooks, signing and pull merges at the cost of reads seeing the app's
   own half-done writes; C allows writes during transfers and is a much larger project.
   *Recommendation: A.*
2. **Reads overtaking queued writes during a transfer.** (a) Allow it (linearizable; nothing in
   the frontend relies on the order of unanswered commands); (b) strict FIFO: serve a read during
   the wait only while no main-lane command is queued before it, so a stage queued from the other
   tab blocks all later reads. *Recommendation: (a).*
3. **Lane membership of the borderline commands**: `PrepareDiffTool` and `SaveFileVersion` (write
   only temp files or a user-picked path), `DetectForgeRepository` (local config only),
   `PrepareMergeTool` (leads to a work-tree write) and `ListPullRequests` (HTTP on the worker,
   would stall the polling). Options: (a) as listed above: diff tool, save and forge detection in,
   merge tool and pull requests out; (b) only the reads a refresh and the commit/diff panes use.
   *Recommendation: (a).*
4. **Pull's merge phase and hooks.** Reads still wait during `pull_after_fetch` and `post-merge`
   (and during commit hooks and signing, unchanged). (a) Accept for now; (b) file a follow-up for
   Option B restricted to hooks. *Recommendation: (a), and file the follow-up only if users report
   long `post-merge` or commit hooks.*
5. **Tauri gate shape.** (a) A second per-repository read gate (at most two blocking threads per
   repository); (b) reads ungated (no bound: a refresh storm against a stuck worker could fill the
   blocking pool again, the #181 problem). *Recommendation: (a).*
6. **Writes during a transfer** (staging or committing during a long push). Out of scope here;
   (a) leave it; (b) open an issue for Option C's write half. *Recommendation: (a) until someone
   asks for it.*
7. **E2E coverage.** (a) One Tauri flow (silent remote, open a commit); (b) none, relying on the
   repo-service and tauri-app tests. *Recommendation: (a): the gate change is only visible
   end-to-end.*

## Decisions

The owner took the recommendation on every question (2026-11-11):

1. Option A: the worker serves read-lane commands while it waits for a transfer helper.
2. A read may answer before an earlier write that is queued but not answered yet.
3. Read lane: `PrepareDiffTool`, `SaveFileVersion` and `DetectForgeRepository` are in;
   `PrepareMergeTool` and `ListPullRequests` stay on the main lane.
4. Reads still wait during a pull's merge, `post-merge` and commit hooks; no follow-up for now.
5. Tauri gets a separate per-repository read gate.
6. Writes during a transfer are out of scope; no issue is filed.
7. One Tauri E2E flow (a remote that never answers, then open a commit).

Implementation: three PRs in order (repo-service read lane, tauri-app read gate and
`ReadHandle`, the E2E flow); the last one closes #734.
