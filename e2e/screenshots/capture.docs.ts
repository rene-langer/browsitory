import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { clickInPage, waitUntilInPage } from "../support/dom";
import { expandSidebarSection, sidebarBranchRow, sidebarRemoteFolder, sidebarSectionToggle } from "../support/sidebar";
import { SLOW_UI_TIMEOUT } from "../support/timeouts";
import {
  addFailingPushRemote,
  cleanForEdit,
  DEMO_REPO_PATH,
  PRE_PUSH_OUTPUT,
  removeFailingPushRemote,
  startConflict,
} from "./demoRepo";

// Captures the documentation screenshots into docs/guide/assets/ (see ./README.md). Run it with
// `pnpm screenshots`, never as part of the E2E suite: it asserts nothing beyond "the screen it
// shows has loaded".
//
// The tests run in order and build on each other (sidebar sections opened by one stay open), so
// the config stops at the first failure (`bail`). Clicks go through `browser.execute`, like the
// rest of the E2E suite.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ASSETS = path.resolve(__dirname, "../../docs/guide/assets");
const WINDOW = { width: 1280, height: 800 };
const MERGE_EDITOR_WINDOW = { width: 1600, height: 1000 };

async function click(selector: string): Promise<void> {
  const element = await $(selector);
  await element.waitForExist();
  await browser.execute((el) => (el as HTMLElement).click(), element);
}

async function collapseSidebarSection(title: string): Promise<void> {
  const trigger = await $(sidebarSectionToggle(title));
  await trigger.waitForExist();
  if ((await trigger.getAttribute("aria-expanded")) === "true") {
    await browser.execute((el) => (el as HTMLElement).click(), trigger);
  }
}

/** Waits for rendering to settle (fonts, layout, a pending fetch), then saves the shot. */
async function shoot(name: string): Promise<void> {
  await browser.execute(() => (document.activeElement as HTMLElement | null)?.blur());
  await browser.pause(600);
  await browser.saveScreenshot(path.join(ASSETS, `${name}.png`));
}

async function setTheme(theme: "light" | "dark"): Promise<void> {
  const toggle = await $(`button[aria-label="Switch to ${theme} theme"]`);
  if (await toggle.isExisting()) await browser.execute((el) => (el as HTMLElement).click(), toggle);
  await browser.waitUntil(async () => (await browser.execute(() => document.documentElement.dataset.theme)) === theme, {
    timeout: 5000,
    timeoutMsg: `the app did not switch to the ${theme} theme`,
  });
}

/** Opens Preferences on `group`. */
async function openPreferencesGroup(group: string): Promise<void> {
  await click('button[aria-label="Preferences"]');
  await $('input[aria-label="Filter settings"]').waitForDisplayed();
  await click(`//div[@aria-label="Preference groups"]/button[normalize-space()="${group}"]`);
}

async function closePreferences(): Promise<void> {
  await browser.keys(["Escape"]);
  await $('input[aria-label="Filter settings"]').waitForDisplayed({ reverse: true });
}

/** Adds a command in Preferences → Commands, as a user types it (#893). */
async function addUserCommand(
  scope: string,
  list: string,
  name: string,
  commandLine: string,
  targets: string[],
): Promise<void> {
  await click(`button[aria-label="Add a command for ${scope}"]`);
  const form = 'form[aria-label="New command"]';
  await $(form).waitForDisplayed();
  const [nameField, commandField] = await $$(`${form} input[type="text"]`);
  await nameField.setValue(name);
  await commandField.setValue(commandLine);
  for (const target of targets) {
    await click(`//form[@aria-label="New command"]//label[normalize-space()="${target}"]/input`);
  }
  await clickInPage(`${form} button`, "Save");
  await $(form).waitForExist({ reverse: true });
  await waitUntilInPage(`ul[aria-label="${list}"] li`, name, { contains: true });
}

/** Runs a command from the palette by its name. */
async function runFromPalette(label: string): Promise<void> {
  await browser.keys(["Control", "k"]);
  const input = await $('input[aria-label="Command palette"]');
  await input.waitForDisplayed();
  await input.setValue(label);
  await waitUntilInPage("li", label, { contains: true });
  await browser.keys(["Enter"]);
  await input.waitForDisplayed({ reverse: true });
}

describe("Documentation screenshots", () => {
  before(async () => {
    fs.mkdirSync(ASSETS, { recursive: true });
    await browser.setWindowSize(WINDOW.width, WINDOW.height);
    // Start from default sidebar state; the E2E build has just opened its own fixture repo.
    await browser.execute(() => localStorage.clear());
    await browser.refresh();
    await $(sidebarSectionToggle("Branches")).waitForExist({ timeout: SLOW_UI_TIMEOUT });
    await setTheme("light");

    // Open the demo from the picker's recent list, then close the E2E fixture's tab.
    await click('button[aria-label="Open another repository"]');
    await click(`button*=${DEMO_REPO_PATH}`);
    await browser.waitUntil(async () => (await (await $$('[role="tab"]')).length) === 2, {
      timeoutMsg: "the demo repository did not open in a second tab",
    });
    const fixtureTabClose = await $(
      `//button[@role="tab" and not(@title="${DEMO_REPO_PATH}")]/following-sibling::button[1]`,
    );
    await browser.execute((el) => (el as HTMLElement).click(), fixtureTabClose);
    await browser.waitUntil(async () => (await (await $$('[role="tab"]')).length) === 1, {
      timeoutMsg: "the E2E fixture tab did not close",
    });
    await $("li*=Add setup steps to the README").waitForExist();
  });

  it("overview", async () => {
    await $("li*=Uncommitted changes").waitForExist();
    await shoot("overview");
  });

  it("staging", async () => {
    // By text in the page: "(Modified)" is screen-reader text, which `span*=` doesn't match.
    await clickInPage("span", "src/app.js (Modified)");
    await waitUntilInPage(".diff-line-add");
    const message = await $("textarea[placeholder='Commit message']");
    await browser.execute(
      (el) => {
        const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
        setter?.call(el, "Add zoom controls to the toolbar");
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      message,
    );
    await shoot("staging");
  });

  it("commit-diff", async () => {
    await click("li*=Limit the tile cache to 200 entries");
    await waitUntilInPage(".diff-line-add");
    await shoot("commit-diff");
  });

  it("branches", async () => {
    await expandSidebarSection("Branches");
    await $(sidebarBranchRow("feature/offline-tiles")).waitForExist();
    const origin = await $(sidebarRemoteFolder("origin"));
    await origin.waitForExist();
    if ((await origin.getAttribute("aria-expanded")) !== "true") {
      await browser.execute((el) => (el as HTMLElement).click(), origin);
      await browser.waitUntil(async () => (await origin.getAttribute("aria-expanded")) === "true", {
        timeout: 5000,
        timeoutMsg: "the origin folder did not expand",
      });
    }
    await shoot("branches");
  });

  it("stashes", async () => {
    await collapseSidebarSection("Branches");
    await expandSidebarSection("Stashes");
    await $("span*=Try a closer default zoom").waitForExist();
    await shoot("stashes");
  });

  it("tags", async () => {
    await collapseSidebarSection("Stashes");
    await expandSidebarSection("Tags");
    await $('button[aria-label="Actions for v0.2.0"]').waitForExist();
    await shoot("tags");
    await collapseSidebarSection("Tags");
    await expandSidebarSection("Branches");
  });

  it("preferences", async () => {
    await click('button[aria-label="Preferences"]');
    const filter = await $('input[aria-label="Filter settings"]');
    await filter.waitForDisplayed();
    // The Appearance group: theme, fonts, sizes and the date format.
    await click('//div[@aria-label="Preference groups"]/button[normalize-space()="Appearance"]');
    await $("legend=Appearance").waitForDisplayed();
    await shoot("preferences");
    await browser.keys(["Escape"]);
    await filter.waitForDisplayed({ reverse: true });
  });

  it("command-palette", async () => {
    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed();
    await input.setValue("branch");
    // Any matching command will do: the palette lists several.
    await waitUntilInPage("li", "Branch", { contains: true });
    await browser.saveScreenshot(path.join(ASSETS, "command-palette.png"));
    await browser.keys(["Escape"]);
    await input.waitForDisplayed({ reverse: true });
  });

  it("preferences-keyboard", async () => {
    await openPreferencesGroup("Keyboard");
    await $("legend=Keyboard shortcuts").waitForDisplayed();
    await shoot("preferences-keyboard");
    await closePreferences();
  });

  it("preferences-commands", async () => {
    await openPreferencesGroup("Commands");
    await $("legend=Commands").waitForDisplayed();
    // Two commands for every repository and one for the demo only, added as a user would (#893).
    await addUserCommand("all repositories", "All repositories", "Show commit stats", "git show --stat ${commit}", ["Commit"]);
    await addUserCommand("all repositories", "All repositories", "Compare with main", "git diff --stat main...${branch}", [
      "Branch",
    ]);
    await addUserCommand("trailmap", "Only in trailmap", "Lint the files", "npx eslint ${paths}", ["File"]);
    // The first one open in its editor: its fields, menus and options.
    const editor = 'form[aria-label="Edit Show commit stats"]';
    await click('button[aria-label="Edit Show commit stats"]');
    await $(editor).waitForDisplayed();
    await shoot("preferences-commands");
    await clickInPage(`${editor} button`, "Cancel");
    await $(editor).waitForExist({ reverse: true });
    await closePreferences();
  });

  it("operation-log", async () => {
    // A command of your own on the selected commit, then a push that its pre-push hook refuses.
    await click("li*=Limit the tile cache to 200 entries");
    await runFromPalette("Run: Show commit stats");
    const output = 'dialog[aria-label="Show commit stats"]';
    await waitUntilInPage(`${output} pre`, "map.js", { contains: true, timeout: SLOW_UI_TIMEOUT });
    await clickInPage(`${output} button`, "Close");

    addFailingPushRemote();
    await runFromPalette("Refresh");
    const remote = await $(sidebarRemoteFolder("backup"));
    await remote.waitForExist({ timeout: SLOW_UI_TIMEOUT });
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
    }, remote);
    await clickInPage("[role='menu'] button", "Push current branch here");
    // Push may first ask where to: confirm the suggestion.
    const pushDialog = 'dialog[aria-label="Push main"]';
    await browser.waitUntil(
      () =>
        browser.execute(
          (dialogSelector: string) =>
            document.querySelector(dialogSelector) !== null ||
            Array.from(document.querySelectorAll("[role='alert']")).some((el) =>
              (el.textContent ?? "").includes("The pre-push hook failed."),
            ),
          pushDialog,
        ),
      { timeout: SLOW_UI_TIMEOUT, timeoutMsg: "expected the push to ask where to, or to fail" },
    );
    if (await (await $(pushDialog)).isExisting()) await clickInPage(`${pushDialog} button`, "Push");
    await waitUntilInPage("[role='alert']", "The pre-push hook failed.", { contains: true, timeout: SLOW_UI_TIMEOUT });
    await clickInPage("button", "Show details");
    await waitUntilInPage("pre[aria-label='Output of pre-push']", PRE_PUSH_OUTPUT, {
      contains: true,
      timeout: SLOW_UI_TIMEOUT,
    });
    await shoot("operation-log");
    await clickInPage("dialog[aria-label='Operation log'] button", "Close");
    // Dismiss the push's error and take the remote away again, so the later shots don't show them.
    await browser.execute(() => {
      document.querySelectorAll<HTMLButtonElement>("[role='alert'] button[aria-label='Dismiss error']").forEach((b) => b.click());
    });
    removeFailingPushRemote();
    await runFromPalette("Refresh");
    await $(sidebarRemoteFolder("backup")).waitForExist({ reverse: true, timeout: SLOW_UI_TIMEOUT });
  });

  it("conflict", async () => {
    startConflict();
    // Reload status through the palette's "Refresh" command.
    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed();
    await input.setValue("Refresh");
    await browser.keys(["Enter"]);
    // The status strip and the commit box both offer it.
    await waitUntilInPage("button", "Abort merge", { contains: true });
    await click("li*=Uncommitted changes");
    // Conflicted files head the list (#665): selecting the row shows its resolve controls.
    // By text in the page: "(Conflicted)" is screen-reader text, which `span*=` doesn't match.
    await clickInPage("span", "src/search.js (Conflicted)");
    await waitUntilInPage("button", "Mark resolved", { contains: true });
    await shoot("conflict");
  });

  it("dark-theme", async () => {
    await setTheme("dark");
    await shoot("dark-theme");
    await setTheme("light");
  });

  it("merge-editor", async () => {
    // A taller and wider window, so the merge editor has room for its three panes (#875) next
    // to the sidebar and the history, and shows a whole conflict block under the file list.
    await browser.setWindowSize(MERGE_EDITOR_WINDOW.width, MERGE_EDITOR_WINDOW.height);
    const block = await $('section[aria-label="Conflict 1 of 1"]');
    await block.waitForExist();
    await waitUntilInPage('button[aria-pressed="true"]', "Three panes");
    // Scroll to the editor's toolbar ("0 of 1 resolved", Previous conflict), not to the block:
    // the block at the top cut the toolbar off (#944). The block still fits below it. The pane's
    // sticky "Diff view options" row covers the top of the scroll area: stop below it.
    const toolbar = await $('[role="group"][aria-label="Conflicts"]');
    await toolbar.waitForExist();
    const options = await $('[role="group"][aria-label="Diff view options"]');
    await browser.execute(
      (el, sticky) => {
        const target = el as HTMLElement;
        target.scrollIntoView({ block: "start" });
        let scroller = target.parentElement;
        while (scroller && !/(auto|scroll)/.test(getComputedStyle(scroller).overflowY)) {
          scroller = scroller.parentElement;
        }
        const below = (sticky as HTMLElement).getBoundingClientRect().bottom + 2;
        scroller?.scrollBy(0, target.getBoundingClientRect().top - below);
      },
      toolbar,
      options,
    );
    await shoot("merge-editor");
    await browser.setWindowSize(WINDOW.width, WINDOW.height);
  });

  it("edit-commit", async () => {
    // Edit commit… (#882, #885) needs a clean tracked tree: end the merge, stash the changes.
    cleanForEdit();
    // Reload status through the palette's "Refresh" command, as for the conflict.
    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed();
    await input.setValue("Refresh");
    await browser.keys(["Enter"]);
    await browser.waitUntil(async () => !(await (await $("button*=Abort merge")).isExisting()), {
      timeout: SLOW_UI_TIMEOUT,
      timeoutMsg: "the app still shows the merge",
    });
    const subject = "Add setup steps to the README";
    await $(`li*=${subject}`).waitForExist({ timeout: SLOW_UI_TIMEOUT });
    // Found and clicked inside the page in one step, as history-rewrite.spec.ts does: a refresh
    // can close the menu between finding the item and clicking it.
    await browser.waitUntil(
      () =>
        browser.execute((wanted: string) => {
          const item = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menu"] button')).find(
            (button) => !button.disabled && (button.textContent ?? "").trim() === "Edit commit…",
          );
          if (item !== undefined) {
            item.click();
            return true;
          }
          Array.from(document.querySelectorAll("li"))
            .find((li) => (li.textContent ?? "").includes(wanted))
            ?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
          return false;
        }, subject),
      { timeout: SLOW_UI_TIMEOUT, timeoutMsg: "could not pick Edit commit… from the menu" },
    );
    await waitUntilInPage("[role='status']", "Editing ", { contains: true, timeout: SLOW_UI_TIMEOUT });
    await click("li*=Uncommitted changes");
    await waitUntilInPage("button", "Commit part");
    await clickInPage("span", "README.md (Modified)");
    await waitUntilInPage(".diff-line-add");
    await shoot("edit-commit");
  });
});
