import { browser } from "@wdio/globals";

// In-page waits and clicks for elements that unmount (#182).
//
// WebdriverIO's element commands find an element and then act on it in separate WebDriver
// round trips. When React unmounts the element (a row that disappears after a mutation, a menu
// that closes), WebKitWebDriver failed with "Stale element found when trying to create the node
// handle" — even inside `waitForExist({ reverse: true })`, whose whole point is to wait for that
// unmount — and kept failing on every retry until the wait timed out, so it wasn't a single
// unlucky round trip that a retry would absorb. These helpers query and act inside one
// `browser.execute`, so no element reference outlives the call.
//
// `clickInPage` only clicks an element that is enabled (no `disabled`, no
// `aria-disabled="true"`) and rendered (has client rects); otherwise it keeps polling.
//
// `selector` is a CSS selector. `text`, when given, matches the element's text with whitespace
// collapsed and trimmed: exactly (like WebdriverIO's `tag=text`) or, with `contains`, as a
// substring (like `tag*=text`).

export interface InPageOptions {
  /** Match `text` as a substring instead of the whole text. */
  contains?: boolean;
  timeout?: number;
}

function describeTarget(selector: string, text: string | undefined, contains: boolean): string {
  if (text === undefined) return selector;
  return `${selector} ${contains ? "containing" : "with text"} "${text}"`;
}

/** Waits until no element matches `selector` (and `text`). */
export async function waitUntilGoneInPage(
  selector: string,
  text?: string,
  { contains = false, timeout = 10000 }: InPageOptions = {},
): Promise<void> {
  await browser.waitUntil(
    () =>
      browser.execute(
        (sel: string, wanted: string | null, partial: boolean) => {
          const normalize = (value: string | null) => (value ?? "").replace(/\s+/g, " ").trim();
          return !Array.from(document.querySelectorAll(sel)).some((element) => {
            if (wanted === null) return true;
            const actual = normalize(element.textContent);
            return partial ? actual.includes(wanted) : actual === wanted;
          });
        },
        selector,
        text ?? null,
        contains,
      ),
    { timeout, timeoutMsg: `${describeTarget(selector, text, contains)} is still in the page` },
  );
}

/**
 * Waits until an element matches `selector` (and `text`). For text with quotes, which
 * WebdriverIO's `tag*=text` selectors don't handle reliably.
 */
export async function waitUntilInPage(
  selector: string,
  text?: string,
  { contains = false, timeout = 10000 }: InPageOptions = {},
): Promise<void> {
  await browser.waitUntil(
    () =>
      browser.execute(
        (sel: string, wanted: string | null, partial: boolean) => {
          const normalize = (value: string | null) => (value ?? "").replace(/\s+/g, " ").trim();
          return Array.from(document.querySelectorAll(sel)).some((element) => {
            if (wanted === null) return true;
            const actual = normalize(element.textContent);
            return partial ? actual.includes(wanted) : actual === wanted;
          });
        },
        selector,
        text ?? null,
        contains,
      ),
    { timeout, timeoutMsg: `${describeTarget(selector, text, contains)} never appeared in the page` },
  );
}

/** Waits for the first element matching `selector` (and `text`) and clicks it, in one step. */
export async function clickInPage(
  selector: string,
  text?: string,
  { contains = false, timeout = 10000 }: InPageOptions = {},
): Promise<void> {
  await browser.waitUntil(
    () =>
      browser.execute(
        (sel: string, wanted: string | null, partial: boolean) => {
          const normalize = (value: string | null) => (value ?? "").replace(/\s+/g, " ").trim();
          const target = Array.from(document.querySelectorAll(sel)).find((element) => {
            if ((element as HTMLButtonElement).disabled === true) return false;
            if (element.getAttribute("aria-disabled") === "true") return false;
            if (element.getClientRects().length === 0) return false;
            if (wanted === null) return true;
            const actual = normalize(element.textContent);
            return partial ? actual.includes(wanted) : actual === wanted;
          });
          if (!target) return false;
          (target as HTMLElement).click();
          return true;
        },
        selector,
        text ?? null,
        contains,
      ),
    { timeout, timeoutMsg: `${describeTarget(selector, text, contains)} never became clickable (present, enabled and rendered)` },
  );
}
