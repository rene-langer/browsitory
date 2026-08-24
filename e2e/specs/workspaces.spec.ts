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

describe("Browsitory multi-repo workspaces", () => {
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

    const editButton = await $('button=Edit');
    await editButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), editButton);

    // TEMPORARY DIAGNOSTIC (2026-08-24): this wait has now timed out twice on real CI (10s,
    // then 30s after converting `scan_repos_in_root` to an async spawn_blocking command) despite
    // the scan itself being a trivial non-recursive `read_dir` over 3 tiny directories — too
    // cheap to plausibly cost multiple seconds, let alone 30. Rather than guess a third timeout
    // value blindly, poll and log the DOM's actual intermediate state once a second so the next
    // CI run's log tells us exactly which stage is stuck: does the root path `<p>` (confirms
    // `WorkspaceEditor` mounted with `root` already set), any checkbox at all (confirms
    // `scanReposInRoot` resolved with *something*), or an alert `<p role="alert">` (confirms it
    // rejected) ever appear, and when.
    const editDiagnosticDeadline = Date.now() + 45000;
    let editDiagnosticFound = false;
    while (Date.now() < editDiagnosticDeadline) {
      const elapsedMs = 45000 - (editDiagnosticDeadline - Date.now());
      const rootParagraphCount = (await $$('p[title]')).length;
      const checkboxCount = (await $$('input[type="checkbox"]')).length;
      const alertText = (await $('p[role="alert"]').isExisting())
        ? await $('p[role="alert"]').getText()
        : null;
      const repoCExists = await $(`input[aria-label="${E2E_WORKSPACE_REPO_C}"]`).isExisting();
      // eslint-disable-next-line no-console
      console.log(
        `[diagnostic t=${elapsedMs}ms] rootParagraphs=${rootParagraphCount} checkboxes=${checkboxCount} repoCExists=${repoCExists} alert=${JSON.stringify(alertText)}`,
      );
      if (repoCExists) {
        editDiagnosticFound = true;
        break;
      }
      await browser.pause(1000);
    }
    if (!editDiagnosticFound) {
      throw new Error("diagnostic: repo-c checkbox never appeared within 45s — see [diagnostic t=...] log lines above for where it stalled");
    }

    const repoCCheckbox = await $(`input[aria-label="${E2E_WORKSPACE_REPO_C}"]`);
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
    await repoCCheckboxAfterSave.waitForExist({ timeout: 30000 });
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
    const deleteButton = await $('button=Delete E2E Workspace');
    await deleteButton.waitForExist({ timeout: 10000 });
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
