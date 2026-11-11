import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { browser } from "@wdio/globals";

// Failure diagnostics for flakes that don't reproduce locally (#810). CI uploads this folder as
// the `e2e-failure-screenshots` artifact when the e2e job fails.
export const FAILURE_DIR = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "failure-screenshots");

/**
 * The commit box's state as the page has it: every commit message field (value, whether it is
 * shown) and every Commit button (enabled, label, the reason shown under it). More than one of
 * either means a hidden tab's workspace or a stale copy is in the page too.
 */
export async function commitBoxState(): Promise<unknown> {
  return browser.execute(() => {
    const shown = (element: Element) => element.getClientRects().length > 0;
    return {
      fields: Array.from(document.querySelectorAll("textarea[placeholder='Commit message']")).map((element) => ({
        value: (element as HTMLTextAreaElement).value,
        shown: shown(element),
      })),
      buttons: Array.from(document.querySelectorAll('button[data-action="commit"]')).map((element) => {
        const describedBy = element.getAttribute("aria-describedby");
        return {
          label: (element.textContent ?? "").trim(),
          disabled: (element as HTMLButtonElement).disabled,
          shown: shown(element),
          reason: describedBy === null ? null : (document.getElementById(describedBy)?.textContent ?? null),
        };
      }),
      tabs: Array.from(document.querySelectorAll('[role="tab"]')).map((element) => (element.textContent ?? "").trim()),
    };
  });
}

/**
 * Runs `step`; when it throws, saves a screenshot as `<name>.png` in `FAILURE_DIR` and rethrows
 * with `describe()`'s result (the page state at the failure) added to the message.
 */
export async function withFailureDiagnostics<T>(
  name: string,
  step: () => Promise<T>,
  describe: () => Promise<unknown> = commitBoxState,
): Promise<T> {
  try {
    return await step();
  } catch (error) {
    const notes: string[] = [];
    try {
      notes.push(`page state: ${JSON.stringify(await describe())}`);
    } catch (stateError) {
      notes.push(`page state unavailable: ${String(stateError)}`);
    }
    try {
      fs.mkdirSync(FAILURE_DIR, { recursive: true });
      const file = path.join(FAILURE_DIR, `${name}.png`);
      await browser.saveScreenshot(file);
      notes.push(`screenshot: ${file}`);
    } catch (screenshotError) {
      notes.push(`no screenshot: ${String(screenshotError)}`);
    }
    if (error instanceof Error) {
      error.message = `${error.message}\n${notes.join("\n")}`;
      throw error;
    }
    throw new Error(`${String(error)}\n${notes.join("\n")}`);
  }
}
