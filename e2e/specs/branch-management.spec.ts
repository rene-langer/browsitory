import { execFileSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { expandSidebarSection } from "../support/sidebar";

// Deviation from the task brief: the brief assumed the fixture repo already has a commit by the
// time this spec runs, so a branch can be created "from HEAD". But `wdio.conf.ts`'s `onPrepare`
// (shared across every spec file in the run — it runs once for the whole suite, not per spec)
// only `git init`s the fixture repo and writes an *uncommitted* README.md; the first commit is
// made by `first-flow.spec.ts`'s own test body, not by fixture setup. WebdriverIO loads spec
// files in alphabetical order, so `branch-management.spec.ts` runs *before*
// `first-flow.spec.ts` and hits a genuinely unborn HEAD — `git2::Repository::head()` errors on
// that, so the backend's `create_branch(..., "HEAD")` fails and `useAppState` swallows it into
// `state.error` rather than updating `branches`. Observed symptom: the branch switcher stayed on
// "no branch" until the 10s `waitUntil` below timed out. Editing the shared `onPrepare` was out
// of scope for this task, so this spec makes its own repo state self-sufficient instead: an
// `--allow-empty` commit in `before()` guarantees a resolvable HEAD regardless of what other
// specs have or haven't done yet (order-independent, and harmless to run twice if a commit
// already exists — e.g. if `first-flow.spec.ts` happens to run first).
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

describe("Browsitory branch management", () => {
  before(() => {
    execFileSync(
      "git",
      ["commit", "--allow-empty", "-m", "e2e: seed commit so HEAD is resolvable"],
      { cwd: E2E_REPO_PATH, stdio: "inherit" },
    );
  });

  it("creates a branch from HEAD, switches to it, and shows it as current", async () => {
    // "Branches" defaults closed; expand it before its contents (the switcher button) exist.
    await expandSidebarSection("Branches");

    const switcherButton = await $('[aria-label="Branch switcher"]');
    await switcherButton.waitForExist({ timeout: 10000 });
    await switcherButton.click();

    const newBranchButton = await $("button=New Branch…");
    await newBranchButton.click();

    const nameInput = await $("input[placeholder='New branch name']");
    await nameInput.setValue("feature/e2e-branch");
    const createButton = await $("button=Create");
    await createButton.click();

    // Switching happens automatically on create; the toggle button's label should now read
    // the new branch's name.
    await browser.waitUntil(
      async () => (await switcherButton.getText()) === "feature/e2e-branch",
      { timeout: 10000, timeoutMsg: "expected the switcher to show the new branch as current" },
    );
  });
});
