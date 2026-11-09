import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { gitIn } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const OLD_NAME = "history-old.txt";
const NEW_NAME = "history-new.txt";
const BODY = "l1\nl2\nl3\nl4\nl5\n";

const git = gitIn(E2E_REPO_PATH);

async function clickViaScript(selector: string) {
  const el = await $(selector);
  await el.waitForExist();
  await browser.execute((node) => (node as HTMLElement).scrollIntoView({ block: "center" }), el);
  await browser.execute((node) => (node as HTMLElement).click(), el);
}

describe("Browsitory file history", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, OLD_NAME), BODY);
    git("add", OLD_NAME);
    git("commit", "-m", "e2e: history add");
    git("mv", OLD_NAME, NEW_NAME);
    fs.writeFileSync(path.join(E2E_REPO_PATH, NEW_NAME), "l1\nl2\nl3\nl4\nCHANGED\n");
    git("add", NEW_NAME);
    git("commit", "-m", "e2e: history rename");
    // Leave the file modified so it shows in the working tree with a History button.
    fs.writeFileSync(path.join(E2E_REPO_PATH, NEW_NAME), "l1\nl2\nl3\nl4\nCHANGED\nl6\n");
  });

  it("opens a file's history, shows the rename diff, and closes back to the graph", async () => {
    await clickViaScript(`button[aria-label="History of ${NEW_NAME}"]`);

    const banner = await $(`[role="toolbar"][aria-label="File history"]`);
    await banner.waitForExist();
    await expect(banner).toHaveText(expect.stringContaining(NEW_NAME));

    const renameRow = await $("li*=e2e: history rename");
    await renameRow.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), renameRow);
    await (await $(`span=Renamed from ${OLD_NAME}`)).waitForExist();

    // The rename diff is against the old path, so only the edited line is added.
    await (await $(".diff-line-add*=CHANGED")).waitForExist();

    const addRow = await $("li*=e2e: history add");
    await addRow.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), addRow);
    await (await $(".diff-line-add*=l1")).waitForExist();

    await clickViaScript(`button[aria-label="Close file history"]`);
    await (await $(`ul[aria-label="Commit history"]`)).waitForExist();
  });
});
