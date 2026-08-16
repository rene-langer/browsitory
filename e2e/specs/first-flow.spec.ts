import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

// Deviation from the task brief: the brief sketched the fixture-repo setup as a `before` hook
// in this spec file. That doesn't work here — the app auto-opens its fixture repo path (baked
// into the build via `VITE_E2E_REPO_PATH`) as soon as it launches, and the app launches while
// the WebDriver session is established, which happens *before* any mocha `before` hook in this
// file runs. So the fixture repo is created in `wdio.conf.ts`'s `onPrepare` instead (runs once,
// before any session/app launch) — see the comments there for the full explanation.
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

describe("Browsitory first flow", () => {
  // This spec originally relied on `onPrepare`'s uncommitted README.md still being there when
  // it ran. That held only by accident of alphabetical spec order and which files earlier
  // specs' own "prime the refresh" steps happen to stage — `commit-graph.spec.ts` sorts between
  // `branch-management.spec.ts` and this file and, like `blame-viewer.spec.ts` before it, stages
  // and commits whatever is uncommitted at that point, so nothing uncommitted is guaranteed to
  // survive to here. Write a fresh uncommitted file of our own so this spec is self-sufficient
  // regardless of what earlier specs left behind, same pattern as the other specs.
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, "first-flow-fixture.txt"), "first flow\n");
  });

  it("opens a repo, stages a file, commits, and sees it in history", async () => {
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });

    // Stage the uncommitted file this spec's own `before` hook created.
    const stageButton = await $("button=Stage");
    await stageButton.scrollIntoView({ block: "center" });

    await stageButton.click();
    await commitMessageInput.setValue("e2e: first commit");
    const commitButton = await $("button=Commit");
    await commitButton.click();

    // The new commit should now appear in the history list.
    const historyEntry = await $("li*=e2e: first commit");
    await historyEntry.waitForExist({ timeout: 10000 });
    await expect(historyEntry).toBeExisting();
  });
});
