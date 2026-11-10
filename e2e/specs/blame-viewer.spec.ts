import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, waitUntilInPage } from "../support/dom";
import { git } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BLAME_FIXTURE_FILE = "blame-fixture.txt";
// Spec-unique name on purpose. `commit-graph.spec.ts` needs the same "write a throwaway file,
// stage+commit it through the UI to force a `useAppState` refresh" trick, and both specs used to
// call theirs `prime.txt`. That only works because `wdio.conf.ts`'s `beforeSession` re-clones the
// fixture repo and WebdriverIO starts a fresh worker (and therefore a fresh session) per spec
// file — the moment two specs share a worker, the second one's write is byte-identical to what
// the first already committed, which is no working-tree change at all: no file row, and no stage
// control to click. Distinct names make that impossible to trip over.
const PRIME_FILE = "blame-prime.txt";

describe("Browsitory blame", () => {
  before(() => {
    const fixturePath = path.join(E2E_REPO_PATH, BLAME_FIXTURE_FILE);
    fs.writeFileSync(fixturePath, "line one\nline two\n");
    git(E2E_REPO_PATH, "add", BLAME_FIXTURE_FILE);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: blame fixture first commit");

    fs.writeFileSync(fixturePath, "line one\nCHANGED LINE\n");
    git(E2E_REPO_PATH, "add", BLAME_FIXTURE_FILE);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: blame fixture second commit");

    fs.writeFileSync(path.join(E2E_REPO_PATH, PRIME_FILE), "prime\n");
  });

  it("shows per-line blame for a historic commit and jumps history selection on a line click", async () => {
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist();

    // Stage+commit the throwaway prime file through the real UI — the only way to make
    // useAppState refetch, which is what makes the two git-CLI commits above show up below.
    // The per-row stage control is icon-only (aria-label only, no visible text) since the
    // file-list migration to `ListRow` — target the throwaway prime file written above.
    const stageButton = await $(`button[aria-label="Stage ${PRIME_FILE}"]`);
    await stageButton.waitForExist();
    // `opacity: 0` until the row is hovered/focused — see `first-flow.spec.ts`'s note on why
    // both the scroll and the click go through `browser.execute`.
    await browser.execute((el) => (el as HTMLElement).scrollIntoView({ block: "center" }), stageButton);
    await browser.execute((el) => (el as HTMLElement).click(), stageButton);
    // Wait for the row to move under Staged before committing (#697).
    await waitUntilInPage(`button[aria-label="Unstage ${PRIME_FILE}"]`);
    await browser.execute((el) => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set; setter?.call(el, "e2e: prime the refresh"); el.dispatchEvent(new Event("input", { bubbles: true })); }, commitMessageInput);
    await clickInPage('button[data-action="commit"]');

    const secondCommitEntry = await $("li*=e2e: blame fixture second commit");
    await secondCommitEntry.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), secondCommitEntry);

    // Commit diffs render every file's section expanded by default (`DiffPane.tsx`'s
    // `CommitFileSection`) — the path is a plain header label now, not a click-to-reveal button.
    const fileEntry = await $(`span=${BLAME_FIXTURE_FILE}`);
    await fileEntry.waitForExist();

    const blameButton = await $("button=Blame");
    await browser.execute((el) => (el as HTMLElement).click(), blameButton);

    // Click a `<td>` inside the row, not the `<tr>` itself: WebKitGTK's WebDriver
    // implementation (what `tauri-driver` proxies to on Linux) reports bare `display:table-row`
    // elements as "element not interactable" even when they're fully visible on screen and have
    // a non-zero bounding rect — a driver quirk, not an app bug. The click still bubbles up to
    // the row's `onClick` handler (`BlameView.tsx`'s `<tr onClick={...}>`), so this exercises
    // the same behavior the brief's row click was meant to.
    const unchangedLineCell = await $("td*=line one");
    await unchangedLineCell.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), unchangedLineCell);

    await browser.waitUntil(
      async () =>
        (await $("li*=e2e: blame fixture first commit").getAttribute("aria-selected")) === "true",
      {
        timeoutMsg:
          "expected clicking the unchanged blame line to select the commit that introduced it, not the commit whose diff was already open",
      },
    );
  });
});
