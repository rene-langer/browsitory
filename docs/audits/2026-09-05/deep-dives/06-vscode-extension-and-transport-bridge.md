# Deep-dive 06: VS Code extension and transport bridge

## Scope and audit questions

This deep-dive covers Browsitory's VS Code extension: the extension host
(`extension/`), the `vscode-sidecar` Rust binary and its JSON-RPC transport,
`crates/repo-service` (the transport-agnostic dispatch layer extracted to be
shared with `crates/tauri-app`), and the extension's CI/E2E coverage. Audit
questions: does the shipped implementation match the approved design spec's
architecture; does the sidecar preserve the desktop app's concurrency
guarantees; is the webview hardened against untrusted content; is
credential/config sharing with the desktop app actually implemented; and is
the extension adequately covered by CI.

## A note on method: this deep-dive audits `origin/main`, not the checked-out working tree

**This is the single most important fact in this deep-dive and the root
cause of most findings below.** The audit commit is local `main` at
`cd0cb63` ("docs(specs): add VSCode extension design spec (Phase 6)" —
its own commit message says the spec is "approved, not yet
planned/implemented"). That statement is stale: `git log --oneline
main..origin/main` shows the local branch is **4 commits behind
`origin/main`**:

```
3ebc59c ci(build-vsix): move Intel macOS runner off retired macos-13 (#69)
3318c44 fix(extension): resolve --sidecar path relative to repo root (#68)
bb8a9eb fix(frontend): keep commit-graph lane lines unbroken across forks (#67)
9bfd49e Ship Browsitory as a VSCode extension (Phase 6) (#66)
```

`git log --oneline origin/main..main` shows local `main` has exactly one
commit origin lacks: `cd0cb63` itself — i.e. local `main` was never
fast-forwarded past the point where PR #66 shipped the extension; someone
added the "not yet implemented" spec commit on top of the old tip instead.
`git merge-base --is-ancestor 9bfd49e HEAD` returns false, confirming the
extension implementation is not an ancestor of the audit commit at all.

Consequently, the audited working tree's `extension/` directory contains
**only stray build output** with no corresponding source: `extension/dist/*.js`
+ `.js.map`, `extension/artifacts/browsitory-0.1.0-linux-x64.vsix`,
`extension/node_modules/`, and `extension/e2e/` (itself containing a
`node_modules` and a 1.3GB `.vscode-test` Electron/Chromium download cache).
There is no `extension/src`, no `extension/package.json`, and no
`extension/e2e/src` under version control at this commit — `git status`
shows the entire tree as untracked, and the root `.gitignore` has no
`extension/` entry at all (see finding VSCE-002). The `extension.js.map`
sourcemap's `sources` field literally reads `["../src/extension.ts"]` — a
file that does not exist anywhere in this working tree or its history.

Auditing only what `git show HEAD:...` can see would mean auditing 1.3GB of
downloaded VS Code test-runner cache and a few minified `.js` bundles with no
way to trace them to source — far less useful to leadership than the real
question ("is the shipped extension sound?"). I therefore read the actual
extension source, the `vscode-sidecar`/`repo-service` Rust crates, and
`origin/main`'s CI workflow directly from git objects with `git show
origin/main:<path>` and `git ls-tree -r origin/main`, **without checking out
or modifying the working tree**. Every finding below cites `origin/main`
paths; treat this whole deep-dive as describing the state of `origin/main`
as of `3ebc59c`, not the literal audit-commit working tree. See finding
VSCE-001 for the branch-hygiene issue itself.

## Architecture examined

Per `docs/superpowers/specs/2026-08-30-vscode-extension-design.md` and
confirmed against `origin/main`:

```
extension host (Node, TypeScript) ──spawns──> vscode-sidecar (Rust binary)
        │  postMessage                              │ line-delimited JSON-RPC 2.0 / stdio
        ▼                                            ▼
   webview (React, frontend/dist-vscode)      repo_service::worker::Worker
                                               (one per open repo, same as tauri-app)
```

- `extension/src/extension.ts` — activation, webview panel creation, CSP-rendered
  HTML, wires `SidecarBridge` to the webview's `postMessage` channel.
- `extension/src/sidecarBridge.ts` — spawns/supervises the sidecar child process,
  JSON-RPC framing over stdio, native-method shims (folder picker, external URL,
  version tracking via `globalState`), reconnect-on-crash state machine.
- `extension/src/webviewHtml.ts` — renders the webview's HTML with a nonce-based CSP.
- `crates/vscode-sidecar/src/main.rs` — the sidecar's stdin/stdout JSON-RPC loop.
- `crates/vscode-sidecar/src/dispatch.rs` — method-name → `repo_service::worker::Worker`
  call dispatch table (~85 methods, matching the desktop app's `RepoClient` surface).
- `crates/repo-service/` — extracted from `crates/tauri-app` per the spec's sub-phase
  (a): `Worker`, the credential store (`credentials.rs`), pull-request/forge glue, and
  the per-domain worker modules (branch, merge, rebase, reflog, remote, stash, status,
  submodule, tag, worktree). `crates/tauri-app/src/commands/*.rs` on `origin/main` is
  now a thin adapter over the same crate, matching the spec's stated goal that this is
  "a refactor of existing tested code, not new logic."

## Evidence reviewed and checks run

- `git ls-tree -r origin/main -- extension/ crates/` — confirmed `extension/src/*`,
  `extension/e2e/src/*`, `crates/repo-service/`, `crates/vscode-sidecar/` all exist on
  `origin/main` with real source (not just scaffolding).
- Read in full: `extension/src/extension.ts`, `extension/src/sidecarBridge.ts`,
  `extension/src/webviewHtml.ts`, `extension/scripts/package-vsix.mjs`,
  `crates/vscode-sidecar/src/main.rs`, `crates/vscode-sidecar/src/protocol.rs` (head),
  `extension/package.json`.
- Read the top of `crates/vscode-sidecar/src/dispatch.rs` (method table) and the
  `fetch_remote`/`push_current_branch` handlers in full, plus every
  `spawn_progress_relay` call site.
- Read `crates/repo-service/src/credentials.rs`'s keyring service-name constants.
- Read `.github/workflows/ci.yml` on `origin/main` in full (job list and the `extension`
  and `e2e-vscode` job bodies) and diffed it against the audited commit's `ci.yml` (which
  has no extension-related job at all — confirming the extension is entirely absent from
  the audited commit's own CI surface, consistent with it also being absent from the
  audited commit's git history).
- Did not build or execute the sidecar, the extension, or its E2E suite — see Coverage
  gaps.

## Findings

### AUD-2026-09-05-VSCE-001 — Audited local `main` is 4 commits behind `origin/main`; the entire VS Code extension is missing from the audit commit's history

- **Severity:** Medium (process/integrity risk, not a code defect)
- **Confidence:** Confirmed
- **Components:** repository/branch state (not extension code itself)
- **Evidence:** `git log --oneline main..origin/main` lists `9bfd49e`, `bb8a9eb`,
  `3318c44`, `3ebc59c`; `git merge-base --is-ancestor 9bfd49e HEAD` → false; local
  `main`'s own tip commit `cd0cb63` claims the extension is "approved, not yet
  planned/implemented," which is false as of `origin/main`.
- **Impact:** Anyone auditing, branching from, or deploying from local `main` as it
  stands gets a version of the project that is unaware an entire shipped subsystem
  (with its own CI job, its own crates, its own release artifacts) exists. A release
  cut from this local branch, or a fresh clone of a mirror seeded from it, would
  silently omit Phase 6. It also means the working tree's stray `extension/`
  build artifacts (VSCE-002) have no corresponding source to explain them locally —
  exactly the confusing state this audit had to work around.
- **Trigger:** Anyone running standard local git hygiene (`git log`, `git status`,
  `cargo build --workspace`) against this checkout without first checking
  `git fetch && git status -sb` against `origin/main` would not discover the
  extension exists.
- **Remediation:** Fast-forward or rebase local `main` onto `origin/main` (there is
  exactly one local-only commit, `cd0cb63`, which itself needs rewording once the
  extension is visible again — it currently asserts something the branch it will sit
  on contradicts). Establish a local practice/hook that warns when `main` is behind
  its upstream before new commits are added to it.
- **Verification:** After syncing, `git merge-base --is-ancestor 9bfd49e HEAD` should
  return true, and `cd0cb63`'s content should be re-reviewed for continued accuracy
  now that Phase 6 has shipped.

### AUD-2026-09-05-VSCE-002 — Extension build artifacts (including a compiled `.vsix`) are untracked with no `.gitignore` coverage, sitting alongside a 1.3GB test cache

- **Severity:** Low
- **Confidence:** Confirmed
- **Components:** repository hygiene, `.gitignore`
- **Evidence:** Root `.gitignore` has entries for `frontend/dist`, `/target`,
  `e2e/node_modules`, but nothing for `extension/` or `frontend/dist-vscode/`
  (confirmed by `git check-ignore -v` returning no match for either
  `extension/dist/extension.js` or `extension/e2e/.vscode-test/user-data/DIPS`).
  `du -sh extension` reports 1.3G, almost entirely `extension/e2e/.vscode-test`
  (a downloaded VS Code + Electron test runner) and `node_modules`.
- **Impact:** A contributor running a broad `git add -A`/`git add .` on a branch with
  this directory present (as this audit's branch demonstrates can happen) risks
  staging a multi-hundred-megabyte-to-gigabyte accidental commit, including a compiled
  binary `.vsix` artifact — exactly the kind of accidental-secret/accidental-binary
  commit the repo's own CLAUDE.md warns against ("prefer adding specific files by
  name"). This is a hygiene/footgun risk, not an active defect, since nothing was
  actually committed.
- **Trigger:** `git add -A` (or an IDE "stage all") run from a working tree that has
  built the extension and/or run its E2E suite locally.
- **Remediation:** Add `extension/dist/`, `extension/artifacts/`, `extension/node_modules/`,
  `extension/e2e/node_modules/`, `extension/e2e/out/`, `extension/e2e/.vscode-test/`, and
  `frontend/dist-vscode/` to the root `.gitignore`, mirroring the existing
  `frontend/dist`/`e2e/node_modules` entries.
- **Verification:** `git status --porcelain` in a working tree with the extension built
  and its E2E suite run once should show no untracked files under `extension/` or
  `frontend/dist-vscode/`.

### AUD-2026-09-05-VSCE-003 — The sidecar's JSON-RPC dispatch loop is single-threaded and synchronous, reintroducing the concurrency serialization the desktop app's async-command design explicitly avoids

- **Severity:** High
- **Confidence:** Confirmed (static trace of `crates/vscode-sidecar/src/main.rs` on
  `origin/main`)
- **Components:** `crates/vscode-sidecar/src/main.rs`, `crates/vscode-sidecar/src/dispatch.rs`
- **Evidence:** `main.rs`'s entire request loop is:
  ```rust
  for line in stdin.lock().lines() {
      ...
      let response = match dispatch::dispatch(&request.method, request.params, &mut repos, &stdout) {
          Ok(result) => JsonRpcResponse::ok(request.id, result),
          Err(message) => JsonRpcResponse::err(request.id, message),
      };
      ...
  }
  ```
  This is a plain, single-threaded loop: no `tokio`/async runtime, no
  `thread::spawn` around the `dispatch::dispatch` call, no worker-pool. The next
  line of stdin is not even read until `dispatch(...)` returns. For any method that
  isn't one of the four `spawn_progress_relay`-based transfer operations
  (`fetch_remote`, `push_current_branch`, `push_tags`, `pull_current_upstream` —
  confirmed by grepping `spawn_progress_relay` call sites in `dispatch.rs`),
  `dispatch` calls straight into a `Worker` method that blocks on that worker
  thread's reply channel (the same request/response pattern
  `docs/ARCHITECTURE.md` describes for `tauri-app`). `docs/ARCHITECTURE.md`
  states this exact hazard for the desktop transport and explains why every
  Tauri command is `async fn`: *"anything that parks the calling thread ...
  freezes the whole window if the command is sync ... Commands clone the channel
  Sender out of the state mutex and drop the guard before blocking on a reply, so
  one slow repository operation can't serialize unrelated commands."* The sidecar
  has no equivalent: its one OS thread reading stdin *is* the "calling thread" for
  every non-transfer RPC, for every open repo, for the lifetime of the process.
- **Impact:** With two or more repositories open in the same VS Code window (a
  supported, expected use case — `repos: HashMap<String, Worker>` is explicitly
  keyed by path), any single slow request against *either* repo — a large `get_blame`,
  a `get_commit_graph`/`get_working_diff`/`commits_since` call against a big
  history, or even `commit`/`stage_hunk` on a large diff — blocks the sidecar
  from reading or answering *any other request* for *any other repo* until it
  returns. From the webview's perspective this looks like the whole extension
  freezing (a `get_status` poll on repo B hangs while repo A is mid-blame), which
  is precisely the desktop-app failure mode `docs/ARCHITECTURE.md` says the
  message-passing design was built to prevent.
- **Trigger:** Open two repositories in one VS Code window; run a slow operation
  (blame on a long-lived file, a large diff, a commit-graph render on a big repo)
  against one while interacting with the other. No malicious input needed — this
  is a straightforward two-repo, one-slow-operation scenario.
- **Remediation:** Either (a) dispatch each incoming request on its own thread
  (spawn-per-request, cheap given `Worker` handles are already `Clone`-able
  channel senders) and write responses through the existing `Arc<Mutex<Stdout>>`
  serialization point that `spawn_progress_relay` already establishes, or (b)
  adopt a small async runtime (`tokio`) for the stdin loop so each `dispatch` call
  becomes a spawned task. Either approach should keep the JSON-RPC framing
  (one line in, one line out) unchanged, so `sidecarBridge.ts` needs no changes.
- **Verification:** Add a sidecar-level test (or extend
  `crates/vscode-sidecar/tests/protocol_roundtrip.rs`) that opens two fixture
  repos, issues a slow request against one (e.g. a large synthetic history) and a
  fast `get_status` against the other concurrently, and asserts the fast request's
  response is not delayed by the slow one.

### AUD-2026-09-05-VSCE-004 — `sidecarBridge.ts` forwards any well-formed JSON-RPC message from the sidecar to the webview without confirming a matching request was ever made

- **Severity:** Informational
- **Confidence:** Confirmed
- **Components:** `extension/src/sidecarBridge.ts` (`relaySidecarLine`)
- **Evidence:** In `relaySidecarLine`, when an incoming line has a numeric `id` that
  isn't in `internalRequestWaiters`, the code does
  `this.pendingRequestIds.delete(message["id"])` (a no-op if absent) and then
  unconditionally calls `this.dependencies.postToWebview(message)` — there is no
  check that the `id` was actually outstanding in `pendingRequestIds` before
  forwarding.
- **Impact:** Low in practice: the sidecar is a first-party binary the extension
  itself spawns and pins by path (`resolvePackagedSidecarPath`/
  `resolveDevelopmentSidecarPath`), so this is not an externally-reachable attack
  surface under the current architecture. It is a latent correctness gap — a bug in
  the sidecar that emits a stray or duplicate response could reach the webview
  as if it were a real answer to a request the webview never made.
- **Trigger:** A hypothetical sidecar bug (e.g. a duplicate write, or a response
  keyed off a stale id after `handleProcessLoss` cleared `pendingRequestIds`) would
  reach `postToWebview` unfiltered.
- **Remediation:** Gate the forward on `this.pendingRequestIds.has(id)` (and delete
  only after confirming membership), matching the defensive pattern already used for
  `internalRequestWaiters`.
- **Verification:** Unit test: feed `relaySidecarLine` a response for an id never
  added via `handleWebviewMessage`, assert `postToWebview` is not called.

## Coverage gaps and open questions

- **Not executed:** this audit did not build `vscode-sidecar`, compile the extension,
  or run `extension/e2e` (the `@vscode/test-electron`-based suite) or
  `sidecarBridge.real.test.ts` (which spawns the real sidecar binary). VSCE-003's
  finding is a static trace, not a measured latency regression — recommended
  verification step above would convert it from a confirmed static defect into an
  empirically measured one.
- **Not verified end-to-end:** whether `frontend/src/ipc/vscodeRepoClient.ts` actually
  achieves full `RepoClient` parity with `tauriRepoClient.ts` (the spec's stated goal)
  was not diffed method-by-method against `dispatch.rs`'s ~85-entry match table; the
  method lists look consistent at a glance (branch/worktree/submodule/reflog/remote/
  stash/tag/merge/rebase/forge all present) but a systematic parity check is
  out of scope for this pass and would belong with deep-dive 01 (architecture/
  RepoClient boundary), which should cross-reference this finding.
- **Not verified:** whether `crates/tauri-app` on `origin/main` still passes its full
  test suite unchanged post-extraction, as the spec requires for sub-phase (a)
  ("the desktop app must keep passing its existing test suite unchanged after the
  extraction") — this would require checking out `origin/main` and running
  `cargo test --workspace`, which this deep-dive avoided to keep the working tree
  untouched. Recommend deep-dive 07 (verification strategy) or a follow-up run this
  check explicitly against `origin/main`.
- **Not assessed:** sidecar startup latency/cold-start UX, memory footprint of one
  sidecar process per VS Code window with N `Worker` threads inside it, and behavior
  when `extension/e2e`'s documented CI limitation (`GSETTINGS_BACKEND`/CDP port
  squatting workaround noted in `ci.yml`'s comments, referencing
  `docs/tasks/phase-6/d-02-vscode-e2e-harness.md`) actually manifests outside
  fresh GitHub-hosted runners (e.g. self-hosted or reused runners, as the workflow's
  own comment flags as an open risk).
- **Accepted risk, not a finding:** the design's choice to give the extension and
  desktop app the same OS-keychain service names (`com.browsitory.git`,
  `com.browsitory.forge`, confirmed in `crates/repo-service/src/credentials.rs`)
  is deliberate and matches the spec ("Config, recent repos, and saved remote
  credentials are shared"). This is correct behavior for the stated design, not a
  defect, but it does mean a credential-handling bug in one transport is a
  credential-handling bug for both — this deep-dive did not re-audit
  `credentials.rs`'s internals beyond confirming the shared service names, since
  the credential store's actual logic is desktop-app code covered by deep-dive 04
  (remote/forge/credential security) and is unchanged by the extension's addition.

## Strengths and positive controls

- **Webview CSP is well-built:** `webviewHtml.ts` uses `default-src 'none'`, a
  per-render random nonce for `script-src`, and scopes `img-src`/`font-src`/
  `style-src` to `webview.cspSource` — a materially stronger baseline than many
  VS Code extension webviews ship with, and consistent with the desktop app's
  general security posture.
- **Sidecar lifecycle handling is thorough:** `SidecarBridge` has an explicit state
  machine (`idle`/`running`/`reconnecting`/`failed`), detaches all process
  listeners on crash, rejects in-flight requests with a structured
  `transportStatus` notification instead of hanging them, and re-opens previously
  open repositories on respawn (`restorePersistedRepositories`) — this matches the
  spec's requirement that "`extension/` must detect sidecar exit and surface a
  reconnecting/failed state rather than hanging pending requests forever."
  `id`-space partitioning between webview-originated and internal (native-method
  restoration) requests via a descending counter from `Number.MAX_SAFE_INTEGER` is
  a deliberate, correct way to avoid collisions without a shared allocator.
- **The `repo-service` extraction matches its own spec:** `crates/repo-service`
  cleanly holds the transport-agnostic `Worker`/dispatch/credential logic, and
  `crates/tauri-app` and `crates/vscode-sidecar` both depend on it rather than
  duplicating ~85 methods' worth of dispatch logic — exactly the sub-phase (a)
  goal stated in the design spec, and it visibly succeeded (both crates exist and
  build against the shared crate on `origin/main`).
- **CI coverage is real, not aspirational:** `origin/main`'s `ci.yml` has both an
  `extension` job (unit tests, packaging staging test, `tsc` compile, lint) and a
  dedicated `e2e-vscode` job that builds the real sidecar, compiles the extension,
  and drives it inside an actual VS Code instance via `xvfb-run`, with
  `build-vsix` gated on both `e2e` and `e2e-vscode` passing before producing
  release-candidate `.vsix` artifacts per platform. This is a materially complete
  verification pipeline for a newly shipped subsystem, not a stub.
- **Packaging script is defensively written:** `package-vsix.mjs` validates its
  `target` against an explicit allow-list, requires all inputs to be non-empty
  strings before touching the filesystem, and cleans up the transient
  `bin/`/`webview/` staging directories in a `finally` block even on failure —
  reducing the chance of a stale/partial `.vsix` or leftover sidecar binary
  surviving a failed build.
