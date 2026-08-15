# Browsitory Phase 4 Design

## Goal

Add four independently shippable repository-management capabilities: linked
worktrees, submodule management, reflog browsing and recovery, and pull-request
listing/creation for GitHub and Bitbucket.

## Scope and delivery order

Phase 4 is deliberately split into four implementation plans and releases:

1. Worktrees
2. Submodules
3. Reflog viewer and recovery
4. Pull-request integration

Each plan must leave the app in a working, tested state; later work does not
depend on an unfinished sibling feature. Pull-request integration is the only
network-facing Phase 4 workstream.

## Shared architecture

The existing boundary remains unchanged:

`React components/state -> RepoClient -> Tauri command -> Worker -> git-core`

Frontend components and state access backend functionality only through
`RepoClient`. New frontend tests mock that interface. Repository mutations run
on the existing per-repository worker thread; `git2::Repository` must not move
into Tauri managed state or be shared across threads.

New local Git operations belong in focused `git-core` modules and are exercised
against real temporary repositories. The worker adds matching commands and the
Tauri layer converts their DTOs at the IPC boundary.

## Worktrees

The worktree feature lists the main worktree plus linked worktrees, showing each
path, branch or detached HEAD identity, and lock/prune status when libgit2
reports it. Users can create a linked worktree by choosing an existing local
branch or a new branch based on a selected starting point. Creation rejects a
path that already exists and surfaces Git errors without deleting or replacing
anything.

Users can open a linked worktree as the active repository. Removal is available
only after the UI identifies the selected linked worktree and asks for explicit
confirmation. Removal must never target the main worktree and must report a
dirty worktree instead of forcing deletion. Prune is a separate explicit action
for stale administrative metadata.

## Submodules

The submodule feature lists every configured submodule with its path, configured
URL, recorded gitlink object ID, initialized state, and checked-out object ID
when available. It supports initializing and updating a selected submodule;
recursive update is explicit rather than the default. It does not create,
remove, or rewrite submodule configuration in this phase.

After a submodule update, the parent repository status refreshes so its gitlink
change can be staged and committed through the existing file-stage and commit
flow. A submodule's own working-tree changes are shown as information only;
opening it for normal repository actions uses the existing repository picker.

## Reflog viewer and recovery

The reflog feature lets the user choose a local reference (including `HEAD`)
and browse its entries in newest-first order. Each row shows the old and new
object IDs, committer identity and timestamp, reflog message, and a concise
target-commit summary when the new object resolves to a commit.

Recovery is limited to moving the selected local branch or `HEAD` to an entry's
new object ID. It presents the target reference and commit identity in an
explicit confirmation dialog. The first release does not alter remote-tracking
refs, expire reflog entries, or perform force pushes. Recovery updates only the
chosen local ref, then refreshes repository state.

## Pull requests

Pull-request integration supports GitHub and Bitbucket Cloud only. It detects a
supported provider and repository slug from a configured HTTPS or SSH remote;
unsupported, ambiguous, or malformed remotes show an actionable unavailable
state rather than making a request.

The app lists open pull requests for the selected supported remote and creates a
pull request with title, optional description, source branch, and target branch.
The release intentionally excludes comments, reviews, merges, approvals, and
PR-state mutations other than creation.

Provider HTTP clients live in `tauri-app`, behind a provider-neutral service and
DTOs exposed via `RepoClient`. `git-core` remains a UI-agnostic local Git layer.
The frontend never receives an API token after submission.

API tokens use the existing operating-system keychain abstraction, with a
provider-and-account-scoped key separate from Git transport credentials. A
saved Git HTTPS token is not implicitly reused for PR APIs: the user explicitly
saves a provider token, so scopes and account identity are clear. SSH-agent-only
remotes therefore require a saved forge token before PR operations are enabled.
Tokens are omitted from logs, errors, persisted config, URLs, and UI state.

GitHub requests use the REST API for repository pull-request listing and
creation. Bitbucket Cloud requests use its pull-request endpoints. Provider
responses are mapped to the same `PullRequest` DTO, while provider-specific
validation and error responses become concise, secret-free user messages.

## Testing and acceptance

- `git-core` tests use real `TempDir` repositories; no mocked `Repository`.
- Tauri worker and credential/service tests use real temporary repositories and
  in-memory trait implementations only at external seams such as the keychain
  and HTTP transport.
- Frontend tests mock `RepoClient`, never `@tauri-apps/api`.
- The worktree, submodule, and reflog plans add GUI E2E coverage where the
  feature spans picker/state/frontend plus the worker.
- PR tests use deterministic mock HTTP servers or injected transports; no live
  GitHub or Bitbucket accounts or tokens are required in CI.
- Any new dependency must use a permitted license and be recorded in
  `docs/LICENSE_COMPLIANCE.md`.
