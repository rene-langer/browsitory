import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { expect } from "@wdio/globals";

const E2E_WORKSPACE_ROOT = path.join(os.tmpdir(), "browsitory-e2e-workspace-root");
const E2E_WORKSPACE_REPO_A = path.join(E2E_WORKSPACE_ROOT, "repo-a");
const E2E_WORKSPACE_REPO_B = path.join(E2E_WORKSPACE_ROOT, "repo-b");
const E2E_WORKSPACE_REPO_C = path.join(E2E_WORKSPACE_ROOT, "repo-c");
const E2E_CONFIG_FILE = path.join(os.tmpdir(), "browsitory-e2e-config", "config.toml");
const E2E_RESTORE_OPEN_REPOS_ONCE = "browsitory-e2e-restore-open-repos-once";

async function waitForAppReady(): Promise<void> {
  await $('section[aria-label="Branches"] button[aria-expanded]').waitForExist({ timeout: 10000 });
}

async function openPickerOverlay(): Promise<void> {
  await waitForAppReady();
  await browser.execute((el) => (el as HTMLElement).click(), await $('button[aria-label="Open another repository"]'));
}

describe("Browsitory multi-repo workspaces", function () {
  // Evidence from three real CI runs on 2026-08-24 (see the Edit-modal wait below): under
  // normal runner load the whole Edit-modal re-scan resolves in well under a second (confirmed
  // via a temporary diagnostic poll — the repo-c checkbox existed at t=0ms on the first check),
  // but under CI's demonstrated shared-runner contention the same operation has stalled long
  // enough to blow past a 10s and then a 30s wait without completing (the "restores..." test's
  // own total runtime swung 42.5s -> 72.7s across two of those runs with zero code changes in
  // between, so the slowdown is general runner contention, not anything specific to this scan).
  // A single retry plus this file's own generous per-test timeout is defense-in-depth against
  // that confirmed environmental variance — not a substitute for the wait budgets below, which
  // still need real margin over the worst case observed so far.
  this.retries(1);
  this.timeout(90000);

  it("restores an open workspace group after restart, then closes the whole group at once", async () => {
    await openPickerOverlay();

    const openAllButton = await $('button=Open All');
    await openAllButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), openAllButton);

    await browser.waitUntil(
      async () => (await (await $$('[role="tab"]')).length) >= 2,
      { timeout: 10000, timeoutMsg: "expected the workspace's two members to open as tabs" },
    );

    const groupLabel = await $('span*=E2E Workspace');
    await groupLabel.waitForExist({ timeout: 10000 });

    const repoATab = await $(`button[title="${E2E_WORKSPACE_REPO_A}"]`);
    const repoBTab = await $(`button[title="${E2E_WORKSPACE_REPO_B}"]`);
    await repoATab.waitForExist({ timeout: 10000 });
    await repoBTab.waitForExist({ timeout: 10000 });

    await browser.waitUntil(
      async () => {
        const config = fs.readFileSync(E2E_CONFIG_FILE, "utf8");
        return (
          config.includes(`[[open_repos]]\npath = "${E2E_WORKSPACE_REPO_A}"`) &&
          config.includes(`[[open_repos]]\npath = "${E2E_WORKSPACE_REPO_B}"`)
        );
      },
      { timeout: 10000, timeoutMsg: "expected workspace members to persist before restart" },
    );

    await browser.execute((key) => window.localStorage.setItem(key, "true"), E2E_RESTORE_OPEN_REPOS_ONCE);
    await browser.reloadSession();
    await waitForAppReady();

    await $('span*=E2E Workspace').waitForExist({ timeout: 10000 });
    await $(`button[title="${E2E_WORKSPACE_REPO_A}"]`).waitForExist({ timeout: 10000 });
    await $(`button[title="${E2E_WORKSPACE_REPO_B}"]`).waitForExist({ timeout: 10000 });

    const closeGroupButton = await $('button[aria-label="Close E2E Workspace"]');
    await browser.execute((el) => (el as HTMLElement).click(), closeGroupButton);

    await browser.waitUntil(
      async () => !(await $(`button[title="${E2E_WORKSPACE_REPO_A}"]`).isExisting()),
      { timeout: 10000, timeoutMsg: "expected both workspace member tabs to close" },
    );
    expect(await $(`button[title="${E2E_WORKSPACE_REPO_A}"]`).isExisting()).toBe(false);
    expect(await $(`button[title="${E2E_WORKSPACE_REPO_B}"]`).isExisting()).toBe(false);
    expect(await (await $$('[role="tab"]')).length).toBe(1);
  });

  it("Edit re-scans the root, pre-checking current members and offering the newly-found repo unchecked", async () => {
    await openPickerOverlay();

    // `useWorkspaces`'s initial `listWorkspaces()` fetch re-runs from scratch after the previous
    // test's `browser.reloadSession()` (a full app restart, not just a re-render) — under the
    // same CI-runner contention documented above for `scan_repos_in_root`, that round trip has
    // also been observed to blow past 10s. Match the 45s budget used for that other IPC round
    // trip rather than a shorter one that only held up under low contention.
    const editButton = await $('button=Edit');
    await editButton.waitForExist({ timeout: 45000 });
    await browser.execute((el) => (el as HTMLElement).click(), editButton);

    // This is the first `scan_repos_in_root` invocation anywhere in this spec's session. A
    // diagnostic poll on 2026-08-24 confirmed the scan+render pipeline itself is near-instant
    // under normal load (the checkbox existed at the very first check, t=0ms) — the two prior
    // failures here (10s, then 30s) were CI-runner contention, not a slow operation. 45s gives
    // real margin over both observed failures; `this.retries(1)`/`this.timeout(90000)` above are
    // the other half of this defense (see that comment for the full evidence).
    const repoCCheckbox = await $(`input[aria-label="${E2E_WORKSPACE_REPO_C}"]`);
    await repoCCheckbox.waitForExist({ timeout: 45000 });
    expect(await repoCCheckbox.isSelected()).toBe(false);

    const repoACheckbox = await $(`input[aria-label="${E2E_WORKSPACE_REPO_A}"]`);
    expect(await repoACheckbox.isSelected()).toBe(true);

    await browser.execute((el) => (el as HTMLElement).click(), repoCCheckbox);
    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Save'));

    // Saving waits for the config update and workspace refresh before returning to the picker.
    // Observe that return before reopening Edit so the second edit reads persisted membership.
    const openAllAfterSave = await $('button=Open All');
    await openAllAfterSave.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Edit'));
    const repoCCheckboxAfterSave = await $(`input[aria-label="${E2E_WORKSPACE_REPO_C}"]`);
    await repoCCheckboxAfterSave.waitForExist({ timeout: 45000 });
    expect(await repoCCheckboxAfterSave.isSelected()).toBe(true);
    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Cancel'));
  });

  it("Delete removes the workspace from the list after confirmation", async () => {
    await openPickerOverlay();

    // Unlike the Edit button above, this used to be looked up and clicked in one shot with no
    // wait, racing RepoPicker's async `listWorkspaces()` fetch (fired from `useWorkspaces`'s
    // `refresh()` on mount) — the picker overlay's own button appears immediately, but the
    // Workspaces panel's contents render only once that fetch resolves. Confirmed via the CI
    // run (32726464369) log: the picker-open click and the very next (unguarded) findElement
    // for this button were back-to-back with zero wait between them, and the button wasn't in
    // the DOM yet. `waitForExist` closes that race the same way every other click in this file
    // already does.
    // Same `listWorkspaces()`-after-reload contention as the Edit test above — 45s budget, not
    // just enough for the picker overlay itself to render.
    const deleteButton = await $('button=Delete E2E Workspace');
    await deleteButton.waitForExist({ timeout: 45000 });
    await browser.execute((el) => (el as HTMLElement).click(), deleteButton);
    const confirmDialog = await $('dialog[aria-label="Delete workspace E2E Workspace"]');
    await confirmDialog.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Delete workspace'));

    await browser.waitUntil(
      async () => !(await $('span*=E2E Workspace').isExisting()),
      { timeout: 10000, timeoutMsg: "expected the workspace to be removed from the list" },
    );
  });
});
