# Phase 6 Layout Rollout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recompose `App.tsx` around the Foundation plan's new primitives —
a resizable sidebar of accordion sections replacing the full-width panel
stack, and an overlay for transient operations — so commit history and
diff get the full viewport by default.

**Architecture:** Seven components each swap their own `Panel` wrapper for
`AccordionSection` (no other change). `RebasePlanner` and `TransferPanel`
move into `Overlay`. `App.tsx` becomes two nested `SplitView`s:
`Sidebar` | `CommitHistory` | `Diff`, with `Overlay` conditionally
rendered on top.

**Tech Stack:** React 19, TypeScript, Vite CSS Modules — no new
dependencies.

**Spec:** `docs/superpowers/specs/2026-08-19-browsitory-phase6-layout-design.md`

**Depends on:** `docs/superpowers/plans/2026-08-19-phase6-foundation.md`
must be fully merged first — every task below imports `Sidebar`,
`AccordionSection`, or `Overlay` from
`frontend/src/components/primitives/`, all three produced there.

## Global Constraints

(Same as the Foundation plan — repeated here since this plan may execute
in a separate session.)

- No `RepoClient` method, DTO, Tauri command, worker message, or
  `git-core` function is added, removed, or changed in shape by this
  plan.
- Frontend tests mock `RepoClient`, never `@tauri-apps/api`.
- `pnpm lint`'s `no-restricted-imports` rule
  (`frontend/eslint.config.js:25-37`) must keep passing.
- Any new dependency must be permissively licensed and recorded in
  `docs/LICENSE_COMPLIANCE.md` in the same commit that adds it (none is
  expected — this plan only consumes what the Foundation plan already
  added).
- `pnpm build`, `pnpm lint`, and `pnpm test -- --run` must pass after
  every task.
- No behavior change to any of the seven migrated components beyond
  removing their own `Panel` wrapper — same props, same state, same
  handlers.
- No behavior change to `RebasePlanner` or `TransferPanel` at all —
  `Overlay` only repositions them.
- `RebaseProgressPanel` and `ConflictResolutionPane` are untouched by
  this plan — they render inside `DiffPane`, not the app shell.
- Every touched component is checked in both light and dark theme.

---

### Task 1: Migrate `BranchSwitcher` to `AccordionSection`

**Files:**
- Modify: `frontend/src/components/BranchSwitcher.tsx`

**Interfaces:**
- Consumes: `AccordionSection` from
  `frontend/src/components/primitives/AccordionSection.tsx` (produced by
  the Foundation plan).

- [ ] **Step 1: Baseline the existing test**

Run: `cd frontend && pnpm test -- --run BranchSwitcher.test.tsx`
Expected: PASS. This test does not assert on `Panel`'s region role (only
on `BranchSwitcher`'s own controls by role/text), so no test change is
expected from this task.

- [ ] **Step 2: Swap the import**

In `frontend/src/components/BranchSwitcher.tsx`, replace:

```typescript
import { Panel } from "./primitives/Panel";
```

with:

```typescript
import { AccordionSection } from "./primitives/AccordionSection";
```

- [ ] **Step 3: Swap the wrapper**

Replace the opening tag:

```typescript
    <Panel title="Branches">
```

with:

```typescript
    <AccordionSection title="Branches" storageKey="sidebar-branches">
```

Replace the matching closing tag `</Panel>` with `</AccordionSection>`.

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run BranchSwitcher.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/BranchSwitcher.tsx
git commit -m "feat(frontend): move BranchSwitcher into an AccordionSection"
```

---

### Task 2: Migrate `RemotePanel` to `AccordionSection`

**Files:**
- Modify: `frontend/src/components/RemotePanel.tsx`

**Interfaces:**
- Consumes: `AccordionSection`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run RemotePanel.test.tsx`
Expected: PASS.

- [ ] **Step 2: Swap the import**

Replace:

```typescript
import { Panel } from "./primitives/Panel";
```

with:

```typescript
import { AccordionSection } from "./primitives/AccordionSection";
```

- [ ] **Step 3: Swap the wrapper**

Replace:

```typescript
    <Panel title="Remotes">
```

with:

```typescript
    <AccordionSection title="Remotes" storageKey="sidebar-remotes">
```

Replace the matching `</Panel>` with `</AccordionSection>`.

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run RemotePanel.test.tsx && pnpm build`
Expected: PASS. Re-confirm (per this component's credential-handling
history) that no token value renders into any DOM attribute — this task
doesn't touch that logic, but it's the one check worth repeating whenever
this file is touched.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/RemotePanel.tsx
git commit -m "feat(frontend): move RemotePanel into an AccordionSection"
```

---

### Task 3: Migrate `TagPanel` to `AccordionSection`

**Files:**
- Modify: `frontend/src/components/TagPanel.tsx`

**Interfaces:**
- Consumes: `AccordionSection`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run TagPanel.test.tsx`
Expected: PASS.

- [ ] **Step 2: Swap the import**

Replace:

```typescript
import { Panel } from "./primitives/Panel";
```

with:

```typescript
import { AccordionSection } from "./primitives/AccordionSection";
```

- [ ] **Step 3: Swap the wrapper**

Replace:

```typescript
    <Panel title="Tags">
```

with:

```typescript
    <AccordionSection title="Tags" storageKey="sidebar-tags">
```

Replace the matching `</Panel>` with `</AccordionSection>`.

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run TagPanel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/TagPanel.tsx
git commit -m "feat(frontend): move TagPanel into an AccordionSection"
```

---

### Task 4: Migrate `WorktreePanel` to `AccordionSection`

**Files:**
- Modify: `frontend/src/components/WorktreePanel.tsx`

**Interfaces:**
- Consumes: `AccordionSection`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run WorktreePanel.test.tsx`
Expected: PASS.

- [ ] **Step 2: Swap the import**

Replace:

```typescript
import { Panel } from "./primitives/Panel";
```

with:

```typescript
import { AccordionSection } from "./primitives/AccordionSection";
```

- [ ] **Step 3: Swap the wrapper**

Replace:

```typescript
    <Panel title="Worktrees">
```

with:

```typescript
    <AccordionSection title="Worktrees" storageKey="sidebar-worktrees">
```

Replace the matching `</Panel>` with `</AccordionSection>`.

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run WorktreePanel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/WorktreePanel.tsx
git commit -m "feat(frontend): move WorktreePanel into an AccordionSection"
```

---

### Task 5: Migrate `SubmodulePanel` to `AccordionSection`

**Files:**
- Modify: `frontend/src/components/SubmodulePanel.tsx`

**Interfaces:**
- Consumes: `AccordionSection`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run SubmodulePanel.test.tsx`
Expected: PASS.

- [ ] **Step 2: Swap the import**

Replace:

```typescript
import { Panel } from "./primitives/Panel";
```

with:

```typescript
import { AccordionSection } from "./primitives/AccordionSection";
```

- [ ] **Step 3: Swap the wrapper**

Replace:

```typescript
    <Panel title="Submodules">
```

with:

```typescript
    <AccordionSection title="Submodules" storageKey="sidebar-submodules">
```

Replace the matching `</Panel>` with `</AccordionSection>`.

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run SubmodulePanel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/SubmodulePanel.tsx
git commit -m "feat(frontend): move SubmodulePanel into an AccordionSection"
```

---

### Task 6: Migrate `ReflogPanel` to `AccordionSection`

**Files:**
- Modify: `frontend/src/components/ReflogPanel.tsx`

**Interfaces:**
- Consumes: `AccordionSection`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run ReflogPanel.test.tsx`
Expected: PASS.

- [ ] **Step 2: Swap the import**

Replace:

```typescript
import { Panel } from "./primitives/Panel";
```

with:

```typescript
import { AccordionSection } from "./primitives/AccordionSection";
```

- [ ] **Step 3: Swap the wrapper**

Replace:

```typescript
    <Panel title="Reflog">
```

with:

```typescript
    <AccordionSection title="Reflog" storageKey="sidebar-reflog">
```

Replace the matching `</Panel>` with `</AccordionSection>`.

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run ReflogPanel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/ReflogPanel.tsx
git commit -m "feat(frontend): move ReflogPanel into an AccordionSection"
```

---

### Task 7: Migrate `PullRequestPanel`'s outer wrapper to `AccordionSection`

**Files:**
- Modify: `frontend/src/components/PullRequestPanel.tsx`

**Interfaces:**
- Consumes: `AccordionSection`.

Only the OUTER "Pull Requests" wrapper migrates. The inner per-repository
`<Panel title={sectionLabel} ariaLabel={sectionLabel}>` (around line 143,
rendered by `ForgeRepositorySection`) stays exactly as-is — it's a nested
card inside the accordion body, one per detected forge repository, not
the section's own chrome. Do not touch it.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run PullRequestPanel.test.tsx`
Expected: PASS. The existing assertions target the inner per-repository
`Panel` (e.g. `getByRole("region", { name: /github: acme\/widget
\(origin\)/i })`), not the outer "Pull Requests" wrapper — since the
inner `Panel` is untouched, no test change is expected from this task.

- [ ] **Step 2: Add the `AccordionSection` import alongside `Panel`**

`Panel` is still needed for the inner per-repository cards, so this is an
addition, not a replacement. Change:

```typescript
import { Panel } from "./primitives/Panel";
```

to:

```typescript
import { AccordionSection } from "./primitives/AccordionSection";
import { Panel } from "./primitives/Panel";
```

- [ ] **Step 3: Swap only the two outer wrapper occurrences**

Replace the empty-state branch:

```typescript
  if (forgeRepositories.length === 0) {
    return (
      <Panel title="Pull Requests">
        <p>No supported GitHub or Bitbucket remotes detected.</p>
      </Panel>
    );
  }
```

with:

```typescript
  if (forgeRepositories.length === 0) {
    return (
      <AccordionSection title="Pull Requests" storageKey="sidebar-pull-requests">
        <p>No supported GitHub or Bitbucket remotes detected.</p>
      </AccordionSection>
    );
  }
```

Replace the populated branch:

```typescript
  // Nested Panels are intentional: the outer one is this stack entry's card ("Pull Requests"),
  // each inner one is a per-forge-repository card titled with its own provider/owner/remote.
  return (
    <Panel title="Pull Requests">
      <div className={styles.sections}>
```

with:

```typescript
  // The inner Panel per forge repository is intentional: each one is a card titled with its
  // own provider/owner/remote, nested inside this section's AccordionSection body.
  return (
    <AccordionSection title="Pull Requests" storageKey="sidebar-pull-requests">
      <div className={styles.sections}>
```

and its matching closing tags:

```typescript
      </div>
    </Panel>
  );
}
```

with:

```typescript
      </div>
    </AccordionSection>
  );
}
```

Leave `ForgeRepositorySection`'s own `<Panel title={sectionLabel}
ariaLabel={sectionLabel}>...</Panel>` (inside the `.map`) completely
unchanged.

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run PullRequestPanel.test.tsx && pnpm build`
Expected: PASS. Re-confirm no token value renders into the DOM (this
task doesn't touch the token-save form, but it's the standing check for
this file).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/PullRequestPanel.tsx
git commit -m "feat(frontend): move PullRequestPanel's outer wrapper into an AccordionSection"
```

---

### Task 8: Move `RebasePlanner` and `TransferPanel` into `Overlay`

**Files:**
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `Overlay` from
  `frontend/src/components/primitives/Overlay.tsx` (produced by the
  Foundation plan).

This task only changes where `RebasePlanner` and `TransferPanel` render —
it does not touch the `Sidebar`/`SplitView` composition, which is Task 9.
Read the current `frontend/src/App.tsx` in full before editing (it has
shifted from the version excerpted below if earlier tasks in this plan
already ran against a stale copy — always re-read before editing).

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run`
Expected: PASS (full suite, confirms the starting point before this
task's App.tsx edits).

- [ ] **Step 2: Add the `Overlay` import**

Add, alongside the other primitive imports in `frontend/src/App.tsx`:

```typescript
import { Overlay } from "./components/primitives/Overlay";
```

- [ ] **Step 3: Wrap `TransferPanel` in `Overlay`, keyed on transfer state**

`TransferPanel` currently renders as `<TransferPanel
progress={appState.state.transfer} />` inside the panel stack — but
`TransferPanel` itself already returns `null` when `progress === null`
(check `frontend/src/components/TransferPanel.tsx` to confirm this before
editing — do not duplicate that null-check in `App.tsx`). Replace:

```typescript
        <TransferPanel progress={appState.state.transfer} />
```

with, placed as a sibling of `<SplitView>` rather than inside the panel
stack (Task 9 removes the panel stack entirely, so this is also where it
ends up permanently — placing it correctly now avoids Task 9 having to
move it again):

```typescript
      {appState.state.transfer !== null && (
        <Overlay>
          <TransferPanel progress={appState.state.transfer} />
        </Overlay>
      )}
```

- [ ] **Step 4: Wrap `RebasePlanner` in `Overlay`**

Replace:

```typescript
      {appState.state.rebaseOnto !== null && (
        <RebasePlanner
          client={tauriRepoClient}
          onto={appState.state.rebaseOnto}
          onStartRebase={appState.startRebase}
          onCancel={appState.closeRebasePlanner}
          operationDisabled={repositoryOperationDisabled}
        />
      )}
```

with:

```typescript
      {appState.state.rebaseOnto !== null && (
        <Overlay onClose={appState.closeRebasePlanner}>
          <RebasePlanner
            client={tauriRepoClient}
            onto={appState.state.rebaseOnto}
            onStartRebase={appState.startRebase}
            onCancel={appState.closeRebasePlanner}
            operationDisabled={repositoryOperationDisabled}
          />
        </Overlay>
      )}
```

`onClose` is wired to the same `appState.closeRebasePlanner` the
component's own Cancel button already calls, so pressing Escape (which
`Overlay`'s underlying `<dialog>` handles natively when opened via
`showModal()`) updates app state consistently instead of leaving the
dialog visually closed while `appState.state.rebaseOnto` still thinks
it's open. `TransferPanel` has no user-cancel affordance today, so its
`Overlay` is left without an `onClose` — Escape will visually close an
empty dialog shell without an app-state update, which is an acceptable,
narrow edge case matching `TransferPanel`'s existing lack of a cancel
path (not introduced by this task).

- [ ] **Step 5: Verify no regression**

Run: `cd frontend && pnpm test -- --run && pnpm build`
Expected: PASS. If any App-level test exercises the rebase-planner or
transfer-progress flow and asserts on DOM structure around
`RebasePlanner`/`TransferPanel` rather than by role/text, it may need a
small selector update to account for the new wrapping `<dialog>` — check
`frontend/src/App.test.tsx` if it exists, or any test file that renders
`App` and drives these flows, and fix forward rather than weaken any
assertion.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/App.tsx
git commit -m "feat(frontend): move RebasePlanner and TransferPanel into Overlay"
```

---

### Task 9: Recompose `App.tsx` around `Sidebar` and nested `SplitView`

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/App.module.css`

**Interfaces:**
- Consumes: `Sidebar`, `AccordionSection` (already imported by the seven
  migrated components, but `App.tsx` itself does not need to import
  `AccordionSection` — it only imports `Sidebar` and passes the already-
  migrated components as `Sidebar`'s children) from
  `frontend/src/components/primitives/`.

Read the current `frontend/src/App.tsx` in full before editing (Task 8
already changed it once in this plan). This task removes the
`.panelStack` wrapper entirely and replaces it with `Sidebar` plus a
second, nested `SplitView`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run`
Expected: PASS.

- [ ] **Step 2: Add the `Sidebar` import**

Add, alongside the other primitive imports:

```typescript
import { Sidebar } from "./components/primitives/Sidebar";
```

- [ ] **Step 3: Replace the panel stack and outer `SplitView` with the new layout**

Find the post-open branch's `<div className={styles.panelStack}>...
</div>` block followed by the single `<SplitView left={<CommitGraph
.../>} right={<DiffPane .../>} />` call. Replace both together with:

```typescript
      <SplitView
        storageKey="sidebar-width"
        defaultWidth={260}
        minWidth={200}
        maxWidth={420}
        collapsible
        left={
          <Sidebar>
            <BranchSwitcher
              branches={appState.state.branches}
              createBranchDraft={appState.state.createBranchDraft}
              onSwitchBranch={appState.switchBranch}
              onCreateBranch={appState.createBranch}
              onDeleteBranch={appState.deleteBranch}
              onRenameBranch={appState.renameBranch}
              onOpenCreateBranchDraft={appState.openCreateBranchDraft}
              onCloseCreateBranchDraft={appState.closeCreateBranchDraft}
              onMergeBranch={appState.mergeBranch}
              isMerging={appState.state.mergeMessage !== null}
              isRebasing={appState.state.rebaseProgress !== null}
              operationDisabled={repositoryOperationDisabled}
            />
            <WorktreePanel
              worktrees={appState.state.worktrees}
              branches={appState.state.branches}
              onOpenWorktree={appState.openRepo}
              onCreateWorktree={appState.createWorktree}
              onRemoveWorktree={appState.removeWorktree}
              onPruneWorktrees={appState.pruneWorktrees}
              operationDisabled={repositoryOperationDisabled}
            />
            <SubmodulePanel
              submodules={appState.state.submodules}
              onInit={appState.initSubmodule}
              onUpdate={appState.updateSubmodule}
              operationDisabled={repositoryOperationDisabled}
            />
            <ReflogPanel
              references={appState.state.reflogRefs}
              selectedReference={appState.state.selectedReflogReference}
              entries={appState.state.reflog}
              onSelectReference={appState.selectReflogReference}
              onRestore={appState.restoreReflogEntry}
              operationDisabled={repositoryOperationDisabled}
            />
            <RemotePanel
              remotes={appState.state.remotes}
              upstream={appState.state.upstream}
              remoteUpstreams={appState.state.remoteUpstreams}
              onAddRemote={appState.addRemote}
              onRenameRemote={appState.renameRemote}
              onUpdateRemoteUrls={appState.updateRemoteUrls}
              onRemoveRemote={appState.removeRemote}
              onSaveHttpsCredential={appState.saveHttpsCredential}
              onForgetHttpsCredential={appState.forgetHttpsCredential}
              onSetRemoteAuthMode={appState.setRemoteAuthMode}
              onSetUpstream={appState.setCurrentUpstream}
              onClearUpstream={appState.clearCurrentUpstream}
              onFetchRemote={appState.fetchRemote}
              fetchDisabled={repositoryOperationDisabled}
              onPushCurrentBranch={appState.pushCurrentBranch}
              pushDisabled={repositoryOperationDisabled}
              onPull={appState.pullCurrentUpstream}
              pullDisabled={repositoryOperationDisabled}
              pendingPull={appState.state.pendingPull}
              pullOutcome={appState.state.pullOutcome}
              onMergePull={async (upstreamRef) => {
                appState.clearPendingPull();
                await appState.mergeBranch(upstreamRef);
              }}
              onRebasePull={(upstreamRef) => {
                appState.clearPendingPull();
                appState.openRebasePlanner(upstreamRef);
              }}
              onCancelPull={appState.clearPendingPull}
            />
            <TagPanel
              tags={appState.state.tags}
              remotes={appState.state.remotes}
              onCreate={appState.createTag}
              onDelete={appState.deleteTag}
              onPush={appState.pushTags}
              pushDisabled={repositoryOperationDisabled}
            />
            <PullRequestPanel
              forgeRepositories={appState.state.forgeRepositories}
              pullRequests={appState.state.pullRequests}
              onListPullRequests={appState.listPullRequests}
              onSaveForgeToken={appState.saveForgeToken}
              onForgetForgeToken={appState.forgetForgeToken}
              onCreatePullRequest={appState.createPullRequest}
              onOpenExternalUrl={appState.openExternalUrl}
              operationDisabled={repositoryOperationDisabled}
            />
          </Sidebar>
        }
        right={
          <SplitView
            storageKey="history-diff-width"
            defaultWidth={420}
            minWidth={280}
            maxWidth={800}
            left={
              <CommitGraph
                status={appState.state.status}
                commits={appState.state.commits}
                stashes={appState.state.stashes}
                selectedRow={appState.state.selectedRow}
                pending={repositoryOperationDisabled}
                onSelectRow={appState.selectRow}
                onBranchFromCommit={appState.openCreateBranchDraft}
                onRebaseFromCommit={appState.openRebasePlanner}
                onApplyStash={appState.applyStash}
                onDropStash={appState.dropStash}
              />
            }
            right={
              <DiffPane
                client={tauriRepoClient}
                selectedRow={appState.state.selectedRow}
                status={appState.state.status}
                onStageFile={appState.stageFile}
                onUnstageFile={appState.unstageFile}
                onCommit={appState.commit}
                onSaveStash={appState.saveStash}
                onSelectRow={appState.selectRow}
                onResolveConflict={appState.resolveConflict}
                onResolveAddDeleteConflict={appState.resolveAddDeleteConflict}
                mergeMessage={appState.state.mergeMessage}
                onAbortMerge={appState.abortMerge}
                rebaseProgress={appState.state.rebaseProgress}
                onRebaseContinue={appState.rebaseContinue}
                onRebaseAbort={appState.abortRebase}
              />
            }
          />
        }
      />
```

Note the outer `SplitView`'s `left` pane (`Sidebar`) is what
`storageKey`/`defaultWidth`/`minWidth`/`maxWidth`/`collapsible` apply to
— it governs the sidebar's width, not the inner history/diff split,
which gets its own independent `storageKey="history-diff-width"` on the
nested `SplitView`. Every prop value passed to each child component is
copied verbatim from the removed panel-stack/outer-`SplitView` block —
this task moves them, it does not change any of them.

- [ ] **Step 4: Remove the now-unused `.panelStack` rule**

In `frontend/src/App.module.css`, delete the `.panelStack` rule block —
`App.tsx` no longer references `styles.panelStack` after Step 3.

- [ ] **Step 5: Verify no regression**

Run: `cd frontend && pnpm build && pnpm lint && pnpm test -- --run`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/App.tsx frontend/src/App.module.css
git commit -m "feat(frontend): recompose App.tsx around Sidebar and nested SplitView"
```

---

### Task 10: Full verification and theme check

**Files:** none (verification only).

- [ ] **Step 1: Full test suite, build, lint**

Run: `cd frontend && pnpm build && pnpm lint && pnpm test -- --run`
Expected: all PASS.

- [ ] **Step 2: Manual theme check**

Open the app (`cargo tauri dev` from `crates/tauri-app`, or `pnpm dev` in
`frontend/` against a mock), toggle the theme button, and visually
confirm: the sidebar's accordion sections (collapsed by default —
expand each once to check), the resize divider (both the sidebar's and
the history/diff split's), and the `Overlay` (trigger a rebase plan or a
push/pull to see `TransferPanel`) all render legibly in both themes.
This closes the gap the Phase 5 rollout plan left open (its own manual
theme check never got past the pre-open screen in a sandboxed
environment) — if this environment has the same limitation, say so
explicitly in the task report rather than silently skipping it.

- [ ] **Step 3: Run the GUI E2E suite**

Run (from the repo root, per `CLAUDE.md`'s E2E block):
```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm test
```
Expected: all specs pass. This phase changes the app's DOM structure
broadly (accordion sections default to closed, so any E2E spec that
locates a sidebar panel's controls without first expanding its section
will fail to find them) — expect to update several specs' setup steps to
click the relevant `AccordionSection`'s header open before interacting
with what's inside it. Update specs to match the new structure; do not
weaken any assertion to make a spec pass.

- [ ] **Step 4: Commit (only if Steps 2-3 required E2E spec fixes)**

```bash
git add e2e/specs/
git commit -m "fix(e2e): expand sidebar sections before interacting with their contents"
```

If no E2E spec needed changes, this task has nothing to commit.
