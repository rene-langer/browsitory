import { expect } from "@wdio/globals";

// Deviation from the task brief: the brief sketched the fixture-repo setup as a `before` hook
// in this spec file. That doesn't work here — the app auto-opens its fixture repo path (baked
// into the build via `VITE_E2E_REPO_PATH`) as soon as it launches, and the app launches while
// the WebDriver session is established, which happens *before* any mocha `before` hook in this
// file runs. So the fixture repo is created in `wdio.conf.ts`'s `onPrepare` instead (runs once,
// before any session/app launch) — see the comments there for the full explanation.
describe("Browsitory first flow", () => {
  it("opens a repo, stages a file, commits, and sees it in history", async () => {
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });

    // Stage the one uncommitted file the `onPrepare` fixture setup created.
    const stageButton = await $("button=Stage");
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
