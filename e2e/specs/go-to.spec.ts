import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { openPalette, waitForAppReady } from "../support/app";
import { clickInPage, waitUntilGoneInPage } from "../support/dom";
import { expandSidebarSection } from "../support/sidebar";
import { gitIn } from "../support/git";

// "Go to commit / branch / tag" (#441): selects and scrolls to a target without checking it out.
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const TAG = "e2e-goto-tag";
const HIDDEN = "e2e-goto-hidden";

const git = gitIn(E2E_REPO_PATH);

function commitFile(name: string, message: string): string {
  fs.writeFileSync(path.join(E2E_REPO_PATH, name), `${message}\n`);
  git("add", name);
  git("commit", "-m", message);
  return git("rev-parse", "HEAD");
}

/** The full id of the graph row that is selected, or `null`. */
function selectedCommitId(): Promise<string | null> {
  return browser.execute(
    () =>
      document
        .querySelector('[data-active-repo="true"] ul[aria-label="Commit history"] li[aria-selected="true"]')
        ?.getAttribute("data-commit-id") ?? null,
  );
}

async function waitForSelected(commitId: string, what: string): Promise<void> {
  await browser.waitUntil(async () => (await selectedCommitId()) === commitId, {
    timeoutMsg: `expected the graph to select ${what} (${commitId})`,
  });
}

/** The text of the palette option Enter would run. Read in the page: `getText()` returns ""
 *  inside the palette's native <dialog> (see `command-palette.spec.ts`). */
function highlighted(input: WebdriverIO.Element): Promise<string> {
  return browser.execute((el) => {
    const id = (el as HTMLElement).getAttribute("aria-activedescendant");
    return id === null ? "" : (document.getElementById(id)?.textContent ?? "");
  }, input);
}

/**
 * Waits until the option Enter would run is `text` (or, with `prefix`, starts with it), then
 * presses Enter. Exact for commands: "Go to branch…" and "Go to Branches" share a prefix.
 */
async function chooseHighlighted(input: WebdriverIO.Element, text: string, prefix = false): Promise<void> {
  let seen = "";
  await browser.waitUntil(
    async () => {
      seen = (await highlighted(input)).trim();
      return prefix ? seen.startsWith(text) : seen === text;
    },
    { timeoutMsg: `expected the command palette to highlight "${text}", not "${seen}"` },
  );
  await browser.keys(["Enter"]);
}

/** Opens the picker `command` ("Go to commit…"), types `query` and goes to the first match. */
async function goToWithPicker(command: string, query: string, firstMatch: string): Promise<void> {
  await waitForAppReady();
  const input = await openPalette();
  // Typed without the ellipsis, as a user would.
  await input.setValue(command.replace(/…$/, "").toLowerCase());
  await chooseHighlighted(input, command);
  await browser.waitUntil(async () => (await input.getAttribute("aria-label")) === command.replace(/…$/, ""), {
    timeoutMsg: `expected the ${command} picker to open`,
  });
  await input.setValue(query);
  await chooseHighlighted(input, firstMatch, true);
  await waitUntilGoneInPage("dialog input[role='combobox']");
}

const TAG_ROW = `//ul[@aria-label='Tags']/li[.//button[@aria-label='Actions for ${TAG}']]`;

describe("Browsitory go to commit, branch or tag", () => {
  let taggedId = "";
  let laterId = "";
  let hiddenId = "";
  let headBefore = "";

  before(() => {
    taggedId = commitFile("goto-tagged.txt", "e2e: go to tagged commit");
    git("tag", TAG);
    laterId = commitFile("goto-later.txt", "e2e: go to later commit");
    const base = git("rev-parse", "--abbrev-ref", "HEAD");
    git("checkout", "-b", HIDDEN);
    hiddenId = commitFile("goto-hidden.txt", "e2e: go to hidden branch commit");
    git("checkout", base);
    headBefore = git("rev-parse", "HEAD");
  });

  // The graph's branch selection is saved in the suite's config folder, which outlives this
  // spec: a branch left hidden (or a list of branches that omits the ones later specs create)
  // would hide their commits. "Show all branches" forgets it, even after a failed test.
  after(async () => {
    await browser.keys(["Escape"]);
    await browser.refresh();
    await expandSidebarSection("Branches");
    await clickInPage("section[aria-label='Branches'] button[aria-label='Add']");
    await $("[role='menu']").waitForExist();
    const restore = await browser.execute(() => {
      const item = Array.from(document.querySelectorAll("[role='menu'] [role='menuitem']")).find(
        (element) => element.textContent?.trim() === "Show all branches",
      ) as HTMLButtonElement | undefined;
      if (item === undefined) return "missing";
      if (item.disabled || item.getAttribute("aria-disabled") === "true") return "already all";
      item.click();
      return "restored";
    });
    if (restore === "already all") await browser.keys(["Escape"]);
    if (restore === "missing") throw new Error('the Branches "+" menu has no "Show all branches"');
    await waitUntilGoneInPage("[role='menu']");
    git("tag", "-d", TAG);
    git("branch", "-D", HIDDEN);
  });

  it("clicking a tag in the Tags panel selects its commit without checking it out", async () => {
    await expandSidebarSection("Tags");
    const row = await $(TAG_ROW);
    await row.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), row);

    await waitForSelected(taggedId, `the commit of ${TAG}`);
    expect(git("rev-parse", "HEAD")).toBe(headBefore);
  });

  it("a focused tag row goes to its commit with Enter and opens its menu with Shift+F10", async () => {
    await expandSidebarSection("Tags");
    const row = await $(TAG_ROW);
    await row.waitForExist();
    await browser.execute((el) => (el as HTMLElement).focus(), row);

    await browser.keys(["Enter"]);
    await waitForSelected(taggedId, `the commit of ${TAG}`);

    await browser.execute((el) => (el as HTMLElement).focus(), row);
    await browser.keys(["Shift", "F10"]);
    await $("[role='menu']").waitForExist();
    await browser.pause(300);
    expect(await $$("[role='menu']")).toHaveLength(1);
    await browser.keys(["Escape"]);
    await waitUntilGoneInPage("[role='menu']");
    expect(await browser.execute((el) => document.activeElement === (el as Element), row)).toBe(true);
  });

  it("Go to commit… finds a commit by its SHA prefix", async () => {
    await goToWithPicker("Go to commit…", laterId.slice(0, 8), laterId.slice(0, 7));
    await waitForSelected(laterId, "the commit typed by SHA");
    expect(git("rev-parse", "HEAD")).toBe(headBefore);
  });

  // #618: Back/Forward through the selected commits, and the parent key.
  it("Back returns from a Go to to the commit selected before it, and Forward goes again", async () => {
    await waitForAppReady();
    await clickInPage(`[data-active-repo="true"] li[data-commit-id="${taggedId}"]`);
    await waitForSelected(taggedId, "the clicked commit");

    await goToWithPicker("Go to commit…", laterId.slice(0, 8), laterId.slice(0, 7));
    await waitForSelected(laterId, "the commit typed by SHA");

    await clickInPage('[data-active-repo="true"] button[aria-label="Back"]');
    await waitForSelected(taggedId, "the commit selected before the Go to");

    // Alt+Right (Cmd+] on macOS) with the commit list focused.
    await browser.execute(() =>
      document.querySelector<HTMLElement>('[data-active-repo="true"] ul[aria-label="Commit history"]')?.focus(),
    );
    await browser.keys(process.platform === "darwin" ? ["Meta", "]"] : ["Alt", "ArrowRight"]);
    await waitForSelected(laterId, "the Go to target again");

    // `[`: the later commit's first parent is the tagged one.
    await browser.keys(["["]);
    await waitForSelected(taggedId, "the parent of the later commit");
    expect(git("rev-parse", "HEAD")).toBe(headBefore);
  });

  it("Go to branch… explains a hidden branch and shows it on request", async () => {
    await expandSidebarSection("Branches");
    const swatch = await $(`button[aria-label="Show ${HIDDEN} in graph"]`);
    await swatch.waitForExist();
    if ((await swatch.getAttribute("aria-pressed")) === "true") {
      await browser.execute((el) => (el as HTMLElement).click(), swatch);
    }
    await browser.waitUntil(async () => (await swatch.getAttribute("aria-pressed")) === "false", {
      timeoutMsg: `expected ${HIDDEN} to be hidden in the graph`,
    });

    await goToWithPicker("Go to branch…", HIDDEN, HIDDEN);

    const notice = await $(`p[role="status"]*=is not in the graph`);
    await notice.waitForExist();
    const show = await $(`button=Show ${HIDDEN}`);
    await browser.execute((el) => (el as HTMLElement).click(), show);

    await waitForSelected(hiddenId, `the tip of ${HIDDEN}`);
    await browser.waitUntil(async () => (await swatch.getAttribute("aria-pressed")) === "true", {
      timeoutMsg: `expected ${HIDDEN} to be shown in the graph again`,
    });
    expect(git("rev-parse", "HEAD")).toBe(headBefore);
  });
});
