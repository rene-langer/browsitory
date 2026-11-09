import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { git } from "../support/git";
import { clickInPage, waitUntilInPage } from "../support/dom";
import { UI_TIMEOUT } from "../support/timeouts";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "tree-e2e/nested.txt";
const MOD = process.platform === "darwin" ? "Meta" : "Control";

function fileContents(): string | null {
  try {
    return fs.readFileSync(path.join(E2E_REPO_PATH, FILE), "utf8");
  } catch {
    return null;
  }
}

// Browsing a commit's files and restoring one of them (#446).
describe("Browsitory files at a commit", () => {
  let first = "";

  before(() => {
    fs.mkdirSync(path.join(E2E_REPO_PATH, "tree-e2e"), { recursive: true });
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "version one\n");
    git(E2E_REPO_PATH, "add", FILE);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: tree v1");
    first = git(E2E_REPO_PATH, "rev-parse", "HEAD");
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "version two\n");
    git(E2E_REPO_PATH, "commit", "-am", "e2e: tree v2");
  });

  after(() => {
    git(E2E_REPO_PATH, "checkout", "--", FILE);
  });

  it("shows a file as it was in an older commit and restores it, undoably", async () => {
    const row = await $("li*=e2e: tree v1");
    await row.waitForExist({ timeout: UI_TIMEOUT });
    await browser.execute((el) => (el as HTMLElement).click(), row);

    await clickInPage("button", "All files");
    const filter = await $('input[aria-label="Filter files"]');
    await filter.waitForExist({ timeout: UI_TIMEOUT });
    await filter.setValue("nested");
    await clickInPage('[role="tree"] button', FILE, { contains: true });
    await waitUntilInPage(`[role="region"][aria-label="Content of ${FILE}"]`, "version one", { contains: true });

    await clickInPage("button", "Restore…");
    await clickInPage('dialog[aria-label="Confirm restore"] button', "Restore");
    await browser.waitUntil(() => fileContents() === "version one\n", {
      timeout: UI_TIMEOUT,
      timeoutMsg: "restore did not write the older version",
    });

    const label = `Restore ${FILE} from ${first.slice(0, 7)}`;
    await $(`button[aria-label="Undo ${label}"]`).waitForExist({ timeout: UI_TIMEOUT });
    await browser.keys([MOD, "z"]);
    await browser.waitUntil(() => fileContents() === "version two\n", {
      timeout: UI_TIMEOUT,
      timeoutMsg: "undo did not bring the file back",
    });

    await clickInPage("button", "Changes");
  });
});
