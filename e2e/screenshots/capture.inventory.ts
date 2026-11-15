import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { SLOW_UI_TIMEOUT } from "../support/timeouts";
import * as h from "./inventoryHelpers";
import { DEMO_REPO_PATH, SCREENSHOT_ROOT } from "./demoRepo";

const EMPTY_REPO_PATH = path.join(SCREENSHOT_ROOT, "empty-repo");

// UX-review screenshot inventory: every reachable screen, menu and dialog, for design reviews
// (not the guide screenshots, which capture.docs.ts makes). Needs the E2E build; run from e2e/:
//
//   TMPDIR=/tmp dbus-run-session -- xvfb-run -a -s "-screen 0 1920x1200x24" \
//     pnpm screenshots:inventory
//
// Shots go to e2e/inventory-out/shots/ (ignored by git), or to INVENTORY_SHOTS.
// INVENTORY_PHASES=A,B,... limits the run to some phases (default: all). Each shot is its own
// test; a failing one is reported and the run goes on (no bail). Probe results (menu items,
// dialog labels) go to notes.jsonl next to the shots folder.

const PHASES = (process.env.INVENTORY_PHASES ?? "A,B,C,D,E,F,G,H,I,J").split(",");
const COMMIT_OLD = "Limit the tile cache to 200 entries";
const COMMIT_HEAD = "Add setup steps to the README";
const COMMIT_MERGE = "Merge branch 'feature/dark-mode'";
const COMMIT_OFFLINE = "Show a banner while offline";

async function openDemo(): Promise<void> {
  await browser.setWindowSize(1280, 800);
  await browser.execute(() => localStorage.clear());
  await browser.refresh();
  await $(h.sidebarSectionToggle("Branches")).waitForExist({ timeout: SLOW_UI_TIMEOUT });
  await h.setTheme("light");
  await h.click('button[aria-label="Open another repository"]');
  await h.click(`button*=${DEMO_REPO_PATH}`);
  await browser.waitUntil(async () => (await (await $$('[role="tab"]')).length) === 2);
  const fixtureTabClose = await $(`//button[@role="tab" and not(@title="${DEMO_REPO_PATH}")]/following-sibling::button[1]`);
  await browser.execute((el) => (el as HTMLElement).click(), fixtureTabClose);
  await browser.waitUntil(async () => (await (await $$('[role="tab"]')).length) === 1);
  await $(`li*=${COMMIT_HEAD}`).waitForExist();
}

/** Closes menus and dialogs a step left open. */
async function tidy(): Promise<void> {
  await h.esc(3);
  await browser.execute(() => {
    document.querySelectorAll<HTMLButtonElement>("[role='alert'] button[aria-label='Dismiss error']").forEach((b) => b.click());
    // Inline forms (new branch, add remote) stay open on Escape: cancel them.
    document.querySelectorAll<HTMLFormElement>("[data-active-repo='true'] form").forEach((form) => {
      if (form.closest("dialog")) return;
      Array.from(form.querySelectorAll<HTMLButtonElement>("button"))
        .find((b) => (b.textContent ?? "").trim() === "Cancel")
        ?.click();
    });
    // Reword turns on "Amend last commit" in the commit box: turn it off again.
    document.querySelectorAll<HTMLLabelElement>("label").forEach((label) => {
      const input = label.querySelector<HTMLInputElement>("input[type='checkbox']");
      if ((label.textContent ?? "").includes("Amend last commit") && input?.checked) input.click();
    });
  });
}

function phase(id: string, title: string, body: () => void): void {
  if (!PHASES.includes(id)) return;
  describe(`${id}: ${title}`, body);
}

/** Opens a dialog with `open`, notes what opened and shoots it. */
async function dialogShot(name: string, open: () => Promise<void>): Promise<void> {
  await open();
  await browser.pause(700);
  h.note(name, await h.dialogs());
  await h.shoot(name);
}

async function selectCommit(subject: string): Promise<void> {
  await h.click(`li*=${subject}`);
  await browser.pause(500);
}

async function selectWorkingTree(): Promise<void> {
  await h.click("li*=Uncommitted changes");
  await browser.pause(500);
}

async function branchMenu(name: string): Promise<string[]> {
  await h.expandSidebarSection("Branches");
  await h.click(`button[aria-label="Actions for ${name}"]`);
  await browser.pause(400);
  return h.menu();
}

async function commitMenu(subject: string): Promise<string[]> {
  return h.ctx(`li*=${subject}`);
}

describe("UX inventory", () => {
  before(async () => {
    await openDemo();
  });
  afterEach(async () => {
    await tidy();
  });

  phase("A", "main window, panes, themes, sizes", () => {
    it("main-overview-light", async () => {
      await $("li*=Uncommitted changes").waitForExist();
      h.note("sections", await h.texts("[data-active-repo='true'] section[aria-label]", "aria-label"));
      h.note("toolbar-buttons", await h.texts("[data-active-repo='true'] button[aria-label]", "aria-label"));
      await h.shoot("main-overview-light");
    });
    it("main-overview-dark", async () => {
      await h.setTheme("dark");
      await h.shoot("main-overview-dark");
      await h.setTheme("light");
    });
    it("main-narrow-900x700", async () => {
      await h.size(900, 700);
      await h.shoot("main-narrow-900x700-light");
      await h.setTheme("dark");
      await h.shoot("main-narrow-900x700-dark");
      await h.setTheme("light");
      await h.size(1280, 800);
    });
    it("main-wide-1600x1000", async () => {
      await h.size(1600, 1000);
      await h.shoot("main-wide-1600x1000");
      await h.size(1280, 800);
    });
    it("staging-unstaged-file-diff", async () => {
      await selectWorkingTree();
      await h.clickInPage("span", "src/app.js (Modified)");
      await h.waitUntilInPage(".diff-line-add");
      await h.shoot("staging-unstaged-file-diff");
    });
    it("staging-staged-new-file", async () => {
      await h.clickInPage("span", "src/zoom.js", { contains: true });
      await browser.pause(800);
      await h.shoot("staging-staged-new-file");
    });
    it("staging-untracked-file", async () => {
      await h.clickInPage("span", "NOTES.md", { contains: true });
      await browser.pause(800);
      await h.shoot("staging-untracked-file");
    });
    it("staging-commit-message-typed", async () => {
      await h.setValueInPage("textarea[placeholder='Commit message']", "Add zoom controls to the toolbar\n\nTwo buttons, + and -.");
      await h.shoot("staging-commit-message-typed");
      await h.setValueInPage("textarea[placeholder='Commit message']", "");
    });
    it("commit-details-and-diff", async () => {
      await selectCommit(COMMIT_OLD);
      await h.waitUntilInPage(".diff-line-add");
      await h.shoot("commit-details-and-diff");
    });
    it("commit-details-merge-commit", async () => {
      await selectCommit(COMMIT_MERGE);
      await h.shoot("commit-details-merge-commit");
    });
    it("commit-details-dark", async () => {
      await selectCommit(COMMIT_OLD);
      await h.setTheme("dark");
      await h.shoot("commit-details-and-diff-dark");
      await h.setTheme("light");
    });
    it("sidebar sections", async () => {
      const sections = ["Branches", "Stashes", "Worktrees", "Submodules", "Reflog", "Tags", "Pull requests"];
      for (const title of sections) {
        try {
          await h.expandSidebarSection(title);
          await browser.pause(500);
          await h.shoot(`sidebar-${title.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`);
          const toggle = await $(h.sidebarSectionToggle(title));
          if ((await toggle.getAttribute("aria-expanded")) === "true") {
            await browser.execute((el) => (el as HTMLElement).click(), toggle);
          }
        } catch (error) {
          h.note(`sidebar-fail-${title}`, String(error));
        }
      }
      await h.expandSidebarSection("Branches");
    });
    it("sidebar-hidden", async () => {
      await h.click('[data-active-repo="true"] button[aria-label="Sidebar"]');
      await h.shoot("main-sidebar-hidden");
      await h.click('[data-active-repo="true"] button[aria-label="Sidebar"]');
    });
    it("stash-diff", async () => {
      await h.expandSidebarSection("Stashes");
      await h.click("span*=Try a closer default zoom");
      await browser.pause(800);
      await h.shoot("stash-selected-diff");
    });
  });

  phase("B", "context menus", () => {
    it("menu-commit", async () => {
      h.note("menu-commit", await commitMenu(COMMIT_OLD));
      await h.shoot("menu-commit");
    });
    it("menu-commit-head", async () => {
      h.note("menu-commit-head", await commitMenu(COMMIT_HEAD));
      await h.shoot("menu-commit-head");
    });
    it("menu-commit-other-branch", async () => {
      h.note("menu-commit-other-branch", await commitMenu(COMMIT_OFFLINE));
      await h.shoot("menu-commit-other-branch");
    });
    it("menu-commit-uncommitted", async () => {
      h.note("menu-uncommitted", await h.ctx("li*=Uncommitted changes"));
      await h.shoot("menu-uncommitted-changes");
    });
    it("menu-branch-local", async () => {
      h.note("menu-branch-local", await branchMenu("feature/offline-tiles"));
      await h.shoot("menu-branch-local");
    });
    it("menu-branch-current", async () => {
      h.note("menu-branch-current", await branchMenu("main"));
      await h.shoot("menu-branch-current");
    });
    it("menu-branch-remote", async () => {
      const origin = await $(h.sidebarRemoteFolder("origin"));
      if ((await origin.getAttribute("aria-expanded")) !== "true") await browser.execute((el) => (el as HTMLElement).click(), origin);
      await browser.pause(300);
      h.note("menu-branch-remote", await branchMenu("origin/feature/dark-mode"));
      await h.shoot("menu-branch-remote");
    });
    it("menu-remote", async () => {
      await h.click('button[aria-label="Actions for remote origin"]');
      await browser.pause(400);
      h.note("menu-remote", await h.menu());
      await h.shoot("menu-remote");
    });
    it("menu-tag", async () => {
      await h.expandSidebarSection("Tags");
      await h.click('button[aria-label="Actions for v0.2.0"]');
      await browser.pause(400);
      h.note("menu-tag", await h.menu());
      await h.shoot("menu-tag");
    });
    it("menu-stash", async () => {
      await h.expandSidebarSection("Stashes");
      h.note("menu-stash", await h.ctx("span*=Try a closer default zoom"));
      await h.shoot("menu-stash");
    });
    it("menu-file-unstaged", async () => {
      await selectWorkingTree();
      h.note("menu-file-unstaged", await h.ctx("//span[contains(., 'src/app.js')]"));
      await h.shoot("menu-file-unstaged");
    });
    it("menu-file-staged", async () => {
      h.note("menu-file-staged", await h.ctx("//span[contains(., 'src/zoom.js')]"));
      await h.shoot("menu-file-staged");
    });
    it("menu-file-in-commit", async () => {
      await selectCommit(COMMIT_OLD);
      h.note("menu-file-commit", await h.ctx("span*=src/map.js"));
      await h.shoot("menu-file-in-commit");
    });
    it("menu-repo-tab", async () => {
      h.note("menu-tab", await h.ctx('[role="tab"]'));
      await h.shoot("menu-repository-tab");
    });
    it("menu-help", async () => {
      await h.click('button[aria-label="Help"]');
      await browser.pause(400);
      h.note("menu-help", await h.menu());
      await h.shoot("menu-help");
    });
    it("menu-fetch-split", async () => {
      await h.click('button[aria-label="More fetch actions"]');
      await browser.pause(400);
      h.note("menu-fetch", await h.menu());
      await h.shoot("menu-fetch-options");
    });
    it("menu-pull-split", async () => {
      await h.click('button[aria-label="Pull options"]');
      await browser.pause(400);
      h.note("menu-pull", await h.menu());
      await h.shoot("menu-pull-options");
    });
    it("menu-stash-split", async () => {
      await h.click('button[aria-label="More stash options"]');
      await browser.pause(400);
      h.note("menu-stash-split", await h.menu());
      await h.shoot("menu-stash-options");
    });
    it("menu-commands-button", async () => {
      const labels = await h.texts("button[aria-label]", "aria-label");
      h.note("all-button-labels", labels);
    });
    it("menu-branch-switcher", async () => {
      await h.click("button[title='Switch branch, or create one']");
      await browser.pause(500);
      h.note("branch-switcher", await h.dialogs());
      await h.shoot("menu-branch-switcher");
    });
  });

  phase("C", "palette, preferences, help dialogs", () => {
    it("palette-all", async () => {
      await h.openPalette();
      h.note("palette-groups", await h.texts('[role="listbox"] [role="presentation"], [role="listbox"] [role="group"]', "aria-label"));
      h.note("palette-items", await h.texts('[role="option"]'));
      await h.shoot("palette-empty-query", { keepFocus: true });
      // Page through the whole list: one shot per screenful, so every group shows once.
      for (let page = 2; page <= 40; page++) {
        const moved = await browser.execute(() => {
          let el: HTMLElement | null = document.querySelector('[role="listbox"]');
          while (el && !(el.scrollHeight > el.clientHeight + 4 && /(auto|scroll)/.test(getComputedStyle(el).overflowY))) {
            el = el.parentElement;
          }
          if (!el) return false;
          const before = el.scrollTop;
          el.scrollTop = before + el.clientHeight - 30;
          return el.scrollTop > before;
        });
        if (!moved) break;
        await h.shoot(`palette-empty-query-page-${page}`, { keepFocus: true, pause: 400 });
      }
    });
    it("palette-filtered", async () => {
      await h.openPalette("stash");
      await h.shoot("palette-filter-stash", { keepFocus: true });
      await h.esc();
      await h.openPalette("zzzzqqq");
      await h.shoot("palette-no-results", { keepFocus: true });
    });
    it("palette-dark", async () => {
      await h.setTheme("dark");
      await h.openPalette("branch");
      await h.shoot("palette-filter-branch-dark", { keepFocus: true });
      await h.esc();
      await h.setTheme("light");
    });
    it("preferences groups", async () => {
      await h.click('button[aria-label="Preferences"]');
      await $('input[aria-label="Filter settings"]').waitForDisplayed();
      const groups = await h.texts('div[aria-label="Preference groups"] button');
      h.note("preference-groups", groups);
      for (const group of groups) {
        await h.click(`//div[@aria-label="Preference groups"]/button[normalize-space()="${group}"]`);
        await browser.pause(500);
        await h.shoot(`preferences-${group.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`);
      }
      await h.setValueInPage('input[aria-label="Filter settings"]', "font");
      await h.shoot("preferences-filter-font");
      await h.setValueInPage('input[aria-label="Filter settings"]', "qqqzzz");
      await h.shoot("preferences-filter-no-match");
    });
    it("preferences-dark", async () => {
      await h.setTheme("dark");
      await h.click('button[aria-label="Preferences"]');
      await $('input[aria-label="Filter settings"]').waitForDisplayed();
      await h.click('//div[@aria-label="Preference groups"]/button[normalize-space()="Appearance"]');
      await h.shoot("preferences-appearance-dark");
      await h.esc();
      await h.setTheme("light");
    });
    it("preferences-narrow", async () => {
      await h.size(900, 700);
      await h.click('button[aria-label="Preferences"]');
      await $('input[aria-label="Filter settings"]').waitForDisplayed();
      await h.shoot("preferences-narrow-900x700");
      await h.esc();
      await h.size(1280, 800);
    });
    it("dialog-keyboard-shortcuts", async () => {
      await dialogShot("dialog-keyboard-shortcuts", () => h.runFromPalette("Show keyboard shortcuts"));
    });
    it("dialog-operation-log-empty", async () => {
      await dialogShot("dialog-operation-log", () => h.runFromPalette("Show operation log"));
    });
    it("dialog-whats-new", async () => {
      await dialogShot("dialog-whats-new", () => h.click('button[aria-label="What’s new"]'));
    });
    it("dialog-third-party-licences", async () => {
      await dialogShot("dialog-third-party-licences", () => h.runFromPalette("Third-party licences"));
    });
    it("dialog-clone", async () => {
      await dialogShot("dialog-clone", () => h.runFromPalette("Clone repository…"));
    });
    it("dialog-clone-filled", async () => {
      await h.runFromPalette("Clone repository…");
      await browser.pause(500);
      const inputs = await $$("dialog[open] input[type='text'], dialog[open] input:not([type])");
      if (inputs.length > 0) await inputs[0].setValue("https://example.com/team/project.git");
      await browser.pause(400);
      await h.shoot("dialog-clone-filled");
    });
    it("dialog-rename-tab", async () => {
      await dialogShot("dialog-rename-tab", () => h.runFromPalette("Rename tab…"));
    });
    it("dialog-go-to-commit", async () => {
      await dialogShot("palette-go-to-commit", () => h.runFromPalette("Go to commit…"));
    });
    it("dialog-switch-branch", async () => {
      await dialogShot("palette-switch-branch", () => h.runFromPalette("Switch branch…"));
    });
    it("search-commits", async () => {
      await h.runFromPalette("Search commits");
      await browser.pause(400);
      await browser.keys("tile".split(""));
      await browser.pause(1200);
      await h.shoot("search-commits-tile", { keepFocus: true });
    });
  });

  phase("D", "dialogs from menus (opened and cancelled)", () => {
    const fromCommit = (name: string, subject: string, item: string) =>
      it(name, async () => {
        await dialogShot(name, async () => {
          await commitMenu(subject);
          await h.pick(item, { prefix: true });
        });
      });
    const fromBranch = (name: string, branch: string, item: string) =>
      it(name, async () => {
        await dialogShot(name, async () => {
          await branchMenu(branch);
          await h.pick(item, { prefix: true });
        });
      });
    fromCommit("dialog-reset", COMMIT_OLD, "Reset main to here");
    fromCommit("dialog-create-branch-from-commit", COMMIT_OLD, "Branch from here");
    fromCommit("dialog-create-tag", COMMIT_OLD, "Create tag");
    fromCommit("error-cherry-pick-with-staged-changes", COMMIT_OFFLINE, "Cherry-pick");
    fromCommit("dialog-reword", COMMIT_HEAD, "Reword");
    fromCommit("dialog-rebase-planner-onto-commit", COMMIT_OFFLINE, "Rebase current branch onto here");
    fromBranch("dialog-merge", "feature/offline-tiles", "Merge");
    fromBranch("dialog-rebase-onto-branch", "feature/offline-tiles", "Rebase current branch onto");
    fromBranch("dialog-delete-branch", "feature/offline-tiles", "Delete");
    fromBranch("dialog-rename-branch", "feature/offline-tiles", "Rename");
    fromBranch("dialog-push-branch", "fix/search-label", "Push");
    fromBranch("dialog-set-upstream", "main", "Set upstream");
    it("menu-branches-add", async () => {
      await h.expandSidebarSection("Branches");
      await h.click('[data-active-repo="true"] section[aria-label="Branches"] button[aria-label="Add"]');
      await browser.pause(400);
      h.note("menu-branches-add", await h.menu());
      await h.shoot("menu-branches-add");
    });
    it("sidebar-new-branch-form", async () => {
      await h.click('[data-active-repo="true"] section[aria-label="Branches"] button[aria-label="Add"]');
      await h.pick("New branch", { prefix: true });
      await browser.pause(500);
      await h.shoot("sidebar-new-branch-form");
    });
    it("dialog-delete-tag", async () => {
      await dialogShot("dialog-delete-tag", async () => {
        await h.expandSidebarSection("Tags");
        await h.click('button[aria-label="Actions for v0.1.0"]');
        await h.pick("Delete tag", { prefix: true });
      });
    });
    it("dialog-stash-options", async () => {
      await dialogShot("dialog-stash", async () => {
        await h.click('button[aria-label="More stash options"]');
        await h.pick("Stash…");
      });
    });
    it("dialog-stash-drop", async () => {
      await dialogShot("dialog-stash-drop", async () => {
        await h.expandSidebarSection("Stashes");
        await h.ctx("span*=Try a closer default zoom");
        await h.pick("Drop", { prefix: true });
      });
    });
    it("dialog-add-remote", async () => {
      await dialogShot("sidebar-add-remote-form", async () => {
        await h.click('[data-active-repo="true"] section[aria-label="Branches"] button[aria-label="Add"]');
        await h.pick("Add remote", { prefix: true });
      });
    });
    it("dialog-edit-remote", async () => {
      await dialogShot("dialog-edit-remote", async () => {
        await h.click('button[aria-label="Actions for remote origin"]');
        await h.pick("Edit remote", { prefix: true });
      });
    });
    it("dialog-remove-remote", async () => {
      await dialogShot("dialog-remove-remote", async () => {
        await h.click('button[aria-label="Actions for remote origin"]');
        await h.pick("Remove remote", { prefix: true });
      });
    });
    it("dialog-manage-credentials", async () => {
      await dialogShot("dialog-manage-credentials", async () => {
        await h.click('button[aria-label="Actions for remote origin"]');
        await h.pick("Manage credentials", { prefix: true });
      });
    });
    it("dialog-discard-file", async () => {
      await dialogShot("dialog-discard-file", async () => {
        await selectWorkingTree();
        await h.ctx("//span[contains(., 'src/app.js')]");
        await h.pick("Discard", { prefix: true });
      });
    });
    it("dialog-delete-untracked", async () => {
      await dialogShot("dialog-delete-untracked", () => h.runFromPalette("Delete untracked files…"));
    });
    it("dialog-create-tag-at-head", async () => {
      await dialogShot("dialog-create-tag-at-head", () => h.runFromPalette("Create tag at HEAD…"));
    });
    it("dialog-create-branch-palette", async () => {
      await dialogShot("dialog-create-branch", () => h.runFromPalette("Create branch…"));
    });
    it("dialog-force-push", async () => {
      await dialogShot("dialog-force-push", () => h.runFromPalette("Force push current branch…"));
    });
    it("dialog-delete-remote-branch", async () => {
      await dialogShot("dialog-delete-remote-branch", () => h.runFromPalette("Delete remote branch…"));
    });
    it("dialog-merge-dark", async () => {
      await h.setTheme("dark");
      await dialogShot("dialog-merge-dark", async () => {
        await branchMenu("feature/offline-tiles");
        await h.pick("Merge", { prefix: true });
      });
      await h.esc();
      await h.setTheme("light");
    });
    it("dialog-reset-dark", async () => {
      await h.setTheme("dark");
      await dialogShot("dialog-reset-dark", async () => {
        await commitMenu(COMMIT_OLD);
        await h.pick("Reset main to here", { prefix: true });
      });
      await h.esc();
      await h.setTheme("light");
    });
    it("dialog-narrow", async () => {
      await h.size(900, 700);
      await dialogShot("dialog-merge-narrow-900x700", async () => {
        await branchMenu("feature/offline-tiles");
        await h.pick("Merge", { prefix: true });
      });
      await h.esc();
      await h.size(1280, 800);
    });
  });

  phase("E", "keyboard focus", () => {
    before(async () => {
      // Xvfb has no window manager, so the app window is never "active" and WebKit paints no
      // focus ring. Give it X input focus directly.
      try {
        const ids = execFileSync("xdotool", ["search", "--onlyvisible", "--name", "Browsitory"], { encoding: "utf8" })
          .split("\n")
          .filter(Boolean);
        for (const id of ids) execFileSync("xdotool", ["windowfocus", "--sync", id]);
        h.note("xdotool-windows", ids);
      } catch (error) {
        h.note("xdotool-fail", String(error));
      }
      await browser.pause(500);
      h.note("has-focus", await browser.execute(() => document.hasFocus()));
    });
    it("focus-toolbar", async () => {
      await browser.execute(() => (document.activeElement as HTMLElement | null)?.blur());
      await h.click('[data-active-repo="true"] button[aria-label="Sidebar"]');
      await h.click('[data-active-repo="true"] button[aria-label="Sidebar"]');
      for (let i = 0; i < 2; i++) await browser.keys(["Tab"]);
      h.note(
        "focus-visible",
        await browser.execute(() => [document.activeElement?.matches(":focus-visible"), document.hasFocus()]),
      );
      await h.shoot("focus-toolbar-1", { keepFocus: true });
      for (let i = 0; i < 3; i++) await browser.keys(["Tab"]);
      h.note("focus-toolbar", await browser.execute(() => document.activeElement?.outerHTML.slice(0, 200)));
      await h.shoot("focus-toolbar-2", { keepFocus: true });
    });
    it("focus-graph-row", async () => {
      await h.click(`li*=${COMMIT_OLD}`);
      await browser.keys(["ArrowDown"]);
      await h.shoot("focus-commit-list-row", { keepFocus: true });
    });
    it("focus-dialog", async () => {
      await commitMenu(COMMIT_OLD);
      await h.pick("Reset main to here", { prefix: true });
      await browser.pause(500);
      await browser.keys(["Tab"]);
      await h.shoot("focus-dialog-reset-1", { keepFocus: true });
      await browser.keys(["Tab"]);
      await browser.keys(["Tab"]);
      await h.shoot("focus-dialog-reset-2", { keepFocus: true });
    });
    it("focus-context-menu-keyboard", async () => {
      await h.click(`li*=${COMMIT_OLD}`);
      await browser.keys(["Shift", "F10"]);
      await browser.pause(400);
      await browser.keys(["ArrowDown"]);
      await browser.keys(["ArrowDown"]);
      await h.shoot("focus-context-menu-keyboard", { keepFocus: true });
    });
  });

  phase("F", "remotes on disk: fetch, push, pull, errors, hooks", () => {
    const ORIGIN = path.join(SCREENSHOT_ROOT, "origin.git");
    const CLONE = path.join(SCREENSHOT_ROOT, "other-clone");
    before(async () => {
      // Point origin at a local bare repository (never the network), one commit ahead of main.
      fs.rmSync(ORIGIN, { recursive: true, force: true });
      fs.rmSync(CLONE, { recursive: true, force: true });
      execFileSync("git", ["init", "-q", "--bare", "-b", "main", ORIGIN]);
      for (const ref of ["main", "feature/dark-mode", "feature/offline-tiles"]) {
        h.git("push", "-q", ORIGIN, `refs/remotes/origin/${ref}:refs/heads/${ref}`);
      }
      execFileSync("git", ["clone", "-q", ORIGIN, CLONE]);
      fs.writeFileSync(path.join(CLONE, "CONTRIBUTING.md"), "Open an issue first.\n");
      h.gitIn(CLONE, "add", ".");
      h.gitIn(CLONE, "commit", "-q", "-m", "Add contributing notes");
      h.gitIn(CLONE, "push", "-q", "origin", "main");
      h.git("remote", "set-url", "origin", ORIGIN);
      await h.refresh();
    });
    it("fetch", async () => {
      await h.click("//div[@aria-label='Sync']//button[normalize-space()='Fetch']");
      await browser.pause(2500);
      await h.shoot("sync-after-fetch-diverged");
    });
    it("push-rejected", async () => {
      await h.clickInPage("button", "Push");
      await browser.pause(3000);
      h.note("push-rejected", await h.dialogs());
      h.note("push-rejected-alerts", await h.texts("[role='alert']"));
      await h.shoot("error-push-rejected");
    });
    it("push-rejected-details", async () => {
      await h.clickInPage("button", "Push");
      await browser.pause(3000);
      try {
        await h.clickInPage("button", "Show details", { timeout: 4000 });
        await browser.pause(800);
        await h.shoot("dialog-operation-log-push-rejected");
      } catch (error) {
        h.note("push-details-fail", String(error));
      }
    });
    it("fetch-error", async () => {
      h.git("remote", "add", "broken", path.join(SCREENSHOT_ROOT, "does-not-exist.git"));
      await h.refresh();
      await h.click('button[aria-label="Actions for remote broken"]');
      h.note("menu-remote-broken", await h.menu());
      await h.pick("Fetch", { prefix: true });
      await browser.pause(2500);
      h.note("fetch-error", await h.texts("[role='alert']"));
      await h.shoot("error-fetch-failed");
      await tidy();
      h.git("remote", "remove", "broken");
      await h.refresh();
    });
    it("pull-dirty", async () => {
      await h.click("//div[@aria-label='Sync']//button[normalize-space()='Pull']");
      await browser.pause(3000);
      h.note("pull-dirty", await h.dialogs());
      h.note("pull-dirty-alerts", await h.texts("[role='alert']"));
      await h.shoot("pull-with-local-changes");
    });
    it("operation-log-with-entries", async () => {
      await tidy();
      await dialogShot("dialog-operation-log-entries", () => h.runFromPalette("Show operation log"));
    });
    it("stash-pull-options", async () => {
      await h.click('button[aria-label="Pull options"]');
      await h.shoot("menu-pull-options-after-fetch");
    });
  });

  phase("G", "conflict and merge editor", () => {
    before(async () => {
      h.demo.startConflict();
      await h.refresh();
      await h.waitUntilInPage("button", "Abort merge", { contains: true });
    });
    it("conflict-overview", async () => {
      await h.shoot("conflict-merge-stopped-overview");
    });
    it("conflict-file", async () => {
      await selectWorkingTree();
      await h.clickInPage("span", "src/search.js (Conflicted)");
      await h.waitUntilInPage("button", "Mark resolved", { contains: true });
      await h.shoot("conflict-file-selected");
    });
    it("conflict-file-menu", async () => {
      h.note("menu-file-conflicted", await h.ctx("span*=src/search.js"));
      await h.shoot("menu-file-conflicted");
    });
    it("merge-editor-three-panes", async () => {
      await h.size(1600, 1000);
      await $('section[aria-label="Conflict 1 of 1"]').waitForExist();
      h.note("merge-editor-buttons", await h.texts("button"));
      await h.shoot("merge-editor-three-panes-1600x1000");
      await h.setTheme("dark");
      await h.shoot("merge-editor-three-panes-dark");
      await h.setTheme("light");
      await h.size(1280, 800);
      await h.shoot("merge-editor-1280x800");
    });
    it("merge-editor-other-layout", async () => {
      const toggles = await h.texts("button[aria-pressed]");
      h.note("merge-editor-toggles", toggles);
      for (const t of toggles) {
        if (t !== "Three panes" && /pane|inline|side|result/i.test(t)) {
          await h.clickInPage("button[aria-pressed]", t);
          await browser.pause(600);
          await h.shoot(`merge-editor-${t.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`);
        }
      }
      await h.clickInPage("button[aria-pressed]", "Three panes").catch(() => undefined);
    });
    it("conflict-abort-confirm", async () => {
      await h.clickInPage("button", "Abort merge");
      await browser.pause(700);
      h.note("abort-merge", await h.dialogs());
      await h.shoot("dialog-abort-merge-confirm");
    });
    after(async () => {
      await tidy();
      try {
        h.git("merge", "--abort");
      } catch {
        // already ended
      }
      await h.refresh();
    });
  });

  phase("H", "commit: hook failure and identity", () => {
    it("hook-failure", async () => {
      const hook = path.join(DEMO_REPO_PATH, ".git", "hooks", "pre-commit");
      fs.mkdirSync(path.dirname(hook), { recursive: true });
      fs.writeFileSync(hook, '#!/bin/sh\necho "lint: 3 problems in src/zoom.js" >&2\nexit 1\n', { mode: 0o755 });
      await h.refresh();
      await selectWorkingTree();
      await h.setValueInPage("textarea[placeholder='Commit message']", "Add zoom controls");
      await h.clickInPage("button", "Commit 1 file", { contains: true });
      await browser.pause(3000);
      h.note("hook-failure", await h.dialogs());
      await h.shoot("dialog-hook-failure");
      await tidy();
      fs.rmSync(hook, { force: true });
      await h.setValueInPage("textarea[placeholder='Commit message']", "");
    });
    it("dialog-identity", async () => {
      h.git("config", "--unset", "user.name");
      h.git("config", "--unset", "user.email");
      await h.refresh();
      await selectWorkingTree();
      h.git("add", "src/zoom.js");
      await h.refresh();
      await h.setValueInPage("textarea[placeholder='Commit message']", "Add zoom controls");
      await h.clickInPage("button", "Commit 1 file", { contains: true });
      await browser.pause(2000);
      h.note("identity", await h.dialogs());
      await h.shoot("dialog-identity");
      await tidy();
      h.git("config", "user.name", "Alex Rivera");
      h.git("config", "user.email", "alex@example.com");
      await h.setValueInPage("textarea[placeholder='Commit message']", "");
    });
  });

  phase("I", "clean tree: rebase planner, edit and split commit, empty states", () => {
    before(async () => {
      h.git("stash", "push", "-q", "--include-untracked", "-m", "Before the history edits");
      await h.refresh();
    });
    it("clean-working-tree", async () => {
      await h.shoot("main-clean-working-tree");
      await selectCommit(COMMIT_HEAD);
      await h.shoot("main-clean-head-selected");
    });
    it("interactive-rebase-planner", async () => {
      await commitMenu(COMMIT_OLD);
      await h.pick("Rebase current branch onto here…");
      await browser.pause(1000);
      h.note("planner", await h.dialogs());
      await h.shoot("dialog-interactive-rebase-planner");
      h.note("planner-buttons", await h.texts("dialog[open] button, [role=dialog] button"));
    });
    it("interactive-rebase-planner-dark", async () => {
      await h.setTheme("dark");
      await commitMenu(COMMIT_OLD);
      await h.pick("Rebase current branch onto here…");
      await browser.pause(1000);
      await h.shoot("dialog-interactive-rebase-planner-dark");
      await h.esc();
      await h.setTheme("light");
    });
    it("cherry-pick-and-revert-clean", async () => {
      const head = h.git("rev-parse", "HEAD");
      await commitMenu(COMMIT_OFFLINE);
      await h.pick("Cherry-pick onto main");
      await browser.pause(2500);
      h.note("cherry-pick-clean", await h.dialogs());
      await h.shoot("after-cherry-pick");
      h.git("reset", "-q", "--hard", head);
      await h.refresh();
      await commitMenu(COMMIT_OLD);
      await h.pick("Revert (new commit on main)");
      await browser.pause(2500);
      h.note("revert-clean", await h.dialogs());
      await h.shoot("after-revert-or-conflict");
      try {
        h.git("revert", "--abort");
      } catch {
        // no revert in progress
      }
      h.git("reset", "-q", "--hard", head);
      await h.refresh();
    });
    it("edit-commit", async () => {
      await commitMenu(COMMIT_HEAD);
      await h.pick("Edit commit…");
      await h.waitUntilInPage("[role='status']", "Editing ", { contains: true, timeout: SLOW_UI_TIMEOUT });
      await h.shoot("edit-commit-stopped");
      await selectWorkingTree();
      await browser.pause(600);
      await h.shoot("edit-commit-working-tree");
      // Edit commit…'s Abort is "Cancel edit" (#994); nothing changed, so it asks nothing (#979).
      await h.clickInPage("button", "Cancel edit");
      await browser.pause(800);
      h.note("abort-rebase", await h.dialogs());
      await h.shoot("toast-aborted-rebase");
      await browser.pause(1500);
    });
    it("split-commit", async () => {
      await commitMenu(COMMIT_OLD);
      await h.shoot("menu-commit-history-edits-blocked-by-merge");
      await h.esc();
      await commitMenu(COMMIT_HEAD);
      await h.pick("Split commit…");
      await browser.pause(2000);
      h.note("split", await h.dialogs());
      await h.shoot("split-commit-stopped");
      await selectWorkingTree();
      await browser.pause(600);
      await h.shoot("split-commit-working-tree");
      await h.clickInPage("button", "Cancel split");
      await browser.pause(1500);
    });
    it("rebase-conflict-progress", async () => {
      // Rebase fix/search-label's change onto main stops on the same conflict as the merge.
      h.git("switch", "-q", "fix/search-label");
      await h.refresh();
      await branchMenu("main");
      h.note("menu-branch-main-from-fix", await h.menu());
      await h.pick("Rebase current branch onto main", { prefix: true });
      await browser.pause(800);
      await h.shoot("dialog-rebase-planner-onto-main");
      await h.clickInPage("dialog[open] button", "Start rebase");
      await browser.pause(2500);
      await h.shoot("rebase-stopped-on-conflict");
      // The app's rebase runs as its own sequence: abort it through the app, not git.
      await h.clickInPage("button", "Abort rebase");
      await browser.pause(2000);
      h.git("switch", "-q", "main");
      await h.refresh();
    });
    it("empty-stashes", async () => {
      const stashes = h.git("stash", "list");
      h.note("stash-list", stashes);
      // Keep the working tree stash for the end: drop only the demo's two older ones? Simpler:
      // save the list as refs, clear, shoot, restore.
      const entries = h
        .git("stash", "list", "--format=%H%x09%gs")
        .split("\n")
        .filter(Boolean)
        .reverse()
        .map((line) => line.split("\t"));
      h.git("stash", "clear");
      await h.refresh();
      await h.expandSidebarSection("Stashes");
      await browser.pause(600);
      await h.shoot("sidebar-stashes-empty");
      for (const [sha, message] of entries) h.git("stash", "store", "-m", message, sha);
      await h.refresh();
    });
    after(async () => {
      try {
        h.git("stash", "pop", "-q");
      } catch (error) {
        h.note("stash-pop-fail", String(error));
      }
      await h.refresh();
    });
  });

  phase("J", "start page and empty repository", () => {
    it("start-page", async () => {
      for (let i = 0; i < 5 && (await $$('[role="tab"]')).length > 0; i++) {
        await h.runFromPalette("Close tab");
        await browser.pause(800);
      }
      await browser.pause(1000);
      await h.shoot("start-page-repository-picker");
      h.note("picker-buttons", await h.texts("button"));
      await h.setTheme("dark");
      await h.shoot("start-page-repository-picker-dark");
      await h.setTheme("light");
      await h.size(900, 700);
      await h.shoot("start-page-narrow-900x700");
      await h.size(1280, 800);
    });
    it("start-page-filter", async () => {
      await h.setValueInPage("input[type='search'], input[placeholder*='ilter']", "trail");
      await h.shoot("start-page-filtered");
      await h.setValueInPage("input[type='search'], input[placeholder*='ilter']", "");
    });
    it("start-page-workspace-editor", async () => {
      await h.clickInPage("button", "New workspace…");
      await browser.pause(800);
      h.note("workspace-editor", await h.dialogs());
      await h.shoot("dialog-new-workspace");
      await h.clickInPage("button", "Cancel");
      await browser.pause(500);
    });
    it("start-page-clone", async () => {
      await h.clickInPage("button", "Clone…");
      await browser.pause(800);
      await h.shoot("dialog-clone-from-start-page");
    });
    it("start-page-focus", async () => {
      await browser.keys(["Tab"]);
      await browser.keys(["Tab"]);
      await h.shoot("focus-start-page", { keepFocus: true });
    });
    it("empty-repository", async () => {
      await h.click(`button*=${EMPTY_REPO_PATH}`);
      await h.shoot("empty-repo-loading", { pause: 50 });
      await browser.pause(2000);
      await h.shoot("empty-repository");
      await h.clickInPage("li", "Uncommitted changes", { contains: true }).catch(() => undefined);
      await h.shoot("empty-repository-working-tree");
    });
    it("loading-demo", async () => {
      await h.click('button[aria-label="Open another repository"]');
      await browser.pause(500);
      await h.shoot("start-page-from-plus-button");
      await h.click(`button*=${DEMO_REPO_PATH}`);
      await h.shoot("demo-loading", { pause: 30 });
      await browser.pause(2500);
      await h.shoot("main-two-tabs");
      await dialogShot("palette-switch-to-tab", () => h.runFromPalette("Switch to tab…"));
      await h.esc();
      await h.click('button[aria-label="List open tabs"]');
      await browser.pause(400);
      h.note("menu-list-open-tabs", await h.menu());
      await h.shoot("menu-list-open-tabs");
    });
    it("missing-folder", async () => {
      await h.click('button[aria-label="Open another repository"]');
      await browser.pause(400);
      await h.clickInPage("button", "moved-away", { contains: true });
      await browser.pause(1500);
      h.note("missing", await h.texts("[role='alert']"));
      await h.shoot("error-open-missing-folder");
    });
  });

  // Run on its own (INVENTORY_PHASES=K): it assumes the demo as built, in its first tab.
  phase("K", "file views, compare, inline rename, pull diverged", () => {
    it("blame", async () => {
      await selectCommit(COMMIT_OLD);
      await h.waitUntilInPage(".diff-line-add");
      await h.clickInPage("button", "Blame");
      await browser.pause(1500);
      await h.shoot("file-blame-view");
      await h.esc();
    });
    it("file-history", async () => {
      await selectCommit(COMMIT_OLD);
      await h.clickInPage("button", "History");
      await browser.pause(1500);
      await h.shoot("file-history");
      await h.esc();
    });
    it("folder-history", async () => {
      await selectCommit(COMMIT_OLD);
      await h.click('button[aria-label^="Actions for src/"]');
      await h.pick("History of folder", { prefix: true });
      await browser.pause(1500);
      await h.shoot("folder-history");
      await h.esc();
    });
    it("all-files-tree", async () => {
      await selectCommit(COMMIT_OLD);
      await h.clickInPage("button", "All files");
      await browser.pause(1200);
      await h.shoot("commit-all-files-tree");
      await h.clickInPage("button", "Changes");
    });
    it("split-diff", async () => {
      await selectCommit(COMMIT_OLD);
      await h.clickInPage("button", "Split");
      await browser.pause(1000);
      await h.shoot("diff-split-view");
      await h.clickInPage("button", "Unified");
    });
    it("staging-tree-view", async () => {
      await selectWorkingTree();
      await h.clickInPage("button", "Tree");
      await browser.pause(800);
      await h.shoot("staging-tree-view");
      await h.clickInPage("button", "Flat");
    });
    it("compare", async () => {
      await branchMenu("feature/offline-tiles");
      await h.pick("Compare with current branch");
      await browser.pause(1500);
      await h.shoot("compare-branch-with-current");
      await h.esc();
    });
    it("isolate-branch", async () => {
      await branchMenu("feature/offline-tiles");
      await h.pick("Isolate branch");
      await browser.pause(1200);
      await h.shoot("graph-isolated-branch");
      await branchMenu("feature/offline-tiles");
      h.note("menu-branch-isolated", await h.menu());
      await h.esc();
      await h.clickInPage("button", "Show all branches").catch(async () => {
        await h.click('[data-active-repo="true"] section[aria-label="Branches"] button[aria-label="Add"]');
        await h.pick("Show all branches");
      });
      await browser.pause(800);
    });
    it("inline-rename-branch", async () => {
      await branchMenu("feature/offline-tiles");
      await h.pick("Rename");
      await browser.pause(600);
      await h.shoot("sidebar-rename-branch-inline", { keepFocus: true });
      await h.esc();
    });
    it("reflog", async () => {
      await h.expandSidebarSection("Reflog");
      await browser.pause(800);
      await h.shoot("sidebar-reflog-open");
    });
    it("pull-diverged", async () => {
      const ORIGIN = path.join(SCREENSHOT_ROOT, "origin.git");
      const CLONE = path.join(SCREENSHOT_ROOT, "other-clone");
      fs.rmSync(ORIGIN, { recursive: true, force: true });
      fs.rmSync(CLONE, { recursive: true, force: true });
      execFileSync("git", ["init", "-q", "--bare", "-b", "main", ORIGIN]);
      for (const ref of ["main", "feature/dark-mode", "feature/offline-tiles"]) {
        h.git("push", "-q", ORIGIN, `refs/remotes/origin/${ref}:refs/heads/${ref}`);
      }
      execFileSync("git", ["clone", "-q", ORIGIN, CLONE]);
      fs.writeFileSync(path.join(CLONE, "CONTRIBUTING.md"), "Open an issue first.\n");
      h.gitIn(CLONE, "add", ".");
      h.gitIn(CLONE, "commit", "-q", "-m", "Add contributing notes");
      h.gitIn(CLONE, "push", "-q", "origin", "main");
      h.git("remote", "set-url", "origin", ORIGIN);
      h.git("stash", "push", "-q", "--include-untracked", "-m", "Before the pull");
      await h.refresh();
      await h.click("//div[@aria-label='Sync']//button[normalize-space()='Fetch']");
      await browser.pause(2500);
      await h.click("//div[@aria-label='Sync']//button[normalize-space()='Pull']");
      await browser.pause(2500);
      h.note("pull-diverged", await h.dialogs());
      await h.shoot("dialog-pull-diverged");
    });
  });
});
