import path from "node:path";
import os from "node:os";
import { expect } from "@wdio/globals";

const E2E_WORKSPACE_ROOT = path.join(os.tmpdir(), "browsitory-e2e-workspace-root");
const E2E_WORKSPACE_REPO_A = path.join(E2E_WORKSPACE_ROOT, "repo-a");
const E2E_WORKSPACE_REPO_B = path.join(E2E_WORKSPACE_ROOT, "repo-b");
const E2E_WORKSPACE_REPO_C = path.join(E2E_WORKSPACE_ROOT, "repo-c");

async function waitForAppReady(): Promise<void> {
  await $('section[aria-label="Branches"] button[aria-expanded]').waitForExist({ timeout: 10000 });
}

async function openPickerOverlay(): Promise<void> {
  await waitForAppReady();
  await browser.execute((el) => (el as HTMLElement).click(), await $('button[aria-label="Open another repository"]'));
}

describe("Browsitory multi-repo workspaces", () => {
  it("opens all workspace members grouped under a chip, then closes the whole group at once", async () => {
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

    const repoCCheckbox = await $(`input[aria-label="${E2E_WORKSPACE_REPO_C}"]`);
    await repoCCheckbox.waitForExist({ timeout: 10000 });
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
    await repoCCheckboxAfterSave.waitForExist({ timeout: 10000 });
    expect(await repoCCheckboxAfterSave.isSelected()).toBe(true);
    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Cancel'));
  });

  it("Delete removes the workspace from the list after confirmation", async () => {
    await openPickerOverlay();

    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Delete E2E Workspace'));
    const confirmDialog = await $('dialog[aria-label="Delete workspace E2E Workspace"]');
    await confirmDialog.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), await $('button=Delete workspace'));

    await browser.waitUntil(
      async () => !(await $('span*=E2E Workspace').isExisting()),
      { timeout: 10000, timeoutMsg: "expected the workspace to be removed from the list" },
    );
  });
});
