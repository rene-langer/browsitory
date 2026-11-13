import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { waitForAppReady } from "../support/app";

// Rebinding a shortcut (#898, #899, #901): Preferences > Keyboard gives Refresh another key, the
// key works and the toolbar says so, and Reset all brings the built-in key back.

const CONFIG_FILE = path.join(os.tmpdir(), "browsitory-e2e-config", "config.toml");
const REFRESH = "Refresh the repository";

async function openKeyboardPreferences(): Promise<void> {
  const preferences = await $("header button[aria-label='Preferences']");
  await preferences.waitForClickable();
  await preferences.click();
  // Preferences shows one group at a time (#784).
  await (await $("//dialog[@aria-label='Preferences']//button[@role='tab' and normalize-space(.)='Keyboard']")).click();
  await $(`button[aria-label='Add a key for ${REFRESH}']`).waitForDisplayed();
}

/** The keys Refresh's row lists. `textContent` via `browser.execute`: `getText()` reads nothing in a native <dialog>. */
function refreshRowKeys(): Promise<string[]> {
  return browser.execute((description) => {
    const label = Array.from(document.querySelectorAll("dialog li > span")).find((span) => span.textContent === description);
    const row = label?.closest("li");
    return Array.from(row?.querySelectorAll("kbd") ?? [], (kbd) => kbd.textContent ?? "");
  }, REFRESH);
}

function refreshButtonTitle(): Promise<string | null> {
  return browser.execute(
    () => document.querySelector('[data-active-repo="true"] button[aria-label="Refresh"]')?.getAttribute("title") ?? null,
  );
}

async function closePreferences(): Promise<void> {
  await (await $("button=Close")).click();
  await $("dialog[aria-label='Preferences']").waitForExist({ reverse: true });
}

/** config.toml's text, "" before the app wrote it. */
function configText(): string {
  return fs.existsSync(CONFIG_FILE) ? fs.readFileSync(CONFIG_FILE, "utf8") : "";
}

describe("keyboard shortcuts", () => {
  after(() => {
    // Should the test stop half-way: the built-in keys for the specs after this one, which reload
    // the page and so read config.toml again.
    const text = configText();
    if (text.includes("[keybindings]")) fs.writeFileSync(CONFIG_FILE, text.replace(/\[keybindings\]\n(?:[^[].*\n|\n)*/, ""));
  });

  it("rebinds Refresh in Preferences, runs it with the new key, and resets it (#901)", async () => {
    await waitForAppReady();
    expect(await refreshButtonTitle()).toBe("Refresh (F5)");

    await openKeyboardPreferences();
    await (await $(`button[aria-label='Remove F5 from ${REFRESH}']`)).click();
    await (await $(`button[aria-label='Add a key for ${REFRESH}']`)).click();
    const recorder = await $(`input[aria-label='New key for ${REFRESH}']`);
    await browser.waitUntil(() => recorder.isFocused(), { timeoutMsg: "expected Add key to focus the key recorder" });
    await browser.keys(["F4"]);
    await browser.waitUntil(async () => JSON.stringify(await refreshRowKeys()) === JSON.stringify(["F4"]), {
      timeoutMsg: "expected Refresh to list F4 as its only key",
    });
    await closePreferences();
    // Saved with the other preferences (#898).
    await browser.waitUntil(() => /\[keybindings\]\n(?:.*\n)*refresh = \["F4"\]\n/.test(configText()), {
      timeoutMsg: "expected config.toml to keep refresh = [\"F4\"] under [keybindings]",
    });

    // The toolbar names the new key, and the app takes F4 as Refresh (it handles the key).
    await browser.waitUntil(async () => (await refreshButtonTitle()) === "Refresh (F4)", {
      timeoutMsg: "expected the Refresh button to name F4",
    });
    await browser.execute(() => {
      const pressed: string[] = [];
      (window as unknown as { refreshKeys: string[] }).refreshKeys = pressed;
      // Added after the app's listener on the window, so it sees whether the app used the key.
      window.addEventListener("keydown", (event) => {
        if (event.key === "F4") pressed.push(`${event.key}:${event.defaultPrevented}`);
      });
      (document.activeElement as HTMLElement | null)?.blur();
    });
    await browser.keys(["F4"]);
    await browser.waitUntil(
      async () =>
        (await browser.execute(() => (window as unknown as { refreshKeys: string[] }).refreshKeys)).includes("F4:true"),
      { timeoutMsg: "expected the app to run Refresh on F4" },
    );

    await openKeyboardPreferences();
    await (await $("button=Reset all shortcuts")).click();
    await browser.waitUntil(async () => JSON.stringify(await refreshRowKeys()) === JSON.stringify(["F5"]), {
      timeoutMsg: "expected Reset all to bring F5 back",
    });
    await closePreferences();
    expect(await refreshButtonTitle()).toBe("Refresh (F5)");
    // The built-in keys again: no [keybindings] left for the specs after this one.
    await browser.waitUntil(() => !configText().includes("[keybindings]"), {
      timeoutMsg: "expected Reset all to leave no [keybindings] in config.toml",
    });
  });
});
