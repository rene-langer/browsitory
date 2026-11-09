import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage } from "../support/dom";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FIXTURE_FILE = "message-history-fixture.txt";
const MESSAGE = "e2e: remembered message\n\nWith a body line.";

const boxValue = () =>
  browser.execute(() => (document.querySelector("textarea[aria-label='Commit message']") as HTMLTextAreaElement).value);

describe("Browsitory commit message history (#443)", () => {
  // Leaves one commit in the shared fixture repo and a clean working tree, like
  // `first-flow.spec.ts`.
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FIXTURE_FILE), "message history\n");
  });

  it("recalls a committed message from Recent messages and undoes the pick with Ctrl+Z", async () => {
    const box = await $("textarea[aria-label='Commit message']");
    await box.waitForExist();
    const stageButton = await $(`button[aria-label="Stage ${FIXTURE_FILE}"]`);
    await stageButton.waitForExist();
    // Transparent until hovered: clicked in the page (see `first-flow.spec.ts`).
    await browser.execute((el) => (el as HTMLElement).scrollIntoView({ block: "center" }), stageButton);
    await browser.execute((el) => (el as HTMLElement).click(), stageButton);
    await browser.execute(
      (el, text) => {
        const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
        setter?.call(el, text);
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      box,
      MESSAGE,
    );
    await clickInPage("button", "Commit");
    await $("li*=e2e: remembered message").waitForExist();
    await browser.waitUntil(async () => (await boxValue()) === "", {
      timeoutMsg: "expected the commit to clear the message box",
    });

    // Keyboard: the menu opens with its first item focused; Enter picks it.
    await clickInPage("button", "Recent messages");
    await $("[role='menu']").waitForExist();
    await browser.waitUntil(
      async () => (await browser.execute(() => document.activeElement?.textContent)) === "e2e: remembered message",
      { timeoutMsg: "expected the newest committed message to be focused first" },
    );
    await browser.keys(["Enter"]);
    await browser.waitUntil(async () => (await boxValue()) === MESSAGE, {
      timeoutMsg: "expected the pick to fill the box with the full message",
    });
    expect(await browser.execute(() => document.activeElement?.getAttribute("aria-label"))).toBe("Commit message");

    // Ctrl+Z in the field brings back what the pick replaced (here: an empty box).
    await browser.keys(["Control", "z"]);
    await browser.waitUntil(async () => (await boxValue()) === "", {
      timeoutMsg: "expected Ctrl+Z to restore the replaced text",
    });
  });
});
