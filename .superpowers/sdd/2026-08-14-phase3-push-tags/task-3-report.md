# Task 3 report — tag and push UI

Baseline: `01caa28`

## Delivered

- Added `TagPanel` for lightweight and annotated tag creation, including required annotated-tag messages.
- Added explicit local-tag delete confirmation, selected-tag checkboxes, and distinct selected/all tag push actions.
- Added per-remote current-branch Push controls and disabled all push/tag mutations during pending, transfer, merge, or rebase operations.
- Wired the UI only through `useAppState`/`RepoClient`; no component imports a transport implementation.
- Updated transfer progress copy for fetch and push, added the remote-transfer E2E scenario, and brought existing test `RepoClient` fakes up to Task 2's expanded required contract.

## TDD evidence

- `TagPanel.test.tsx` first failed because `TagPanel` did not exist, then passed after its minimal implementation.
- The added RemotePanel Push test first failed because no Push control was rendered, then passed after wiring the callback.
- The transfer-progress accessibility test first failed against the Fetch-specific label, then passed after making it generic.

## Verification

- `corepack pnpm test -- --run` — 17 files, 178 tests passed.
- `corepack pnpm lint` — passed.
- `corepack pnpm build` — passed.
- `VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo corepack pnpm build` — passed.
- `cargo build --workspace --features tauri-app/custom-protocol` — passed.
- `git diff --check` — passed.
- Focused `remote-transfer.spec.ts` E2E could not run: a pre-existing Tauri/WebDriver process held port 4444, so the runner's spawned `tauri-driver` exited before the spec body began. The full E2E attempt had the same port conflict. This is environment infrastructure, not a test assertion failure.
- `corepack pnpm typecheck` in `e2e/` remains blocked by pre-existing WebdriverIO typing errors in `e2e/specs/merge.spec.ts` (unrelated to this task).
