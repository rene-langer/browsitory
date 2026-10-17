# Feature parity, batch 3a: clone and new repository

## Goal

Start working on a repository without leaving the app: clone a remote (with progress, cancel,
HTTPS token or SSH agent auth, optional branch, optional submodules) and create a new empty
repository. Tauri desktop app only.

Batch 3 is split: 3a clone + init (this spec), 3b undo stack, 3c commit signing. Each gets its
own spec, plan and PR.

## Current state

- `git-core/repo.rs`: `open` (`Repository::discover`) and `head_info`. No init, no clone.
- `git-core/remote.rs`: fetch/push/pull take `credentials: &mut dyn CredentialProvider`,
  `reporter: &mut dyn TransferReporter`, `cancel: &dyn Fn() -> bool`. `TransferProgress` /
  `TransferOperation` / `TransferErrorKind` classify progress and failures.
- `repo-service`: one worker thread per open repo; transfer cancel goes through the worker's
  shared cancel registry, keyed by repo path. `credentials.rs`: `CredentialService::save_https`
  keys keychain entries by URL + username (no repo needed); `RemoteCredentialProvider` picks the
  credential from the remote's `RemoteAuthMode` profile (stored in repo config).
- `git-core/submodule.rs`: `init_submodule`, `update_submodule(repo, path, recursive)` (no
  credential callbacks). `validate_remote_url_scheme` rejects `file://` — a guard against
  untrusted `.gitmodules` URLs.
- `tauri-app`: `pick_repo_folder` is a plain native folder picker (`blocking_pick_folder`).
- Frontend: `RepoPicker.tsx` toolbar has "Open folder" and "Open workspace root"; opening goes
  through `onOpenRepo(path)`.
- VSCode: the extension is bound to the workspace folder; VSCode itself already offers Git:
  Clone and Initialize Repository.

## Decisions

- **Tauri only.** VSCode client reports `supportsCloneAndInit = false`; entry points hidden there,
  methods reject. No sidecar changes.
- **Repo-less clone job, not the per-repo worker.** A `CloneService` in `repo-service` runs each
  clone on its own thread with its own cancel registry and reuses the transfer-progress event
  channel with a new `TransferOperation::Clone`. A blocking command without progress/cancel was
  rejected: large clones would show only a spinner.
- **Frontend picks the operation id** (`crypto.randomUUID()`), so it can cancel before
  `cloneRepo` resolves and `cloneRepo` resolves with the outcome directly.
- **HTTPS token is one-shot, saved only on success.** A wrong token is never persisted. After a
  successful clone: keychain `save_https(url, username, token)` and origin's auth profile set to
  `HttpsToken { username }`, so later fetch/push work without setup.
- **Clone URL validation is clone-specific.** A user-typed clone URL may be a local repo, so
  `file://` and local paths are allowed (unlike submodule URLs). Rejected: remote-helper syntax
  `<transport>::<address>` and empty input. Submodules updated during clone still go through
  `update_submodule` and its strict `file://` ban.
- **Options**: branch to check out, include submodules. No shallow, bare, mirror.
- **Submodule failures don't fail the clone.** Reported in the outcome.

## Backend (`git-core`)

### New `clone.rs`

```rust
pub struct CloneOptions {
    pub url: String,
    pub dest: PathBuf,
    pub branch: Option<String>,
    pub recurse_submodules: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneOutcome {
    pub path: PathBuf,
    /// Submodule paths whose init/update failed; the clone itself succeeded.
    pub submodule_failures: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CloneError {
    #[error("Unsupported URL.")]
    InvalidUrl,
    #[error("The URL contains a password or token.")]
    CredentialBearingUrl,
    #[error("Destination already exists and isn't empty.")]
    DestNotEmpty,
    #[error("Branch not found on remote.")]
    BranchNotFound,
    #[error("Clone cancelled.")]
    Cancelled,
    #[error("{0}")]
    Transfer(TransferErrorKind, String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub fn clone_repo(
    opts: &CloneOptions,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<CloneOutcome, CloneError>
```

Flow:

1. Validate URL: trimmed non-empty; reject `<transport>::<address>` (a `::` before any `/`) ⇒
   `InvalidUrl`; a URL with an embedded password ⇒ `CredentialBearingUrl` (it would otherwise
   be persisted in `.git/config`). A blank branch is treated as none.
2. Destination: absent ⇒ create it (remember `created = true`). Exists and is an empty directory
   ⇒ use it. Exists otherwise (file, or non-empty dir) ⇒ `DestNotEmpty`, nothing touched.
3. `git2::build::RepoBuilder` with `FetchOptions` + `RemoteCallbacks`:
   - `credentials` ⇒ the provider;
   - `transfer_progress` ⇒ report `Receiving` (`received_objects`/`total_objects`,
     `received_bytes`) and return `!cancel()`;
   - `CheckoutBuilder::progress` ⇒ report `Updating`;
   - `branch(b)` when set.
4. Failure:
   - cancel observed ⇒ `Cancelled`;
   - requested branch missing on remote (libgit2 `NotFound` on the branch ref) ⇒
     `BranchNotFound`;
   - otherwise classify like fetch (`MissingCredential`, `CredentialStoreFailure`,
     `SshAgentFailure`, `TransferFailed`) ⇒ `Transfer(kind, message)`.
   - Cleanup on every failure: `created` ⇒ remove `dest` recursively; else remove only `dest`'s
     contents (it was empty before we started).
5. Success with `recurse_submodules`: for each `list_submodules` entry, `init_submodule` then
   `update_submodule(path, recursive = true)`; any error pushes the path to
   `submodule_failures` and continues.

### `repo.rs`

```rust
pub fn init_repo(path: &Path) -> Result<(), RepoError>
```

- Creates `path` (and parents) if missing.
- `path/.git` exists ⇒ new `RepoError::AlreadyRepository`.
- `Repository::init_opts` with `initial_head` = global `init.defaultBranch`, else `main`.

## `repo-service`

- `clone.rs` (new): `CloneService` holds `Mutex<HashMap<String, Arc<AtomicBool>>>` of running
  clones.
  - `clone(operation_id, opts, https: Option<HttpsCredentialInput>, reporter) ->
    Result<CloneOutcome, CloneError>`: registers the cancel flag, runs `git_core::clone::clone_repo`
    on a dedicated thread, unregisters when done.
  - `cancel(operation_id)`: sets the flag; unknown id is a no-op.
- Credentials: with `https` set, a one-shot provider returns
  `Cred::userpass_plaintext(username, token)` from memory; without it, the existing
  `RemoteCredentialProvider` with `profile = None` (SSH ⇒ agent, HTTPS ⇒ `MissingCredential`,
  local ⇒ default). On success with `https` set: `CredentialService::save_https(url, username,
  token)` and `set_remote_auth_profile(origin, HttpsToken { username })`. A keychain failure here
  doesn't fail the clone; it's reported as `credential_save_failed: true` in the DTO so the UI
  can say "Cloned, but the token couldn't be saved."
- Clone emits `TransferEvent::Progress` only — never `Started`/`Completed`, which an open
  repo tab's pending fetch/push (`useRemoteTransferActions`) would adopt as its own transfer.
  Completion is the `clone_repo` call returning.
- `cancel(id)` before the clone registers still takes effect (flag created on cancel).
- `init_repo` needs no service state; the Tauri command calls `git_core::repo::init_repo`
  directly.

## IPC (Tauri)

Commands:

- `clone_repo(operation_id, url, dest, branch, recurse_submodules, https) -> CloneOutcomeDto`
  (`async`; progress emitted on the existing transfer-progress event with
  `operation: "Clone"`).
- `cancel_clone(operation_id)`.
- `init_repo(path)` (`async`, per the file's inline-command note).

Errors cross the boundary as `{ kind, message }` with `kind` one of `InvalidUrl`,
`CredentialBearingUrl`, `DestNotEmpty`, `BranchNotFound`, `Cancelled`, `AlreadyRepository`, or a `TransferErrorKind`.
The token is never logged or included in any error string.

`RepoClient` additions:

```ts
export interface CloneRequest {
  url: string;
  dest: string;
  branch: string | null;
  recurseSubmodules: boolean;
  https: { username: string; token: string } | null;
}
export interface CloneOutcome {
  path: string;
  submoduleFailures: string[];
  credentialSaveFailed: boolean;
}

readonly supportsCloneAndInit: boolean;
cloneRepo(operationId: string, request: CloneRequest): Promise<CloneOutcome>;
cancelClone(operationId: string): Promise<void>;
initRepo(path: string): Promise<void>;
```

`TransferOperation` gains `"Clone"`. `tauriRepoClient`: `supportsCloneAndInit = true`.
`vscodeRepoClient`: `false`, methods reject with "Not supported in the VSCode extension".

## Frontend

### Entry points

- `RepoPicker` toolbar: "Clone…" and "New repository…" after "Open folder", only when
  `client.supportsCloneAndInit`.
- Palette: "Clone repository…" and "New repository…" (same condition).

### `CloneDialog` (new component)

- Fields: URL; parent folder (text + Browse via `pickRepoFolder`); name (derived from the URL's
  last path segment minus `.git`, updated while the user hasn't edited it); branch (optional);
  "Include submodules" checkbox (default on). Destination = parent + name, shown below.
- HTTPS section (username, token) only for `http://`/`https://` URLs, collapsed by default.
- Clone button disabled until URL, parent and name are non-empty.
- Running: fields locked, progress bar ("Receiving objects n/total", then "Checking out…"),
  Cancel calls `cancelClone`.
- Errors inline, dialog stays open for retry:

| Kind | Message |
|---|---|
| `DestNotEmpty` | "⟨dest⟩ already exists and isn't empty." |
| `BranchNotFound` | "Branch ⟨b⟩ not found on the remote." |
| `InvalidUrl` | "Unsupported URL. Use https://, ssh://, git@host:path or a local path." |
| `MissingCredential` | HTTPS section expands: "This repository needs a username and token." |
| `SshAgentFailure` | "SSH agent unavailable or has no matching key." |
| `TransferFailed` | backend message |
| `Cancelled` | no error; form unlocked |

- Success: close dialog, `onOpenRepo(outcome.path)` (new tab + recents), toast
  "Cloned ⟨name⟩". With submodule failures: "Cloned ⟨name⟩ — N submodules failed to update"
  (Submodules panel has details). With `credentialSaveFailed`: "Cloned ⟨name⟩, but the token
  couldn't be saved."

### New repository

`pickRepoFolder` (native dialog can create a folder) ⇒ `initRepo` ⇒ `onOpenRepo` ⇒ toast
"Created repository ⟨name⟩". `AlreadyRepository` ⇒ `ConfirmDialog` "⟨path⟩ is already a
repository. Open it?" ⇒ `onOpenRepo` on confirm.

## Testing

- `git-core` `tests/clone.rs` (real temp repos; a local bare repo as remote, addressed by
  `file://` URL):
  - default branch cloned: worktree checked out, `origin` set, HEAD tracks `origin/<default>`;
  - `branch` option checks it out; unknown branch ⇒ `BranchNotFound`, created `dest` removed;
  - non-empty `dest` ⇒ `DestNotEmpty`, files untouched; existing empty dir is used, and kept
    (empty) after a failed clone;
  - cancel closure returning true ⇒ `Cancelled`, created `dest` removed;
  - reporter receives `Receiving` then `Updating`;
  - submodules: submodule addressed by plain local path is initialized and checked out; one with
    a `file://` URL lands in `submodule_failures` and the clone still succeeds;
  - `ext::sh -c ...` and empty URL ⇒ `InvalidUrl`.
- `git-core` `tests/repo.rs`: `init_repo` on a new dir ⇒ repo with HEAD on `main` (or
  `init.defaultBranch`); existing `.git` ⇒ `AlreadyRepository`; missing parents created.
- `repo-service`: cancel registry (cancel unknown id no-op, cancel running clone); with a fake
  `CredentialStore`: success saves credential + sets origin profile; failure saves nothing;
  keychain error on save ⇒ outcome `credential_save_failed`.
- `tauri-app`: DTO serialization for `CloneOutcomeDto`, `TransferOperation::Clone`, clone error
  kinds.
- Frontend (Vitest, mocked `RepoClient`): entry points hidden when unsupported; name derivation
  and manual-edit stickiness; HTTPS section visibility and expand on `MissingCredential`;
  progress rendering; Cancel; each error message; success opens repo and toasts (plus submodule
  and credential-save variants); New repository happy path and `AlreadyRepository` confirm;
  palette commands.
- E2E (Tauri): clone a `file://` bare fixture from the picker; new tab opens and shows the
  graph. No VSCode E2E (feature hidden there).

## Docs

- `docs/USER_GUIDE.md`: "Cloning and creating repositories" (URL types, HTTPS token saved only
  after a successful clone, SSH agent, submodules best-effort, desktop app only).
- `CHANGELOG.md` Unreleased: Added entry.

## Out of scope

Shallow, bare, mirror and partial clone; LFS; clone/init in VSCode; init templates; credentials
for submodule fetches (submodules use anonymous/agent auth, so a private HTTPS submodule lands
in `submodule_failures`).
