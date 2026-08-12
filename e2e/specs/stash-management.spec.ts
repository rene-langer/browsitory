import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const STASH_FIXTURE_FILE = "stash-fixture.txt";

describe("Browsitory stash", () => {
  before(() => {
    const filePath = path.join(E2E_REPO_PATH, STASH_FIXTURE_FILE);
    fs.writeFileSync(filePath, "committed content\n");
    execFileSync("git", ["add", STASH_FIXTURE_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: seed stash fixture file"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    fs.writeFileSync(filePath, "dirty content\n");
  });

  it("saves a stash, sees it listed, applies it, and restores the file's dirty content", async () => {
    const uncommittedRow = await $("li*=Uncommitted Changes");
    await uncommittedRow.waitForExist({ timeout: 10000 });
    await uncommittedRow.click();

    const stashButton = await $("button=Stash");
    await stashButton.click();

    const stashRow = await $("li*=WIP on");
    await stashRow.waitForExist({ timeout: 10000 });

    const applyButton = await $("button=Apply");
    await applyButton.click();

    await browser.waitUntil(
      async () => {
        const content = fs.readFileSync(path.join(E2E_REPO_PATH, STASH_FIXTURE_FILE), "utf8");
        return content === "dirty content\n";
      },
      {
        timeout: 10000,
        timeoutMsg: "expected the stashed file's content to be restored after Apply",
      },
    );
  });
});
