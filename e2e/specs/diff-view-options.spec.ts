import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, waitUntilInPage } from "../support/dom";
import { gitIn } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
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

    await clickInPage("button", "Wrap lines");
    await browser.waitUntil(async () => (await state()).wrap === "true", { timeout: 5000 });
    const wrapped = await state();
    expect(wrapped.whiteSpace).toBe("pre-wrap");
    expect(wrapped.scrolls).toBe(false);
    expect(wrapped.lineNumber).toBe("5");
    expect(wrapped.height).toBeGreaterThan(unwrapped.height);
  });

  it("shows the whole file and stages one selected change of it", async () => {
    await clickInPage("button", "Show whole file");
    // l20 is 15 lines from either change: only the whole file shows it.
    await waitUntilInPage(".diff-line-context", " l20", { contains: true });
    const hunkHeaders = await browser.execute(
      () => [...document.querySelectorAll("span")].filter((el) => el.textContent!.startsWith("@@ ")).length,
    );
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
  });
});
