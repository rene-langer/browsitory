import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, waitUntilInPage } from "../support/dom";
import { gitIn } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
// The suite's config.toml (`wdio.conf.ts`, BROWSITORY_CONFIG_DIR), shared by every spec.
const CONFIG_FILE = path.join(os.tmpdir(), "browsitory-e2e-config", "config.toml");
const FILE = "wrap-fixture.rs";
// Long, but under the 300 characters at which word wrap Auto wraps code anyway.
const LONG = `let long = ${"x".repeat(200)};`;

const lines = (changed: Record<number, string>) =>
  Array.from({ length: 60 }, (_, i) => changed[i + 1] ?? `l${i + 1}`).join("\n") + "\n";

// Untrimmed: the index blob is compared with the whole file, trailing newline included.
const git = gitIn(E2E_REPO_PATH, { trim: false });

describe("Browsitory diff word wrap and whole file", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), lines({}));
    git("add", FILE);
    git("commit", "-q", "-m", "e2e: seed wrap fixture");
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), lines({ 5: LONG, 45: "changed 45" }));
  });

  after(() => {
    git("reset", "-q", "--", FILE);
    git("checkout", "--", FILE);
    // Back to the built-in look for the specs after this one.
    if (fs.existsSync(CONFIG_FILE)) {
      const text = fs.readFileSync(CONFIG_FILE, "utf8");
      fs.writeFileSync(CONFIG_FILE, text.replace(/\[appearance\]\n(?:[^[\n][^\n]*\n)*/, ""));
    }
  });

  it("wraps long lines on demand and keeps their line numbers", async () => {
    const row = await $(`span=${FILE} (Modified)`);
    await row.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), row);
    await waitUntilInPage(".diff-line-add");

    // Word wrap Auto leaves code unwrapped in the unified view: the long line scrolls sideways.
    const state = () =>
      browser.execute(() => {
        const diff = document.querySelector<HTMLElement>("[data-wrap]")!;
        const added = [...diff.querySelectorAll<HTMLElement>(".diff-line-add")].find((el) =>
          el.textContent!.includes("let long"),
        )!;
        const content = added.lastElementChild as HTMLElement;
        const lines = added.parentElement!;
        return {
          wrap: diff.dataset.wrap,
          whiteSpace: getComputedStyle(content).whiteSpace,
          scrolls: lines.scrollWidth > lines.clientWidth,
          lineNumber: (added.children[1] as HTMLElement).textContent,
          height: added.getBoundingClientRect().height,
        };
      });
    const unwrapped = await state();
    expect(unwrapped.wrap).toBe("false");
    expect(unwrapped.whiteSpace).toBe("pre");
    expect(unwrapped.scrolls).toBe(true);
    expect(unwrapped.lineNumber).toBe("5");

    await clickInPage(`button[aria-label="Wrap lines"]`);
    await browser.waitUntil(async () => (await state()).wrap === "true", { timeout: 5000 });
    const wrapped = await state();
    expect(wrapped.whiteSpace).toBe("pre-wrap");
    expect(wrapped.scrolls).toBe(false);
    expect(wrapped.lineNumber).toBe("5");
    expect(wrapped.height).toBeGreaterThan(unwrapped.height);

    // The toolbar's Wrap lines persists as the setting (#640): On, then back to Auto (only
    // Preferences offers it) for the specs after this one.
    await browser.waitUntil(() => /word_wrap = "on"/.test(fs.readFileSync(CONFIG_FILE, "utf8")), {
      timeoutMsg: "Wrap lines did not reach config.toml",
    });
    const preferences = await $("header button[aria-label='Preferences']");
    await preferences.waitForClickable();
    await preferences.click();
    // Preferences shows one group at a time (#784).
    await (await $("//dialog[@aria-label='Preferences']//button[@role='tab' and normalize-space(.)='Diff']")).click();
    const wordWrap = await $("//select[@id=//label[normalize-space(.)='Word wrap']/@for]");
    await wordWrap.waitForExist();
    await wordWrap.selectByVisibleText("Auto");
    await browser.waitUntil(() => /word_wrap = "auto"/.test(fs.readFileSync(CONFIG_FILE, "utf8")), {
      timeoutMsg: "Word wrap Auto did not reach config.toml",
    });
    await (await $("button=Close")).click();
    await browser.waitUntil(async () => (await state()).wrap === "false", { timeout: 5000 });
  });

  it("applies the code font size from Preferences to the diff at once (#615)", async () => {
    const row = await $(`span=${FILE} (Modified)`);
    await row.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), row);
    await waitUntilInPage(".diff-line-add");
    const fontSize = () =>
      browser.execute(() => getComputedStyle(document.querySelector<HTMLElement>(".diff-line-add")!).fontSize);
    expect(await fontSize()).toBe("12.5px");

    const preferences = await $("header button[aria-label='Preferences']");
    await preferences.waitForClickable();
    await preferences.click();
    // Preferences shows one group at a time (#784).
    await (await $("//dialog[@aria-label='Preferences']//button[@role='tab' and normalize-space(.)='Appearance']")).click();
    const size = await $("//input[@id=//label[normalize-space(.)='Code font size (px)']/@for]");
    await size.waitForExist();
    expect(await size.getValue()).toBe("");
    await size.setValue("18");
    await browser.keys("Enter");

    await browser.waitUntil(async () => (await fontSize()) === "18px", {
      timeoutMsg: "the diff did not take the new code font size",
    });
    await browser.waitUntil(() => /\[appearance\]\n(?:.*\n)*code_font_size = 18\n/.test(fs.readFileSync(CONFIG_FILE, "utf8")), {
      timeoutMsg: "the code font size did not reach config.toml",
    });
    await (await $("button=Close")).click();
  });

  it("shows whitespace markers without changing the text or the line widths (#616)", async () => {
    const row = await $(`span=${FILE} (Modified)`);
    await row.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), row);
    await waitUntilInPage(".diff-line-add");
    const state = () =>
      browser.execute(() => {
        const added = [...document.querySelectorAll<HTMLElement>(".diff-line-add")].find((el) =>
          el.textContent!.includes("let long"),
        )!;
        const content = added.lastElementChild as HTMLElement;
        const space = content.querySelector<HTMLElement>("[data-whitespace='space']");
        return {
          text: content.textContent,
          width: content.scrollWidth,
          markers: content.querySelectorAll("[data-whitespace]").length,
          marker: space === null ? null : getComputedStyle(space, "::before").content,
        };
      });
    const before = await state();
    expect(before.markers).toBe(0);

    await clickInPage(`button[aria-label="Show whitespace"]`);
    await browser.waitUntil(async () => (await state()).markers > 0, { timeoutMsg: "no whitespace markers" });
    const shown = await state();
    expect(shown.marker).toBe('"·"');
    expect(shown.text).toBe(before.text);
    expect(shown.width).toBe(before.width);
    await browser.waitUntil(() => /show_whitespace = true/.test(fs.readFileSync(CONFIG_FILE, "utf8")), {
      timeoutMsg: "show whitespace did not reach config.toml",
    });

    // Off again, for the specs after this one.
    await clickInPage(`button[aria-label="Show whitespace"]`);
    await browser.waitUntil(async () => (await state()).markers === 0, { timeoutMsg: "markers stayed" });
    await browser.waitUntil(() => /show_whitespace = false/.test(fs.readFileSync(CONFIG_FILE, "utf8")));
  });

  it("shows the whole file and stages one selected change of it", async () => {
    await clickInPage(`button[aria-label="Show whole file"]`);
    // l20 is 15 lines from either change: only the whole file shows it.
    await waitUntilInPage(".diff-line-context", " l20", { contains: true });
    // By class: other spans (commit summaries, line labels) can start with "Lines 3" too.
    const hunkHeaders = await browser.execute(() => document.querySelectorAll(".diff-hunk-range").length);
    expect(hunkHeaders).toBe(1);

    // Select line 45's removal and its replacement.
    await browser.execute(() => {
      const content = (el: HTMLElement) => (el.lastElementChild as HTMLElement).textContent!.trim();
      const removed = [...document.querySelectorAll<HTMLElement>(".diff-line-remove")].find((el) => content(el) === "l45")!;
      const added = [...document.querySelectorAll<HTMLElement>(".diff-line-add")].find((el) => content(el) === "changed 45")!;
      removed.click();
      added.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true }));
    });
    await clickInPage("button", "Stage lines (2)");
    await browser
      .waitUntil(() => git("show", `:${FILE}`) === lines({ 45: "changed 45" }))
      .catch(() => {});
    expect(git("show", `:${FILE}`)).toBe(lines({ 45: "changed 45" }));

    // "Show whole file" is the pane's and persists (#640): off again for the specs after this one.
    await clickInPage(`button[aria-label="Show whole file"]`);
    await waitUntilInPage('button[aria-label="Show whole file"][aria-pressed="false"]');
  });
});
