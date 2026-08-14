# Phase 3 Remote Workflows Design

## Purpose and scope

Phase 3 completes Browsitory's everyday remote workflow: users can manage multiple remotes and upstreams, fetch, pull, push branches and tags, create and delete local tags, view transfer progress, and authenticate through HTTPS credentials stored in the operating system's credential manager or through the local SSH agent.

The work is split into five independently shippable plans:

1. Remote and upstream management.
2. Transfer engine and progress reporting, beginning with fetch.
3. Pull, including fast-forward, merge, and rebase choices.
4. Push branches and tags, plus local tag management.
5. HTTPS credential storage and SSH-agent authentication.

The plans are ordered so durable remote configuration and a transfer seam exist before user-facing operations depend on them. Credential support is last because every transfer API accepts an injected authentication provider from the first plan; until it ships, local-file remotes remain fully testable without credentials.

This phase does not include cloning, force-push, force-updating or deleting remote tags, certificate or SSH-host-key overrides, Git credential-helper configuration, PR integration, worktrees, or submodules.

## Architecture

Phase 3 preserves the existing transport boundary:

React components -> RepoClient -> Tauri commands -> per-repo Worker -> git-core

crates/git-core/src/remote.rs is the UI-agnostic home for remote configuration and transfer operations. It owns remote CRUD, upstream tracking, remote/tracking-ref discovery, fetch, push, pull integration decisions, and local-tag operations. It receives callbacks for credentials and progress; it does not depend on Tauri, the operating-system credential store, or React.

crates/tauri-app supplies those callbacks. A credential service backed by the operating system's keychain returns an HTTPS username/token when libgit2 asks for it. SSH credentials use git2::Cred::ssh_key_from_agent and never expose key material to Browsitory. The worker emits typed, non-secret transfer events through Tauri while synchronous libgit2 callbacks run on the repository's own worker thread.

The frontend continues to depend only on RepoClient. The interface gains remote/tag/transfer methods plus a transport-neutral progress subscription; only tauriRepoClient.ts imports Tauri's event API. A future VS Code client can implement the same subscription over its own messaging transport.

The existing one-worker-per-open-repository rule remains in force. Only one transfer or history-mutating operation may run on a given repository worker at a time. Transfer controls are disabled while a transfer, merge, or rebase is active.

## Core domain contract

git-core::remote exposes data that carries no UI or secret state:

- RemoteInfo: name, fetch URL, optional push URL, and tracking branches.
- UpstreamInfo: local branch name, remote name, remote branch name, and ahead/behind counts when a tracking reference exists.
- TagInfo: full ref name, short name, target object ID, annotation message when present, tagger metadata when present, and whether it is annotated.
- TransferOperation: fetch, pull, push branch, or push tags.
- TransferProgress: operation ID, phase, current/total object counts, received-byte count, and optional side-band text. It contains no URL username, token, or private key data.

The remote module accepts an operation-scoped credential callback and progress reporter. It converts libgit2 callback data into owned, serializable progress values before forwarding it. A transfer has one opaque operation ID from start through completion or failure, allowing the frontend to discard stale events.

Remote configuration follows normal Git semantics: remotes are named and have a fetch URL, may have a distinct push URL, and a local branch may track one remote branch. Removing a remote first requires its associated local upstreams to be cleared in the same explicit user action; no unrelated remote or credential is removed.

HTTPS secrets are stored in the OS credential manager using a deterministic Browsitory service/account key derived from protocol, host, port, and username. The frontend sends a just-entered secret only to the credential-save command; it is never placed in useAppState, logged, returned by a command, or included in transfer events. A user can update or forget a stored credential explicitly.

The repository's local Git config records only the selected authentication mode and HTTPS username for each remote, so a later transfer can derive the matching keychain entry. Renaming a remote moves this metadata and removing one clears it. Neither the token nor any other secret is written to Git config.

The keyring 4.1.6 crate with its v1 feature supplies cross-platform keychain integration. Its Apache-2.0 license is permitted by this repository's license policy and is recorded in docs/LICENSE_COMPLIANCE.md when added.

git2 is configured with its https and ssh features. HTTPS uses stored username/token credentials; SSH uses the local SSH agent only. Browsitory does not read private-key files, prompt for a passphrase, or write credential.helper configuration.

## User experience and data flow

The app gains a Remote panel that lists remotes, their fetch and push URLs, and the currently checked-out branch's upstream. It supports adding, renaming, editing URLs, removing a remote, and setting or clearing the current branch's upstream. The active upstream determines the default fetch, pull, and push target; users can choose another remote explicitly before an operation.

Transfer controls provide Fetch, Pull, Push, and Push Tags. Starting one creates an operation ID, shows a single transfer panel, and disables competing repository mutations. Progress events update that panel with the current phase, object counts, byte totals, and server status text where libgit2 provides it. Completion refreshes graph commits, local branches and upstream status, remotes, tags, working-tree status, and the selected diff. Failure leaves the working tree untouched unless Git has already entered the existing merge or rebase workflow, in which case its existing resolution UI takes over.

Pull always fetches first. If the local branch and fetched upstream are equal, it reports that the branch is current. If a fast-forward is possible, it moves the local branch and checks out the new tree. If histories diverge, the app offers Merge, Rebase, or Cancel. Merge reuses Phase 2's merge conflict-resolution flow. Rebase starts Phase 2's interactive rebase workflow against the fetched tracking ref, including its existing progress and conflict handling. Pull refuses to alter HEAD when the worktree or index is dirty; Fetch remains available in that state.

Tag management lists local tags, creates either lightweight or annotated tags, and deletes a local tag only after confirmation. Push Tags pushes a user-chosen set of tags, or all local tags, to the selected remote. It never force-updates a remote ref and never deletes a remote tag.

The Remote panel also provides HTTPS credential save/update/forget controls and an SSH-agent selection. A missing credential produces an actionable request to save a token or configure the SSH agent before retrying; it never falls back to persisting a secret in a URL or Git configuration.

## Safety and errors

Fetch is permitted with a dirty worktree because it changes only remote-tracking references and object storage. Pull refuses a dirty worktree before any fast-forward, merge, or rebase action. Push rejects non-fast-forward updates; the UI reports that the user must first pull or reconcile history. No force option exists in this phase.

TLS certificate validation and SSH host-key checking retain libgit2's default rejecting behavior. The app offers no bypass. Credential errors distinguish a missing saved HTTPS credential, keychain read/write failure, and SSH-agent failure. Other user-facing error classes distinguish absent upstream, invalid remote configuration, dirty pull, non-fast-forward push, rejected remote ref, and generic Git transfer failure. Raw server messages may be displayed only after removing any URL userinfo; token, password, and private-key values never enter an error string.

All existing git-core typed-error conventions remain: RemoteError is mapped to Result<T, String> only at the worker/Tauri boundary. The frontend maps stable error kinds to concise remediation while keeping enough Git detail for diagnosis.

## Testing and acceptance criteria

Git-core tests use real temporary local repositories and a real temporary bare remote. They cover remote CRUD, upstream configuration, fetch updates, fast-forward pull, divergent pull planning, branch push, tag push, lightweight and annotated tags, local tag deletion, rejected non-fast-forward push, and dirty-worktree pull rejection. Recording credential/progress adapters are injected into operations; repositories themselves are never mocked.

Worker tests run against the same real local fixtures and assert that owned progress events cross out of libgit2 callbacks in operation order. Tauri command tests continue to pin every DTO string union to RepoClient.ts. The credential service is unit-tested behind a store trait with an in-memory test store, so CI does not write real user credentials.

Frontend tests mock RepoClient, never Tauri APIs. They cover the Remote panel CRUD forms, upstream selection, disabled controls, progress-event subscription and cleanup, pull's merge/rebase choice, tag confirmation, credential forms, and every stable error remediation. The frontend's Tauri implementation gets focused tests for event-to-RepoClient translation.

One E2E flow uses a local bare remote to verify remote creation, fetch, pull, push, tag creation, and tag push through the real desktop binary. Credential transport is not exercised with live accounts in CI. Release acceptance includes manual HTTPS-token save/reuse/forget and SSH-agent push/fetch checks against a test host, plus verification that no secret appears in Git config, logs, or the rendered UI after completion.

## Plan sequence

1. Remote and upstream management establishes remote.rs, DTOs, RepoClient methods, the Remote panel, and local-bare-repository coverage.
2. Transfer engine and progress adds the callback bridge, operation events, the transport-neutral subscription, Fetch, and the transfer panel.
3. Pull adds upstream comparison, clean-worktree enforcement, fast-forward checkout, and merge/rebase routing into the existing Phase 2 flows.
4. Push and tags adds branch push, local tag CRUD, selected/all-tag push, and non-fast-forward safety.
5. Credentials adds the keychain-backed HTTPS provider, SSH-agent provider, credential settings UI, redaction, and the manual release acceptance steps.

Each plan is independently testable and carries the required license and transport-isolation constraints forward.
