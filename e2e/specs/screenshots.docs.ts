import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expandSidebarSection } from "../support/sidebar";

// One-off spec whose purpose is capturing real screenshots for docs/USER_GUIDE.md, not
// assertions. Named `.docs.ts` rather than `.spec.ts` so `wdio.conf.ts`'s default `specs` glob
// (`./specs/**/*.spec.ts`) never picks it up — it only runs when explicitly targeted:
// `pnpm test -- --spec ./specs/screenshots.docs.ts`. Kept in the suite directory (not deleted
// after use) so the screenshots can be regenerated whenever the UI changes, the same way the
// rest of the E2E suite documents behavior.
//
// Clicks/text-entry below go through `browser.execute` rather than WebdriverIO's own
// `.click()`/`.setValue()`, matching every other spec in this suite (see e.g.
// first-flow.spec.ts) — not a screenshot-specific workaround.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const SCREENSHOT_DIR = path.resolve(__dirname, "../../docs/assets");

function git(args: string[]): void {
  execFileSync("git", args, { cwd: E2E_REPO_PATH, stdio: "inherit" });
}

describe("Browsitory screenshots (docs)", () => {
  before(() => {
    fs.mkdirSync(SCREENSHOT_DIR, { recursive: true });

    // Base history: two commits on the default branch, a tag on the tip, then a feature branch
    // with its own commit, back on the default branch — enough for the graph to show a fork.
    fs.writeFileSync(
      path.join(E2E_REPO_PATH, "README.md"),
      "# Demo Project\n\nA sample repository used to capture Browsitory's documentation screenshots.\n",
    );
    git(["add", "README.md"]);
    git(["commit", "-m", "docs: seed README"]);

    fs.writeFileSync(path.join(E2E_REPO_PATH, "src.txt"), "line one\nline two\n");
    git(["add", "src.txt"]);
    git(["commit", "-m", "feat: add src.txt"]);
    git(["tag", "-a", "v1.0.0", "-m", "First release"]);

    const defaultBranch = execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], { cwd: E2E_REPO_PATH })
      .toString()
      .trim();
    git(["checkout", "-b", "feature/screenshots"]);
    fs.writeFileSync(path.join(E2E_REPO_PATH, "feature.txt"), "feature work in progress\n");
    git(["add", "feature.txt"]);
    git(["commit", "-m", "feat: add feature work"]);
    git(["checkout", defaultBranch]);

    // `resetFixtureRepo` in wdio.conf.ts clones the parent source, which already leaves an
    // "origin" remote pointing at a local tmp path — repoint it at a demo-looking URL for the
    // Remotes panel screenshot. The panel only lists configured remotes, it never has to
    // actually reach them.
    git(["remote", "set-url", "origin", "https://example.com/demo/browsitory-demo.git"]);

    // Working-tree changes for the staging screenshot: one staged new file, one unstaged edit
    // to an already-tracked file.
    fs.writeFileSync(path.join(E2E_REPO_PATH, "notes.txt"), "todo: polish the release notes\n");
    git(["add", "notes.txt"]);
    fs.writeFileSync(path.join(E2E_REPO_PATH, "src.txt"), "line one\nline two, revised\nline three\n");
  });

  it("captures the default working-directory view", async () => {
    // `AccordionSection` persists open/closed per section to localStorage, and the app's
    // profile isn't reset between separate local/CI runs of this suite — clear it so this shot
    // always starts from every sidebar section's true default (closed), not whatever an
    // unrelated earlier run happened to leave open.
    await browser.execute(() => localStorage.clear());
    await browser.refresh();

    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });
    // Let the graph/status finish rendering before the shot.
    await $("li*=feat: add feature work").waitForExist({ timeout: 10000 });
    await browser.saveScreenshot(path.join(SCREENSHOT_DIR, "overview.png"));
  });

  it("captures staging an unstaged change with a commit message typed", async () => {
    const unstagedFileButton = await $("button*=src.txt (Modified)");
    await unstagedFileButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), unstagedFileButton);
    await $(".diff-line-add").waitForExist({ timeout: 10000 });

    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await browser.execute(
      (el) => {
        const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
        setter?.call(el, "feat: revise src.txt");
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      commitMessageInput,
    );

    await browser.saveScreenshot(path.join(SCREENSHOT_DIR, "staging.png"));
  });

  it("captures a commit's diff from history", async () => {
    const commitEntry = await $("li*=feat: add src.txt");
    await commitEntry.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), commitEntry);

    const fileButton = await $("button=src.txt");
    await fileButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), fileButton);
    await $(".diff-line-add").waitForExist({ timeout: 10000 });

    await browser.saveScreenshot(path.join(SCREENSHOT_DIR, "commit-diff.png"));
  });

  it("captures the branch switcher open over the commit graph", async () => {
    await expandSidebarSection("Branches");
    const switcherButton = await $('[aria-label="Branch switcher"]');
    await switcherButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), switcherButton);
    await $("li*=feature/screenshots").waitForExist({ timeout: 10000 });

    await browser.saveScreenshot(path.join(SCREENSHOT_DIR, "branches.png"));

    // Close the popover so it doesn't linger over later screenshots in this run.
    await browser.execute((el) => (el as HTMLElement).click(), switcherButton);
  });

  it("captures the Tags sidebar panel", async () => {
    await expandSidebarSection("Tags");
    await $("label*=v1.0.0").waitForExist({ timeout: 10000 });

    await browser.saveScreenshot(path.join(SCREENSHOT_DIR, "tags.png"));
  });

  it("captures the Remotes sidebar panel", async () => {
    // "Tags" is still expanded (localStorage-persisted from the previous test, and `beforeTest`
    // only reloads the page — it doesn't clear storage) and would otherwise push Remotes below
    // the fold. Collapse it first so this shot isn't cut off.
    const tagsTrigger = await $("section[aria-label='Tags'] button[aria-expanded]");
    await tagsTrigger.waitForExist({ timeout: 10000 });
    if ((await tagsTrigger.getAttribute("aria-expanded")) === "true") {
      await browser.execute((el) => (el as HTMLElement).click(), tagsTrigger);
    }

    await expandSidebarSection("Remotes");
    await $("strong=origin").waitForExist({ timeout: 10000 });

    await browser.saveScreenshot(path.join(SCREENSHOT_DIR, "remotes.png"));
  });

  it("captures the command palette open and filtered", async () => {
    // `beforeTest` (wdio.conf.ts) refreshes the page before every `it`, and the repo's async
    // auto-open (App.tsx) can still be in flight right when the test starts — Ctrl+K no-ops
    // until `state.repoPath` is set, so wait for a sidebar section to exist first (same gate
    // command-palette.spec.ts uses).
    await $('section[aria-label="Branches"] button[aria-expanded]').waitForExist({ timeout: 10000 });
    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed({ timeout: 10000 });
    await input.setValue("branch");
    await $("li*=Branch").waitForExist({ timeout: 10000 });

    await browser.saveScreenshot(path.join(SCREENSHOT_DIR, "command-palette.png"));

    await browser.keys(["Escape"]);
  });
});
