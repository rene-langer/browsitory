import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { expandSidebarSection } from "../support/sidebar";
import { git, gitWith } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const STASH_FIXTURE_FILE = "stash-fixture.txt";

describe("Browsitory stash", () => {
  before(() => {
    const filePath = path.join(E2E_REPO_PATH, STASH_FIXTURE_FILE);
    fs.writeFileSync(filePath, "committed content\n");
    git(E2E_REPO_PATH, "add", STASH_FIXTURE_FILE);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: seed stash fixture file");
    fs.writeFileSync(filePath, "dirty content\n");
  });

  // `wdio.conf.ts`'s `onPrepare` builds the fixture repo once for the whole suite run, and
  // specs run alphabetically — leaving the one stash entry this spec creates (and the file's
  // dirty content) behind would corrupt the fixture for any spec sorting after "stash-".
  after(() => {
    const filePath = path.join(E2E_REPO_PATH, STASH_FIXTURE_FILE);
    try {
      gitWith(E2E_REPO_PATH, ["rev-parse", "--verify", "refs/stash"], { stdio: "ignore" });
      git(E2E_REPO_PATH, "stash", "drop", "0");
    } catch {
      // If the interaction failed before creating a stash, there is nothing to clean up.
    }
    fs.writeFileSync(filePath, "committed content\n");
  });

  it("saves a stash, sees it listed, applies it, and restores the file's dirty content", async () => {
    const uncommittedRow = await $("li*=Uncommitted changes");
    await uncommittedRow.waitForExist();
    const stashButton = await $("button=Stash");
    await stashButton.waitForEnabled();
    await stashButton.scrollIntoView({ block: "center" });

    await browser.execute((el) => (el as HTMLElement).click(), stashButton);

    // The stash list lives in its own "Stashes" accordion section (see
    // `frontend/src/components/StashPanel.tsx`), not inline in the commit history — expand it
    // before looking for the row it renders.
    await expandSidebarSection("Stashes");

    const stashRow = await $("li*=WIP on");
    await stashRow.waitForExist();

    // Apply sits in the row's actions menu ("…"), not inline in the row (#339).
    const actions = await stashRow.$('button[aria-label^="Actions for"]');
    await browser.execute((el) => (el as HTMLElement).click(), actions);
    const applyItem = await $('[role="menu"]').$("button=Apply");
    await applyItem.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), applyItem);

    await browser.waitUntil(
      async () => {
        const content = fs.readFileSync(path.join(E2E_REPO_PATH, STASH_FIXTURE_FILE), "utf8");
        return content === "dirty content\n";
      },
      {
        timeoutMsg: "expected the stashed file's content to be restored after Apply",
      },
    );
  });
});
