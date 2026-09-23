import "@testing-library/jest-dom";

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
