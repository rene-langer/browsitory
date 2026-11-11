import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage } from "../support/dom";
import { git } from "../support/git";
import { UI_TIMEOUT } from "../support/timeouts";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILES = ["multi-select-a.txt", "multi-select-b.txt", "multi-select-c.txt"];

// Selecting several changed files and staging them with one action (#449).
describe("Browsitory multi-select in the change lists", () => {
  before(() => {
    for (const file of FILES) fs.writeFileSync(path.join(E2E_REPO_PATH, file), `${file}\n`);
  });

  after(() => {
    // Leave the shared fixture repo clean for the next spec: nothing staged, no new files.
    git(E2E_REPO_PATH, "reset", "-q", "--", ...FILES);
    for (const file of FILES) fs.rmSync(path.join(E2E_REPO_PATH, file), { force: true });
  });

  it("Ctrl+click selects two files and the selection bar stages both", async () => {
    const first = await $(`span=${FILES[0]} (New)`);
    await first.waitForExist({ timeout: UI_TIMEOUT });
    // The row's `{path} ({kind})` text is a `<span>` inside `<li role="option">`; the click
    // bubbles to the row, as in `hunk-staging.spec.ts`.
    await browser.execute((el) => (el as HTMLElement).click(), first);
    await browser.execute(
      (el) => (el as HTMLElement).dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true })),
      await $(`span=${FILES[2]} (New)`),
    );

    const stageSelected = await $("button=Stage 2 files");
    await stageSelected.waitForExist({ timeout: UI_TIMEOUT });
    await browser.execute((el) => (el as HTMLElement).click(), stageSelected);

    await browser.waitUntil(
      () => {
        const staged = git(E2E_REPO_PATH, "diff", "--cached", "--name-only").split("\n");
        return staged.includes(FILES[0]) && staged.includes(FILES[2]);
      },
      { timeout: UI_TIMEOUT, timeoutMsg: "the two selected files were not staged" },
    );
    expect(git(E2E_REPO_PATH, "diff", "--cached", "--name-only").split("\n")).not.toContain(FILES[1]);
  });
});

const TRACKED = "stage-modified-tracked.txt";
const UNTRACKED = "stage-modified-new.txt";

// "Stage modified" stages the tracked files' changes and leaves untracked files alone (#621).
describe("Browsitory Stage modified in the Changes heading", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, TRACKED), "committed\n");
    git(E2E_REPO_PATH, "add", TRACKED);
    git(E2E_REPO_PATH, "commit", "-q", "-m", "e2e: stage modified fixture");
    fs.writeFileSync(path.join(E2E_REPO_PATH, TRACKED), "edited\n");
    fs.writeFileSync(path.join(E2E_REPO_PATH, UNTRACKED), "new\n");
  });

  after(() => {
    git(E2E_REPO_PATH, "reset", "-q", "--", TRACKED, UNTRACKED);
    git(E2E_REPO_PATH, "checkout", "--", TRACKED);
    fs.rmSync(path.join(E2E_REPO_PATH, UNTRACKED), { force: true });
  });

  it("stages the modified tracked file and not the untracked one", async () => {
    await $(`span=${UNTRACKED} (New)`).waitForExist({ timeout: UI_TIMEOUT });
    const stageModified = await $("button=Stage modified");
    await stageModified.waitForExist({ timeout: UI_TIMEOUT });
    await browser.execute((el) => (el as HTMLElement).click(), stageModified);

    await browser.waitUntil(
      () => git(E2E_REPO_PATH, "diff", "--cached", "--name-only").split("\n").includes(TRACKED),
      { timeout: UI_TIMEOUT, timeoutMsg: "the modified tracked file was not staged" },
    );
    expect(git(E2E_REPO_PATH, "diff", "--cached", "--name-only").split("\n")).not.toContain(UNTRACKED);
    expect(git(E2E_REPO_PATH, "status", "--porcelain", "--", UNTRACKED).trim()).toBe(`?? ${UNTRACKED}`);
  });
});

const KEPT = "delete-untracked-tracked.txt";
const NEW_FILES = ["delete-untracked-a.txt", "delete-untracked-b.txt"];

// "Delete untracked…" deletes the untracked files and keeps the tracked file's change (#622).
describe("Browsitory Delete untracked in the Changes heading", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, KEPT), "committed\n");
    git(E2E_REPO_PATH, "add", KEPT);
    git(E2E_REPO_PATH, "commit", "-q", "-m", "e2e: delete untracked fixture");
    fs.writeFileSync(path.join(E2E_REPO_PATH, KEPT), "edited\n");
    for (const file of NEW_FILES) fs.writeFileSync(path.join(E2E_REPO_PATH, file), `${file}\n`);
  });

  after(() => {
    git(E2E_REPO_PATH, "checkout", "--", KEPT);
    for (const file of NEW_FILES) fs.rmSync(path.join(E2E_REPO_PATH, file), { force: true });
  });

  it("deletes the two untracked files and leaves the modified one changed", async () => {
    await $(`span=${NEW_FILES[1]} (New)`).waitForExist({ timeout: UI_TIMEOUT });
    await clickInPage("button", "Delete untracked…");

    const dialog = await $('dialog[aria-label="Delete untracked files"]');
    await dialog.waitForExist({ timeout: UI_TIMEOUT });
    // Exactly this spec's two files, so nothing else untracked in the shared fixture is deleted.
    await expect(dialog).toHaveText(expect.stringContaining("Delete 2 untracked files? Undo can bring them back."));
    await expect(dialog).toHaveText(expect.stringContaining(NEW_FILES[0]));
    await clickInPage('dialog[aria-label="Delete untracked files"] button', "Delete 2 files");

    await browser.waitUntil(() => NEW_FILES.every((file) => !fs.existsSync(path.join(E2E_REPO_PATH, file))), {
      timeout: UI_TIMEOUT,
      timeoutMsg: "the untracked files were not deleted",
    });
    expect(git(E2E_REPO_PATH, "status", "--porcelain", "--", KEPT)).toBe(`M ${KEPT}`);
    expect(fs.readFileSync(path.join(E2E_REPO_PATH, KEPT), "utf8")).toBe("edited\n");
  });
});

const FILTER_DIR = "tree-filter/x";
const FILTERED = [`${FILTER_DIR}/one.txt`, `${FILTER_DIR}/two.txt`];
const OUTSIDE = "tree-filter-outside.txt";

// The path filter and the tree layout of the change lists (#729): while the filter hides files,
// "Stage all" stages the shown ones only; a folder's Unstage unstages its files.
describe("Browsitory path filter and tree layout in the change lists", () => {
  before(() => {
    fs.mkdirSync(path.join(E2E_REPO_PATH, FILTER_DIR), { recursive: true });
    for (const file of [...FILTERED, OUTSIDE]) fs.writeFileSync(path.join(E2E_REPO_PATH, file), `${file}\n`);
  });

  after(async () => {
    git(E2E_REPO_PATH, "reset", "-q", "--", ...FILTERED, OUTSIDE);
    fs.rmSync(path.join(E2E_REPO_PATH, "tree-filter"), { recursive: true, force: true });
    fs.rmSync(path.join(E2E_REPO_PATH, OUTSIDE), { force: true });
    // The layout is a per-machine preference: later specs expect the flat list.
    await browser.execute(() => localStorage.removeItem("working-tree-file-layout"));
  });

  it("stages only the filtered files, then unstages their folder in the tree", async () => {
    await $(`span=${OUTSIDE} (New)`).waitForExist({ timeout: UI_TIMEOUT });
    const filter = await $("[data-active-repo='true'] input[aria-label='Filter files']");
    await filter.setValue(FILTER_DIR);

    const stageShown = await $("button=Stage 2 shown");
    await stageShown.waitForExist({ timeout: UI_TIMEOUT });
    await browser.execute((el) => (el as HTMLElement).click(), stageShown);
    await browser.waitUntil(
      () => {
        const staged = git(E2E_REPO_PATH, "diff", "--cached", "--name-only").split("\n");
        return FILTERED.every((file) => staged.includes(file));
      },
      { timeout: UI_TIMEOUT, timeoutMsg: "the filtered files were not staged" },
    );
    expect(git(E2E_REPO_PATH, "status", "--porcelain", "--", OUTSIDE).trim()).toBe(`?? ${OUTSIDE}`);

    await clickInPage("[data-active-repo='true'] button[aria-pressed]", "Tree");
    const unstageFolder = await $(`[data-active-repo='true'] button[aria-label="Unstage folder ${FILTER_DIR}"]`);
    await unstageFolder.waitForExist({ timeout: UI_TIMEOUT });
    await browser.execute((el) => (el as HTMLElement).click(), unstageFolder);
    await browser.waitUntil(
      () => {
        const staged = git(E2E_REPO_PATH, "diff", "--cached", "--name-only").split("\n");
        return FILTERED.every((file) => !staged.includes(file));
      },
      {
        timeout: UI_TIMEOUT,
        timeoutMsg: "the folder's files were not unstaged",
      },
    );

    await clickInPage("[data-active-repo='true'] button[aria-pressed]", "Flat");
    await filter.clearValue();
  });
});
