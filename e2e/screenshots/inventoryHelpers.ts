import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { clickInPage, setValueInPage, waitUntilGoneInPage, waitUntilInPage } from "../support/dom";
import { expandSidebarSection, sidebarBranchRow, sidebarRemoteFolder, sidebarSectionToggle } from "../support/sidebar";
import * as demo from "./demoRepo";

// Helpers for the UX-review screenshot inventory (see capture.inventory.ts).

export const SHOTS =
  process.env.INVENTORY_SHOTS ?? path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "inventory-out", "shots");
const NOTES = path.join(SHOTS, "..", "notes.jsonl");

/** Appends a probe result (menu items, dialog labels) to notes.jsonl next to the shots. */
export function note(key: string, value: unknown): void {
  fs.mkdirSync(path.dirname(NOTES), { recursive: true });
  fs.appendFileSync(NOTES, JSON.stringify({ key, value }) + "\n");
}

export async function click(selector: string): Promise<void> {
  await browser.waitUntil(async () => (await $$(selector)).length > 0, { timeoutMsg: `no ${selector}` });
  const element = (await $$(selector))[0];
  await browser.execute((el) => (el as HTMLElement).click(), element);
}

/** Saves a shot. `keepFocus` keeps the focused element (for focus-ring shots). */
export async function shoot(name: string, { keepFocus = false, pause = 600 } = {}): Promise<string> {
  fs.mkdirSync(SHOTS, { recursive: true });
  if (!keepFocus) await browser.execute(() => (document.activeElement as HTMLElement | null)?.blur());
  await browser.pause(pause);
  const file = path.join(SHOTS, `${name}.png`);
  await browser.saveScreenshot(file);
  return file;
}

export async function setTheme(theme: "light" | "dark"): Promise<void> {
  const toggle = await $(`button[aria-label="Switch to ${theme} theme"]`);
  if (await toggle.isExisting()) await browser.execute((el) => (el as HTMLElement).click(), toggle);
  await browser.waitUntil(async () => (await browser.execute(() => document.documentElement.dataset.theme)) === theme, {
    timeout: 5000,
  });
}

/** Opens a context menu on the element `selector` (any WebdriverIO selector) and returns its items. */
export async function ctx(selector: string): Promise<string[]> {
  await browser.waitUntil(async () => (await $$(selector)).length > 0, { timeoutMsg: `no ${selector}` });
  const element = (await $$(selector))[0];
  await browser.execute((el) => {
    const r = (el as HTMLElement).getBoundingClientRect();
    el.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        clientX: Math.round(r.left + Math.min(40, r.width / 2)),
        clientY: Math.round(r.top + r.height / 2),
      }),
    );
  }, element);
  await browser.pause(400);
  return menu();
}

/** Texts of the open menu's items. */
export async function menu(): Promise<string[]> {
  return browser.execute(() =>
    Array.from(document.querySelectorAll('[role="menu"] [role^="menuitem"], [role="menu"] button')).map(
      (e) =>
        (e.textContent ?? "").replace(/\s+/g, " ").trim() +
        ((e as HTMLButtonElement).disabled || e.getAttribute("aria-disabled") === "true" ? " [disabled]" : ""),
    ),
  );
}

/** Clicks the open menu's item whose text is `label` (exact, or prefix with `prefix`). */
export async function pick(label: string, { prefix = false } = {}): Promise<void> {
  await browser.waitUntil(
    () =>
      browser.execute(
        (wanted: string, isPrefix: boolean) => {
          const item = Array.from(
            document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"], [role="menu"] button'),
          ).find((e) => {
            const t = (e.textContent ?? "").replace(/\s+/g, " ").trim();
            return isPrefix ? t.startsWith(wanted) : t === wanted;
          });
          if (!item) return false;
          item.click();
          return true;
        },
        label,
        prefix,
      ),
    { timeout: 5000, timeoutMsg: `no menu item ${label}` },
  );
  await browser.pause(500);
}

/** Labels of open dialogs. */
export async function dialogs(): Promise<string[]> {
  return browser.execute(() =>
    Array.from(document.querySelectorAll("dialog[open], [role=dialog], [role=alertdialog]")).map(
      (e) => `${e.tagName} ${e.getAttribute("aria-label") ?? ""} | ${(e.textContent ?? "").slice(0, 100).replace(/\s+/g, " ")}`,
    ),
  );
}

export async function esc(times = 1): Promise<void> {
  for (let i = 0; i < times; i++) {
    await browser.keys(["Escape"]);
    await browser.pause(200);
  }
}

/** Visible texts (or attribute values) of elements matching a CSS selector, for probing. */
export async function texts(selector: string, attr?: string): Promise<string[]> {
  return browser.execute(
    (sel: string, a: string | null) =>
      Array.from(document.querySelectorAll(sel))
        .filter((e) => e.getClientRects().length > 0)
        .map((e) => (a ? (e.getAttribute(a) ?? "") : (e.textContent ?? "").replace(/\s+/g, " ").trim()).slice(0, 120)),
    selector,
    attr ?? null,
  );
}

export async function openPalette(query = ""): Promise<void> {
  await browser.keys(["Control", "k"]);
  const input = await $('input[aria-label="Command palette"]');
  await input.waitForDisplayed();
  if (query) await input.setValue(query);
  await browser.pause(300);
}

export async function runFromPalette(label: string): Promise<void> {
  await openPalette(label);
  await waitUntilInPage("li", label, { contains: true });
  await browser.keys(["Enter"]);
  await $('input[aria-label="Command palette"]').waitForDisplayed({ reverse: true });
  await browser.pause(400);
}

export function git(...args: string[]): string {
  return gitIn(demo.DEMO_REPO_PATH, ...args);
}

export function gitIn(cwd: string, ...args: string[]): string {
  return execFileSync("git", args, {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
      GIT_AUTHOR_NAME: "Alex Rivera",
      GIT_AUTHOR_EMAIL: "alex@example.com",
      GIT_COMMITTER_NAME: "Alex Rivera",
      GIT_COMMITTER_EMAIL: "alex@example.com",
    },
  }).trim();
}

export async function size(width: number, height: number): Promise<void> {
  await browser.setWindowSize(width, height);
  await browser.pause(400);
}

export async function refresh(): Promise<void> {
  await runFromPalette("Refresh");
  await browser.pause(800);
}

export {
  clickInPage,
  setValueInPage,
  waitUntilGoneInPage,
  waitUntilInPage,
  expandSidebarSection,
  sidebarBranchRow,
  sidebarRemoteFolder,
  sidebarSectionToggle,
  demo,
};
