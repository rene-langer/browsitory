import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, waitUntilGoneInPage } from "../support/dom";
import { expandSidebarSection, sidebarSectionToggle } from "../support/sidebar";

// "Go to commit / branch / tag" (#441): selects and scrolls to a target without checking it out.
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const TAG = "e2e-goto-tag";
const HIDDEN = "e2e-goto-hidden";

function git(args: string[]): string {
  return execFileSync("git", args, { cwd: E2E_REPO_PATH, encoding: "utf8" }).trim();
}

function commitFile(name: string, message: string): string {
  fs.writeFileSync(path.join(E2E_REPO_PATH, name), `${message}\n`);
  git(["add", name]);
  git(["commit", "-m", message]);
  return git(["rev-parse", "HEAD"]);
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
    timeout: 10000,
    timeoutMsg: `expected the graph to select ${what} (${commitId})`,
  });
}

/** The palette's input; its label is the picker's name once a picker is open. */
async function openPalette() {
  await $(sidebarSectionToggle("Branches")).waitForExist({ timeout: 10000 });
  const input = await $('input[aria-label="Command palette"]');
  // Retried: see `command-palette.spec.ts`'s `openPalette`.
  await browser.waitUntil(
    async () => {
      if (await input.isDisplayed()) return true;
      await browser.keys(["Control", "k"]);
      return input.waitForDisplayed({ timeout: 1000 }).then(
        () => true,
        () => false,
      );
    },
    { timeout: 10000, interval: 0, timeoutMsg: "expected Ctrl/Cmd+K to open the command palette" },
  );
  return input;
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
    { timeout: 10000, timeoutMsg: `expected the command palette to highlight "${text}", not "${seen}"` },
  );
  await browser.keys(["Enter"]);
}

/** Opens the picker `command` ("Go to commit…"), types `query` and goes to the first match. */
async function goToWithPicker(command: string, query: string, firstMatch: string): Promise<void> {
  const input = await openPalette();
  // Typed without the ellipsis, as a user would.
  await input.setValue(command.replace(/…$/, "").toLowerCase());
  await chooseHighlighted(input, command);
  await browser.waitUntil(async () => (await input.getAttribute("aria-label")) === command.replace(/…$/, ""), {
    timeout: 10000,
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
    git(["tag", TAG]);
    laterId = commitFile("goto-later.txt", "e2e: go to later commit");
    const base = git(["rev-parse", "--abbrev-ref", "HEAD"]);
    git(["checkout", "-b", HIDDEN]);
    hiddenId = commitFile("goto-hidden.txt", "e2e: go to hidden branch commit");
    git(["checkout", base]);
    headBefore = git(["rev-parse", "HEAD"]);
  });

  // The graph's branch selection is saved in the suite's config folder, which outlives this
  // spec: a branch left hidden (or a list of branches that omits the ones later specs create)
  // would hide their commits. "Show all branches" forgets it, even after a failed test.
  after(async () => {
    await browser.keys(["Escape"]);
    await browser.refresh();
    await expandSidebarSection("Branches");
    await clickInPage("section[aria-label='Branches'] button[aria-label='Add']");
    await $("[role='menu']").waitForExist({ timeout: 10000 });
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
    git(["tag", "-d", TAG]);
    git(["branch", "-D", HIDDEN]);
  });

  it("clicking a tag in the Tags panel selects its commit without checking it out", async () => {
    await expandSidebarSection("Tags");
    const row = await $(TAG_ROW);
    await row.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), row);

    await waitForSelected(taggedId, `the commit of ${TAG}`);
    expect(git(["rev-parse", "HEAD"])).toBe(headBefore);
  });

  it("a focused tag row goes to its commit with Enter and opens its menu with Shift+F10", async () => {
    await expandSidebarSection("Tags");
    const row = await $(TAG_ROW);
    await row.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).focus(), row);

    await browser.keys(["Enter"]);
    await waitForSelected(taggedId, `the commit of ${TAG}`);

    await browser.execute((el) => (el as HTMLElement).focus(), row);
    await browser.keys(["Shift", "F10"]);
    await $("[role='menu']").waitForExist({ timeout: 10000 });
    await browser.pause(300);
    expect(await $$("[role='menu']")).toHaveLength(1);
    await browser.keys(["Escape"]);
    await waitUntilGoneInPage("[role='menu']");
    expect(await browser.execute((el) => document.activeElement === (el as Element), row)).toBe(true);
  });

  it("Go to commit… finds a commit by its SHA prefix", async () => {
    await goToWithPicker("Go to commit…", laterId.slice(0, 8), laterId.slice(0, 7));
    await waitForSelected(laterId, "the commit typed by SHA");
    expect(git(["rev-parse", "HEAD"])).toBe(headBefore);
  });

  it("Go to branch… explains a hidden branch and shows it on request", async () => {
    await expandSidebarSection("Branches");
    const swatch = await $(`button[aria-label="Show ${HIDDEN} in graph"]`);
    await swatch.waitForExist({ timeout: 10000 });
    if ((await swatch.getAttribute("aria-pressed")) === "true") {
      await browser.execute((el) => (el as HTMLElement).click(), swatch);
    }
    await browser.waitUntil(async () => (await swatch.getAttribute("aria-pressed")) === "false", {
      timeout: 10000,
      timeoutMsg: `expected ${HIDDEN} to be hidden in the graph`,
    });

    await goToWithPicker("Go to branch…", HIDDEN, HIDDEN);

    const notice = await $(`p[role="status"]*=is not in the graph`);
    await notice.waitForExist({ timeout: 10000 });
    const show = await $(`button=Show ${HIDDEN}`);
    await browser.execute((el) => (el as HTMLElement).click(), show);

    await waitForSelected(hiddenId, `the tip of ${HIDDEN}`);
    await browser.waitUntil(async () => (await swatch.getAttribute("aria-pressed")) === "true", {
      timeout: 10000,
      timeoutMsg: `expected ${HIDDEN} to be shown in the graph again`,
    });
    expect(git(["rev-parse", "HEAD"])).toBe(headBefore);
  });
});
