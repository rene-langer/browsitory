import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, waitUntilGoneInPage, waitUntilInPage } from "../support/dom";
import { gitIn } from "../support/git";
import { SLOW_UI_TIMEOUT } from "../support/timeouts";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const git = gitIn(E2E_REPO_PATH);

/**
 * Picks `label` from the context menu of `commitId`'s row in the commit graph.
 *
 * A command run from the app lands before the graph refreshes, and a refresh closes an open
 * menu. A click on an item of a menu that a refresh already closed does nothing (#182). Seeing
 * the new commit's row doesn't prove the graph has settled either. So each poll, inside the
 * page: if the menu shows `label`, click it (found and clicked in one step, so nothing can close
 * the menu in between); otherwise open the menu on the row (again, if a refresh closed it).
 */
async function chooseFromCommitMenu(commitId: string, label: string) {
  await browser.waitUntil(
    () =>
      browser.execute(
        (id: string, wanted: string) => {
          const item = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menu"] button')).find(
            (button) => !button.disabled && (button.textContent ?? "").trim() === wanted,
          );
          if (item !== undefined) {
            item.click();
            return true;
          }
          document
            .querySelector(`li[data-commit-id="${id}"]`)
            ?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
          return false;
        },
        commitId,
        label,
      ),
    { timeoutMsg: `could not pick "${label}" from the menu of commit ${commitId}` },
  );
}

async function clickButton(label: string) {
  const item = await $(`button*=${label}`);
  await item.waitForExist();
  await browser.execute((el) => (el as HTMLElement).click(), item);
}

describe("Browsitory history rewrite", () => {
  let baseBranch = "";
  let startTip = "";
  let sourceCommit = "";

  before(() => {
    baseBranch = git("rev-parse", "--abbrev-ref", "HEAD");
    git("checkout", "-b", "e2e-pick-source");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "picked.txt"), "picked\n");
    git("add", "picked.txt");
    git("commit", "-m", "e2e: commit to cherry-pick");
    sourceCommit = git("rev-parse", "HEAD");
    git("checkout", baseBranch);
    startTip = git("rev-parse", "HEAD");
  });

  it("cherry-picks a commit, reverts it, then resets hard back", async () => {
    await chooseFromCommitMenu(sourceCommit, "Cherry-pick");
    // Wait on the commit, not the file: checkout writes picked.txt before the commit lands.
    await browser.waitUntil(() => git("log", "-1", "--format=%s") === "e2e: commit to cherry-pick");
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "picked.txt"))).toBe(true);

    // The picked copy is HEAD now. (Picked in the same second as the original, it has the same
    // id, as nothing in the commit differs.)
    await chooseFromCommitMenu(git("rev-parse", "HEAD"), "Revert");
    await browser.waitUntil(
      () => git("log", "-1", "--format=%s") === 'Revert "e2e: commit to cherry-pick"',
    );
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "picked.txt"))).toBe(false);

    await chooseFromCommitMenu(startTip, `Reset ${baseBranch} to here…`);
    const hard = await $("input[type=radio][value=hard]");
    await hard.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), hard);
    await clickButton("Reset");
    await clickButton("Reset hard");
    await browser.waitUntil(() => git("rev-parse", "HEAD") === startTip);
  });
});

// One-click history edits from the commit menu (#442): each runs as a one-step rebase.
describe("Browsitory one-click history edits", () => {
  const commitFile = (file: string, content: string, message: string) => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, file), content);
    git("add", file);
    git("commit", "-m", message);
  };
  // The newest commit with exactly this subject (ids change with every edit).
  const idOf = (subject: string) =>
    git("log", "--format=%H%x09%s", "-50")
      .split("\n")
      .map((line) => line.split("\t"))
      .find(([, s]) => s === subject)![0];
  const subjects = (count: number) => git("log", "--format=%s", `-${count}`).split("\n");
  const waitForSubjects = (expected: string[]) =>
    browser.waitUntil(() => JSON.stringify(subjects(expected.length)) === JSON.stringify(expected), {
      timeout: SLOW_UI_TIMEOUT,
      timeoutMsg: `history never became ${JSON.stringify(expected)}; it is ${JSON.stringify(subjects(expected.length))}`,
    });

  before(() => {
    commitFile("edit-1.txt", "1\n", "e2e: edit one");
    commitFile("edit-2.txt", "2\n", "e2e: edit two");
    commitFile("edit-3.txt", "3\n", "e2e: edit three");
    commitFile("edit-4.txt", "4\n", "e2e: edit four");
  });

  it("rewords an older commit, squashes one into its parent, moves one up and drops one", async () => {
    await browser.refresh();

    await chooseFromCommitMenu(idOf("e2e: edit two"), "Reword…");
    const field = await $("dialog[aria-label^='Reword commit'] textarea");
    await field.waitForExist();
    await browser.waitUntil(async () => (await field.getValue()).startsWith("e2e: edit two"));
    await field.setValue("e2e: edit two reworded");
    await clickInPage("dialog[aria-label^='Reword commit'] button", "Reword");
    await waitForSubjects(["e2e: edit four", "e2e: edit three", "e2e: edit two reworded", "e2e: edit one"]);

    await chooseFromCommitMenu(idOf("e2e: edit three"), "Squash with parent");
    await waitForSubjects(["e2e: edit four", "e2e: edit two reworded", "e2e: edit one"]);
    expect(git("log", "-1", "--format=%B", idOf("e2e: edit two reworded"))).toBe(
      "e2e: edit two reworded\n\ne2e: edit three",
    );
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "edit-3.txt"))).toBe(true);

    await chooseFromCommitMenu(idOf("e2e: edit two reworded"), "Move up");
    await waitForSubjects(["e2e: edit two reworded", "e2e: edit four", "e2e: edit one"]);

    await chooseFromCommitMenu(idOf("e2e: edit four"), "Drop");
    await waitForSubjects(["e2e: edit two reworded", "e2e: edit one"]);
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "edit-4.txt"))).toBe(false);
    expect(git("status", "--porcelain", "--untracked-files=no")).toBe("");
  });

  it("pauses a move that conflicts, and Abort puts the branch back", async () => {
    commitFile("edit-move.txt", "one\n", "e2e: move base");
    commitFile("edit-move.txt", "two\n", "e2e: move first");
    commitFile("edit-move.txt", "three\n", "e2e: move second");
    const tipBefore = git("rev-parse", "HEAD");
    await browser.refresh();

    // "second" changes the line "first" wrote: replayed before it, it conflicts.
    await chooseFromCommitMenu(idOf("e2e: move second"), "Move down");
    const panel = await $("section[aria-label='Rebase in progress']");
    await panel.waitForExist({ timeout: SLOW_UI_TIMEOUT });
    await waitUntilInPage("span", "edit-move.txt (Conflicted)", { contains: true });

    await clickInPage("section[aria-label='Rebase in progress'] button", "Abort rebase");
    await waitUntilGoneInPage("section[aria-label='Rebase in progress']");
    await browser.waitUntil(() => git("rev-parse", "HEAD") === tipBefore);
    expect(git("rev-parse", "--abbrev-ref", "HEAD")).not.toBe("HEAD");
    expect(fs.readFileSync(path.join(E2E_REPO_PATH, "edit-move.txt"), "utf8")).toBe("three\n");
  });
});
