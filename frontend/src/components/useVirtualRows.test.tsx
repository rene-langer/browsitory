import { act, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useVirtualRows } from "./useVirtualRows";

// #828: content above a windowed list, in the same scroller, changes height without a scroll.
// jsdom has no layout, so a small block-flow model stands in: every `li` is ROW tall, an element
// with an inline `height` is that tall, any other element is as tall as its children (a `ul`
// plus its padding), and elements stack from the pane's top, moved up by its `scrollTop`.
// A fake ResizeObserver reports, on `frame()`, each observed element whose height in that model
// changed since it last reported, and every newly observed one once, as browsers do.

const ROW = 26;
const PANE_HEIGHT = 260;
const COUNT = 1000;

function heightOf(element: Element): number {
  if (element.tagName === "LI") return ROW;
  const style = (element as HTMLElement).style;
  if (style.height !== "") return parseFloat(style.height);
  const padding = (parseFloat(style.paddingTop) || 0) + (parseFloat(style.paddingBottom) || 0);
  return padding + [...element.children].reduce((sum, child) => sum + heightOf(child), 0);
}

/** The top of `element` relative to the pane's content (not scrolled). */
function offsetIn(pane: Element, element: Element): number {
  let top = 0;
  for (let node: Element = element; node !== pane && node.parentElement !== null; node = node.parentElement) {
    for (let sibling = node.previousElementSibling; sibling !== null; sibling = sibling.previousElementSibling)
      top += heightOf(sibling);
  }
  return top;
}

const observers = new Set<FakeResizeObserver>();

class FakeResizeObserver {
  private readonly callback: ResizeObserverCallback;
  /** Observed elements and the height they last reported (`undefined`: not yet). */
  private readonly targets = new Map<Element, number | undefined>();
  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
    observers.add(this);
  }
  observe(target: Element) {
    if (!this.targets.has(target)) this.targets.set(target, undefined);
  }
  unobserve(target: Element) {
    this.targets.delete(target);
  }
  disconnect() {
    this.targets.clear();
    observers.delete(this);
  }
  deliver() {
    const entries: ResizeObserverEntry[] = [];
    for (const [target, reported] of this.targets) {
      if (!target.isConnected) continue;
      const height = heightOf(target);
      if (height === reported) continue;
      this.targets.set(target, height);
      entries.push({ target, contentRect: { height } } as unknown as ResizeObserverEntry);
    }
    if (entries.length > 0) this.callback(entries, this as unknown as ResizeObserver);
  }
}

/** A rendered frame: lets pending mutation records through, then delivers resize observations. */
async function frame() {
  await act(async () => {
    await Promise.resolve();
  });
  act(() => {
    for (const observer of [...observers]) observer.deliver();
  });
}

let pane: HTMLDivElement;
const original = Element.prototype.getBoundingClientRect;

beforeEach(() => {
  observers.clear();
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  pane = document.createElement("div");
  pane.style.overflowY = "auto";
  // A scroller's own height does not follow its content.
  pane.style.height = `${PANE_HEIGHT}px`;
  Object.defineProperty(pane, "clientHeight", { configurable: true, value: PANE_HEIGHT });
  document.body.appendChild(pane);
  Element.prototype.getBoundingClientRect = function (this: Element) {
    const top = this === pane ? 0 : offsetIn(pane, this) - pane.scrollTop;
    const height = heightOf(this);
    return { top, left: 0, right: 0, bottom: top + height, width: 0, height, x: 0, y: top, toJSON: () => ({}) };
  };
});

afterEach(() => {
  Element.prototype.getBoundingClientRect = original;
  pane.remove();
  vi.unstubAllGlobals();
});

/** A sidebar-like section above the list that opens and closes on its own state. */
function Section({ label, initiallyOpen, openHeight }: { label: string; initiallyOpen: boolean; openHeight: number }) {
  const [open, setOpen] = useState(initiallyOpen);
  return (
    <div>
      <button type="button" style={{ height: ROW }} onClick={() => setOpen((value) => !value)}>
        {label}
      </button>
      {open && <div style={{ height: openHeight }} />}
    </div>
  );
}

/** A placeholder replaced by different content once "loaded", like the Branches tree (#660). */
function Loading({ loadedHeight }: { loadedHeight: number }) {
  const [loaded, setLoaded] = useState(false);
  return loaded ? (
    <section style={{ height: loadedHeight }} />
  ) : (
    <div style={{ height: 2000 }}>
      <button type="button" style={{ height: ROW }} onClick={() => setLoaded(true)}>
        Load
      </button>
    </div>
  );
}

function List() {
  const [list, setList] = useState<HTMLUListElement | null>(null);
  const virtual = useVirtualRows(list, COUNT);
  const rows = [];
  for (let index = virtual.start; index < virtual.end; index++)
    rows.push(
      <li key={index} data-row-index={index}>
        row {index}
      </li>,
    );
  return (
    <section>
      <h3 style={{ height: ROW }}>Rows</h3>
      <ul
        ref={setList}
        style={virtual.windowed ? { paddingTop: virtual.padTop, paddingBottom: virtual.padBottom } : undefined}
      >
        {rows}
      </ul>
    </section>
  );
}

const renderedRows = () => [...pane.querySelectorAll<HTMLElement>("ul > li")].map((row) => Number(row.dataset.rowIndex));

describe("useVirtualRows — content above the list changes height without a scroll (#828)", () => {
  it("renders the rows that come into view when a section above closes", async () => {
    render(
      <>
        <Section label="Above" initiallyOpen openHeight={2000} />
        <List />
      </>,
      { container: pane },
    );
    await frame();
    // The list starts 2052px down a 260px pane: no row is in view or within the margin.
    expect(renderedRows()).toEqual([]);

    fireEvent.click(screen.getByRole("button", { name: "Above" }));
    await frame();

    // Now it starts at 52px: rows 0–7 are in view, 20 more below them.
    const rows = renderedRows();
    expect(rows[0]).toBe(0);
    expect(rows.at(-1)).toBe(27);
  });

  it("renders the rows now in view when a section above opens", async () => {
    render(
      <>
        <Section label="Above" initiallyOpen={false} openHeight={2000} />
        <List />
      </>,
      { container: pane },
    );
    await frame();
    pane.scrollTop = 52 + ROW * 500;
    fireEvent.scroll(pane);
    expect(renderedRows()[0]).toBe(480);

    fireEvent.click(screen.getByRole("button", { name: "Above" }));
    await frame();

    // The list moved down by 2000px: row 423 is now at the pane's top.
    const rows = renderedRows();
    expect(rows[0]).toBe(403);
    expect(rows.at(-1)).toBe(453);
  });

  it("renders rows once a tall placeholder above is replaced by shorter content", async () => {
    render(
      <>
        <Loading loadedHeight={ROW} />
        <List />
      </>,
      { container: pane },
    );
    await frame();
    expect(renderedRows()).toEqual([]);

    fireEvent.click(screen.getByRole("button", { name: "Load" }));
    await frame();

    // The list's rows start at 52px: rows 0–7 are in view, 20 more below them.
    const rows = renderedRows();
    expect(rows[0]).toBe(0);
    expect(rows.at(-1)).toBe(27);
  });

  it("follows a section above that sits inside the list's own wrapper", async () => {
    render(
      <div>
        <Section label="Nested" initiallyOpen openHeight={2000} />
        <List />
      </div>,
      { container: pane },
    );
    await frame();
    expect(renderedRows()).toEqual([]);

    fireEvent.click(screen.getByRole("button", { name: "Nested" }));
    await frame();

    expect(renderedRows()[0]).toBe(0);
  });

  it("stops observing once the list unmounts", async () => {
    const { unmount } = render(
      <>
        <Section label="Above" initiallyOpen openHeight={2000} />
        <List />
      </>,
      { container: pane },
    );
    await frame();
    unmount();
    expect(observers.size).toBe(0);
  });
});
