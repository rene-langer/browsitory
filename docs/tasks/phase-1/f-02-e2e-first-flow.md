# Task 1.F.02: E2E setup + first flow (`tauri-driver` + WebdriverIO)

## Goal

Stand up Browsitory's first E2E test, per `docs/ARCHITECTURE.md`'s testing-strategy section:
one flow that spans backend+frontend in a way unit tests can't catch — open a repo, see its
status, stage a file, commit, see the new commit in history — driven against the real
`cargo tauri` build via `tauri-driver` + WebdriverIO (not Playwright; see the correction in
`docs/ARCHITECTURE.md` and `docs/superpowers/specs/2026-08-12-browsitory-phase1-design.md` —
Playwright can't attach to a native Tauri window, `tauri-driver` is Tauri's own WebDriver
bridge).

**Read this whole task before starting** — more of it than usual is "verify against the live
Tauri WebDriver guide" rather than copy-paste-ready, flagged explicitly below. `tauri-driver`'s
exact WebdriverIO capability shape isn't cross-checked against a locally vendored source the way
the rest of this plan's Rust code was (there's no cached crate source to check the way `git2`'s
was checked for Tasks 1.A.01-03) — verify the specifics against
`https://v2.tauri.app/develop/tests/webdriver/` (or whatever the current official WebDriver
testing guide URL is) before writing the config, and note in the task report if anything below
didn't match current reality.

## Depends on

1.F.01 (the full app must be wired up and working before there's anything to E2E test).

## Interfaces produced

A new `e2e/` directory at the repo root (per the original architecture spec's file-structure
section, which already reserved this path — it previously said "Playwright specs", now
WebdriverIO specs instead) containing the E2E test suite. Nothing under `crates/`/`frontend/`
consumes this — it's the terminal task of the phase's testing pyramid, consuming the *built app*
as a black box.

## Implementation notes

**System prerequisites (document, don't try to script an install for the implementer — note
these in the task report as manual/CI setup steps):**
- `cargo install tauri-driver --locked` (a standalone binary, not a `Cargo.toml` dependency —
  same category as `cargo install tauri-cli` from Task 1.C.01's Cargo.toml comment).
- On Linux: the `webkit2gtk-driver` system package (separate from the `libwebkit2gtk-4.1-dev`
  package Phase 0's CI already installs — this one provides `WebKitWebDriver`, the actual
  WebDriver server `tauri-driver` wraps) and `xvfb` for headless CI runs (no physical display).

**`e2e/package.json`** (new, separate `pnpm` workspace member from `frontend/` — it drives the
built app, not the frontend source, so it doesn't need Vite/React/etc.):
```json
{
  "name": "e2e",
  "private": true,
  "type": "module",
  "scripts": {
    "test": "wdio run wdio.conf.ts"
  },
  "devDependencies": {
    "@wdio/cli": "^9",
    "@wdio/local-runner": "^9",
    "@wdio/mocha-framework": "^9",
    "typescript": "~6.0.2"
  }
}
```
(Verify these WebdriverIO package versions/names against what's actually current when this task
is implemented — `^9` is this plan's best estimate, not a pinned-and-verified number the way
`git2 = "0.21"` was for Task 1.A.01.)

**`e2e/wdio.conf.ts`** (new) — the part most likely to need adjustment against the live guide:
```ts
import path from "node:path";
import type { Options } from "@wdio/types";

// Path to the debug build produced by `cargo build --workspace` from the repo root.
const tauriAppBinary = path.resolve(
  import.meta.dirname,
  "../target/debug/tauri-app",
);

export const config: Options.Testrunner = {
  runner: "local",
  hostname: "127.0.0.1",
  port: 4444,
  specs: ["./specs/**/*.spec.ts"],
  capabilities: [
    {
      // @ts-expect-error — tauri:options isn't in WebdriverIO's built-in capability types
      "tauri:options": {
        application: tauriAppBinary,
      },
      browserName: "wry",
    },
  ],
  framework: "mocha",
  mochaOpts: {
    timeout: 60000,
  },
};
```
`tauri-driver` itself must be running (listening on `127.0.0.1:4444`) before `wdio run` starts —
either start it manually in a separate terminal (`tauri-driver`) for local development, or have
CI start it as a background step before running `pnpm test` in `e2e/`. Don't try to have
WebdriverIO spawn `tauri-driver` itself unless the live guide shows a documented way to do that
cleanly — a separate CI step is simpler and easier to debug when it fails.

**`e2e/specs/first-flow.spec.ts`** (new):
```ts
import { expect } from "@wdio/globals";

describe("Browsitory first flow", () => {
  it("opens a repo, stages a file, commits, and sees it in history", async () => {
    // This test needs a real git repo with an uncommitted change to open. The simplest
    // approach: have a `before` hook shell out to create one in a temp directory (Node's
    // `child_process.execSync` running `git init`/`git commit`/writing a file), then drive
    // the picker to that path — there is no `pickRepoFolder` automation shortcut through
    // WebDriver for a native OS dialog, so this flow can't click "Open Folder" and use the
    // real dialog. Instead, this spec should call `openRepo` directly via whatever debug/test
    // entry point is simplest to add — the cleanest option is a `?repoPath=` query param or an
    // `E2E_REPO_PATH` environment variable that `App.tsx` checks on mount to auto-open a repo,
    // bypassing `RepoPicker` for this one test. That one small addition to `App.tsx` (gated so
    // it only does anything when the env var is set, so it changes no normal-run behavior) is
    // in scope for this task, since without it the picker's native dialog is unautomatable.

    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });

    // Stage the one uncommitted file the `before` hook created.
    const stageButton = await $("button=Stage");
    await stageButton.click();

    await commitMessageInput.setValue("e2e: first commit");
    const commitButton = await $("button=Commit");
    await commitButton.click();

    // The new commit should now appear in the history list.
    const historyEntry = await $("li*=e2e: first commit");
    await historyEntry.waitForExist({ timeout: 10000 });
    await expect(historyEntry).toBeExisting();
  });
});
```
The `E2E_REPO_PATH` auto-open addition to `App.tsx`:
```tsx
useEffect(() => {
  const autoOpenPath = import.meta.env.VITE_E2E_REPO_PATH;
  if (typeof autoOpenPath === "string" && autoOpenPath.length > 0) {
    appState.openRepo(autoOpenPath);
  }
  // eslint-disable-next-line react-hooks/exhaustive-deps
}, []);
```
(Placed in `App.tsx`, reads a Vite env var so it's statically absent from a normal production
build unless `VITE_E2E_REPO_PATH` is actually set at build/dev time — the E2E test run sets it
when launching the app under test.)

## TDD requirement

This task's "test" is the E2E spec itself — there's no separate unit test for E2E scaffolding.
The spec must fail first for the expected reason (no app running / `tauri-driver` not started /
`App.tsx` not yet reading `VITE_E2E_REPO_PATH`) before the scaffolding is complete, then pass
once `tauri-driver` is running against a real built binary and the `App.tsx` addition lands.

## Acceptance criteria

- [ ] `cargo build --workspace` produces `target/debug/tauri-app` (the binary `wdio.conf.ts`
      points at).
- [ ] With `tauri-driver` running separately, `cd e2e && VITE_E2E_REPO_PATH=<a temp repo path
      created by the test setup> pnpm test` passes the one spec.
- [ ] `.github/workflows/ci.yml` gains an `e2e` job (Linux only): installs
      `webkit2gtk-driver`, `xvfb`; `cargo install tauri-driver --locked`; `cargo build
      --workspace`; runs `tauri-driver` in the background (or via `xvfb-run`); runs
      `cd e2e && pnpm install && pnpm test`.
- [ ] `docs/ARCHITECTURE.md`'s testing-strategy section is already corrected (done earlier this
      phase, not part of this task's diff) — confirm it still reads accurately once this task's
      actual implementation is done, and fix it if reality diverged from the plan above.
- [ ] Commit: `git add e2e .github/workflows/ci.yml frontend/src/App.tsx && git commit -m "test: add tauri-driver + WebdriverIO E2E setup and first flow"`.

## Out of scope

E2E coverage of any flow beyond the one described (diff viewing, unstage, error states) — one
flow is this phase's explicit target per the design spec, more flows arrive with later phases'
features. Cross-platform E2E (Windows/macOS `tauri-driver` support is less mature than Linux's
WebKitWebDriver path as of this writing) — Linux-only for this phase's CI job.
