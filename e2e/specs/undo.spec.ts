import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { git } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "undo-e2e.txt";
const LABEL = `Discard changes to ${FILE}`;
const MOD = process.platform === "darwin" ? "Meta" : "Control";

function fileContents(): string | null {
  try {
    return fs.readFileSync(path.join(E2E_REPO_PATH, FILE), "utf8");
  } catch {
    return null;
  }
}

describe("Browsitory undo and redo", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "committed\n");
    git(E2E_REPO_PATH, "add", FILE);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: undo fixture");
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "precious edit\n");
  });

  it("restores a discarded file with Mod+Z and discards it again with Mod+Shift+Z", async () => {
    // Discard through the file's actions menu, as line-staging.spec.ts does.
    const actions = await $(`button[aria-label="Actions for ${FILE}"]`);
    await actions.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), actions);
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Discard changes…"));
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Discard changes"));
    await browser.waitUntil(() => fileContents() === "committed\n", {
      timeoutMsg: "discard did not land",
    });

    await $(`button[aria-label="Undo ${LABEL}"]`).waitForExist();
    await browser.keys([MOD, "z"]);
    await browser.waitUntil(() => fileContents() === "precious edit\n", {
      timeoutMsg: "undo did not restore the file",
    });
    // Read textContent via execute: WebKitGTK's getText() can come back empty for top-layer elements.
    await browser.waitUntil(
      async () => (await browser.execute(() => document.body.textContent ?? "")).includes(`Undid ${LABEL}`),
      { timeoutMsg: "expected an 'Undid' toast" },
    );

    await browser.keys([MOD, "Shift", "z"]);
    await browser.waitUntil(() => fileContents() === "committed\n", {
      timeoutMsg: "redo did not discard again",
    });
  });
});
