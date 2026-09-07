# Deep-dive 09: Documentation and Operability

## Purpose, scope, audit questions

Does the repository's own documentation (`CLAUDE.md`, `docs/ARCHITECTURE.md`, `README.md`,
`docs/USER_GUIDE.md`, `docs/LICENSE_COMPLIANCE.md`, `CHANGELOG.md`, and the
`docs/superpowers/specs/*` design documents) accurately describe the state of the codebase at
the audit commit? Do the documented developer-setup and build commands work as written? Is the
diagnostic/operability surface added in 0.2.0 (rotated failure log) itself documented anywhere a
support engineer or end user would find it?

This deep-dive intentionally does **not** re-audit the VSCode extension's implementation
(see [`06-vscode-extension-and-transport-bridge.md`](./06-vscode-extension-and-transport-bridge.md))
or the CI/release pipeline gap for it (see
[`08-build-ci-release-and-supply-chain.md`](./08-build-ci-release-and-supply-chain.md)). It covers
only the documentation/status-tracking failure itself.

## Evidence reviewed

- `CLAUDE.md` (project instructions, checked into the repo, read in full as part of every tool
  call's context this session).
- `docs/superpowers/specs/2026-08-30-vscode-extension-design.md` (full read).
- `CHANGELOG.md` (tail read, `[Unreleased]`/`[0.2.0]` sections).
- `docs/ARCHITECTURE.md` (full read).
- `git log`/`git show`/`git diff` against `origin/main` (read-only; no checkout, merge, or pull
  performed).
- `docs/USER_GUIDE.md`, `README.md`, `docs/LICENSE_COMPLIANCE.md` (spot-read for staleness).

## Findings

### AUD-2026-09-05-DOC-001 — Project-status documentation is stale relative to `origin/main`; the committed "not yet implemented" claim is false

- **Severity:** High
- **Confidence:** Confirmed
- **Affected components:** `CLAUDE.md` ("Project status" section), `docs/superpowers/specs/2026-08-30-vscode-extension-design.md` (line 3), repository-wide status tracking.
- **Evidence:**
  - `CLAUDE.md`'s "Project status" section, as committed at the audit commit (`cd0cb63`),
    describes the current state as Phase 5 (UI/UX polish), and only mentions a VSCode
    extension as forward-looking architectural rationale ("a UI that can't be reused as a
    VSCode webview, which is a stated future requirement... so a VSCode extension can
    implement the same interface later without touching UI code"). It nowhere states that a
    VSCode extension has been designed, planned, or built.
  - `docs/superpowers/specs/2026-08-30-vscode-extension-design.md:3` states in full: `Status:
    approved, not yet planned/implemented.`
  - The audit commit `cd0cb63` itself is titled `docs(specs): add VSCode extension design spec
    (Phase 6)` — i.e., the local branch's most recent commit is exactly the one that added the
    "not yet implemented" status line.
  - This is demonstrably false as of the audit date. `origin/main` (the same repository's
    GitHub-hosted remote, fetched and present locally as `refs/remotes/origin/main`) is four
    commits ahead of the locally checked-out `main` branch and contains a fully merged,
    tested, and (per `08`) partially CI-wired VSCode extension implementation:
    - `9bfd49e` — "Ship Browsitory as a VSCode extension (Phase 6) (#66)" (merge commit)
    - `bb8a9eb` — "fix(frontend): keep commit-graph lane lines unbroken across forks (#67)"
    - `3318c44` — "fix(extension): resolve --sidecar path relative to repo root (#68)"
    - `3ebc59c` — "ci(build-vsix): move Intel macOS runner off retired macos-13 (#69)"
  - `git log --oneline 9bfd49e` (ancestry of the Phase 6 merge commit) shows a full feature
    branch of ~16 commits implementing the sidecar, webview host, CSP hardening, crash
    recovery, and a dedicated E2E harness (`0fc7184`, `bdf508b`, `e7d3c88`, `5f71a1d`,
    `099764b`, `d93c7b0`, `2026cdb`, `8904e8c`, `fcd54a9`, `ed67cf3`, `a13e64f`, `8caa165`,
    `41db519`, `59ba5c7`, among others).
  - Confirmed via `git ls-tree -r origin/main -- extension/ crates/`: `origin/main` contains
    `extension/src/{extension,sidecarBridge,webviewHtml}.ts` (+ tests), `crates/vscode-sidecar/`
    (JSON-RPC sidecar binary), and `crates/repo-service/` (the shared dispatch crate the design
    spec called for extracting out of `tauri-app`) — none of which exist in the locally
    checked-out working tree.
- **Impact:** Anyone relying on `CLAUDE.md` (including an AI coding agent operating under these
  exact instructions, as this audit's tooling does) to understand "project status" will
  materially misjudge the project's maturity, believe a major shipped capability is merely
  planned, and may unknowingly re-derive or duplicate design decisions already made and shipped.
  It also means the audit's own commissioning commit is on a stale/orphaned local branch state —
  a process risk independent of any code defect.
- **Trigger / reproduction:** Read `CLAUDE.md`'s "Project status" section on the local `main`
  branch, then run `git log --oneline main..origin/main`.
- **Remediation:** Fast-forward or rebase local `main` onto `origin/main` (after confirming
  the one local-only commit `cd0cb63` is preserved or superseded — see `08`'s note on the stray
  build artifacts this produced). Update `CLAUDE.md`'s "Project status" section to describe
  Phase 6 as shipped, and update or retire
  `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`'s status line (or fold its
  content into `docs/ARCHITECTURE.md`, which — even on `origin/main` — does not appear to
  mention `crates/repo-service`, `crates/vscode-sidecar`, or `extension/` at all; confirm and
  extend during the sync).
- **Verification:** After the branch sync, confirm `git log --oneline main..origin/main` is
  empty and that `CLAUDE.md` names Phase 6 as complete, matching the actual crate layout.

### AUD-2026-09-05-DOC-002 — `docs/ARCHITECTURE.md` documents a 3-crate layout that is already incomplete on `origin/main`

- **Severity:** Medium
- **Confidence:** Confirmed
- **Affected components:** `docs/ARCHITECTURE.md` (crate/package layout diagram, lines 5-12).
- **Evidence:** The architecture doc's crate diagram lists only `git-core`, `config`, and
  `tauri-app`, plus `frontend/`. `origin/main` additionally has `crates/repo-service` and
  `crates/vscode-sidecar` (see DOC-001's evidence), and a second frontend build target
  (`frontend/dist-vscode`, referenced by `extension/src/extension.ts:20`'s
  `resolveWebviewAssetRoot`). This diagram was already the documented architecture at the audit
  commit and does not reflect even the locally-absent-but-shipped reality.
- **Impact:** A contributor reading only `docs/ARCHITECTURE.md` (which `CLAUDE.md` names as the
  authoritative architecture reference) gets a wrong mental model of crate boundaries once the
  branch is synced. Lower severity than DOC-001 because this document's own content is
  internally consistent for the code that IS present locally — it is only incomplete, not
  actively contradicted, at the audit commit.
- **Trigger / reproduction:** Compare `docs/ARCHITECTURE.md:5-12` against
  `git ls-tree -r origin/main --name-only -- crates/ | cut -d/ -f1-2 | sort -u`.
- **Remediation:** Update the crate diagram and add a "Why `repo-service`" section (mirroring the
  existing "Why Tauri + a web frontend" / "Why git2" sections) once the branch is synced, so the
  extraction's rationale is preserved in the same document future contributors already check
  first.
- **Verification:** Diagram lists all five crates plus both frontend build outputs after sync.

### AUD-2026-09-05-DOC-003 — Failure-log location is undocumented for end users/support

- **Severity:** Low
- **Confidence:** Confirmed
- **Affected components:** `docs/USER_GUIDE.md`, the 0.2.0 failure-logging feature
  (`crates/tauri-app/src/main.rs:36-47`, `tauri_plugin_log` with `TargetKind::LogDir`).
- **Evidence:** `CHANGELOG.md`'s `[0.2.0]` "Added" section documents that failures are now
  written to "a rotated log file to the OS log directory," and `main.rs:39-41` confirms this
  (`tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name:
  Some("browsitory".into()) })`). `docs/USER_GUIDE.md` (read in full) has no section describing
  where this file lives per platform, how to attach it to a bug report, or its retention
  (`RotationStrategy::KeepSome(5)`, `main.rs:45`).
- **Impact:** A user hitting a crash or a silent failure has no in-product or documented way to
  find diagnostic output without already knowing Tauri's `LogDir` convention (which varies by
  OS: e.g. `~/.local/share/com.browsitory.browsitory/logs` on Linux,
  `~/Library/Logs/com.browsitory.Browsitory` on macOS, `%APPDATA%\com.browsitory.Browsitory\logs`
  on Windows, per `tauri-plugin-log`'s documented defaults). This weakens the value of the
  feature CHANGELOG.md specifically credits with letting "a bug report... not require a live dev
  session to diagnose."
- **Trigger / reproduction:** Search `docs/USER_GUIDE.md` for "log" — no hits describing the
  failure-log file path.
- **Remediation:** Add a short "Diagnosing a failure" section to `docs/USER_GUIDE.md` naming the
  per-OS log directory and the 5-file rotation, and cross-reference it from any in-app error
  banner if one exists.
- **Verification:** `docs/USER_GUIDE.md` contains a discoverable section naming the log path per
  platform.

## Coverage gaps and open questions

- Whether `README.md` or `docs/USER_GUIDE.md` (beyond the specific gap in DOC-003) describe
  end-user-facing setup instructions accurately was spot-checked but not exhaustively
  line-audited; a full pass was out of scope given this deep-dive's focus on the Phase 6
  status-tracking failure, which dominates the area's risk.
- `docs/LICENSE_COMPLIANCE.md` accuracy against actual dependencies is covered in `08`, not
  here.
- Whether `origin/main`'s copy of `docs/ARCHITECTURE.md` already fixes DOC-002 was not checked
  (would require `git show origin/main:docs/ARCHITECTURE.md`); if it already documents
  `repo-service`/`vscode-sidecar`, DOC-002 is purely a "sync the branch" issue with no additional
  doc-authoring work needed. **Smallest next action:** run
  `git show origin/main:docs/ARCHITECTURE.md | head -20` before doing any remediation work here.

## Strengths and positive controls

- `CHANGELOG.md` is well-maintained and specific for the work that IS on the local branch — each
  0.2.0 entry cites the concrete behavior change and, where relevant, the reason (e.g., the
  atomic `config.toml` write fix, the two CI-flake fixes). This is a strong control that makes
  the DOC-001 staleness more surprising by contrast — the process clearly works when a branch is
  kept in sync.
- The credential-release acceptance procedure in `docs/ARCHITECTURE.md` (the manual HTTPS/SSH
  test checklist) is unusually precise and testable for a documentation artifact — it names
  exact required user-facing strings and exact allowed `.git/config` contents, which made it
  possible to directly verify against the implementation in deep-dive 04 rather than treating it
  as an unverifiable claim.
