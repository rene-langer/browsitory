import "@testing-library/jest-dom/vitest";
import { configure, getConfig } from "@testing-library/react";

// Testing Library's timing budgets, tuned for a loaded machine (#217).
//
// A `findBy*`/`waitFor` retries its query on every DOM change and every 50 ms. Each failed try
// builds a "TestingLibraryElementError" whose message holds a pretty-printed dump of the whole
// document. `waitFor` throws all of these away except the last one, so on a full-App render the
// dumps alone were about 15% of the CPU time of App.test.tsx. Build the dump only when something
// reads the message: a failed synchronous `getBy*` (Vitest reads it right away, before cleanup)
// or the error `waitFor` rejects with when it times out. The message text is unchanged.
//
// `asyncUtilTimeout` (default 1000 ms) is how long a `findBy*`/`waitFor` waits. One second is
// enough on an idle machine but not under load: with a load average of about 24 on 4 cores,
// most App.test.tsx failures were a `findBy*` giving up after 1 s while the App was still
// re-rendering. Waiting longer only slows a test that is failing anyway; a passing test returns
// as soon as its query matches. Keep it below Vitest's 5 s default `testTimeout`, so a test that
// does fail reports the element it was waiting for, not just "Test timed out".
const buildElementError = getConfig().getElementError;
configure({
  asyncUtilTimeout: 4000,
  getElementError(message, container) {
    const error = new Error(message ?? "");
    error.name = "TestingLibraryElementError";
    let fullMessage: string | undefined;
    Object.defineProperty(error, "message", {
      configurable: true,
      enumerable: false,
      get: () => (fullMessage ??= buildElementError(message, container).message),
      set: (value: string) => {
        fullMessage = value;
      },
    });
    return error;
  },
});

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: () => ({ matches: false }),
});

// jsdom has no IntersectionObserver. Default to reporting every observed element as intersecting
// immediately, so existing tests that don't care about viewport-gating (PERF-001) keep passing
// without each one having to stub it. Tests that specifically exercise the gate (DiffPane.test.tsx)
// override this with their own `vi.stubGlobal("IntersectionObserver", ...)`.
class ImmediatelyIntersectingObserver implements IntersectionObserver {
  readonly root: Element | Document | null = null;
  readonly rootMargin: string = "";
  readonly scrollMargin: string = "";
  readonly thresholds: ReadonlyArray<number> = [];
  private readonly callback: IntersectionObserverCallback;

  constructor(callback: IntersectionObserverCallback) {
    this.callback = callback;
  }

  observe(target: Element): void {
    this.callback(
      [{ isIntersecting: true, target } as IntersectionObserverEntry],
      this,
    );
  }

  unobserve(): void {}
  disconnect(): void {}
  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }
}

Object.defineProperty(window, "IntersectionObserver", {
  writable: true,
  configurable: true,
  value: ImmediatelyIntersectingObserver,
});
Object.defineProperty(globalThis, "IntersectionObserver", {
  writable: true,
  configurable: true,
  value: ImmediatelyIntersectingObserver,
});
