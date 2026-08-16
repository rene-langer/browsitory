import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const E2E_SUBMODULE_PATH = "deps/e2e-child";

describe("Browsitory submodules", () => {
  it("initializes and updates a local submodule, then shows its changed gitlink in parent status", async () => {
    const initializeButton = await $("button=Initialize " + E2E_SUBMODULE_PATH);
    await initializeButton.waitForExist({ timeout: 10000 });

    await initializeButton.click();
    const initializedIndicator = await $("span=Initialized");
    await initializedIndicator.waitForExist({ timeout: 10000 });

    const updateButton = await $("button=Update " + E2E_SUBMODULE_PATH);
    await updateButton.click();
    const childPath = path.join(E2E_REPO_PATH, E2E_SUBMODULE_PATH);
    await browser.waitUntil(
      () => {
        try {
          execFileSync("git", ["rev-parse", "--verify", "HEAD"], {
            cwd: childPath,
            stdio: "ignore",
          });
          return true;
        } catch {
          return false;
        }
      },
      { timeout: 10000, timeoutMsg: "expected the submodule update to check out a valid HEAD" },
    );
    await updateButton.waitForEnabled({ timeout: 10000 });

    execFileSync("git", ["config", "user.name", "Test User"], { cwd: childPath });
    fs.writeFileSync(path.join(childPath, "advanced.txt"), "advanced child commit\n");
    execFileSync("git", ["config", "user.email", "test@example.com"], { cwd: childPath });
    execFileSync("git", ["add", "."], { cwd: childPath });
    execFileSync("git", ["commit", "-m", "e2e: advance child"], { cwd: childPath });

    await initializeButton.click();
    const gitlinkStatus = await $("button=" + E2E_SUBMODULE_PATH + " (Modified)");
    await gitlinkStatus.waitForExist({ timeout: 10000 });
    await expect(gitlinkStatus).toBeExisting();
  });
});
