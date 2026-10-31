import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SignatureInput } from "../ipc/RepoClient";
import { CoAuthorPicker } from "./CoAuthorPicker";

const authors: SignatureInput[] = [{ name: "Ada", email: "ada@example.com" }];

// jsdom does no layout: the button's rect and the popover's size are stubbed. jsdom's window is
// 1024 × 768.
let popoverHeight = 200;
const originals = {
  offsetWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth"),
  scrollHeight: Object.getOwnPropertyDescriptor(Element.prototype, "scrollHeight"),
};

beforeEach(() => {
  popoverHeight = 200;
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => 300 });
  Object.defineProperty(Element.prototype, "scrollHeight", { configurable: true, get: () => popoverHeight });
});

afterEach(() => {
  if (originals.offsetWidth !== undefined) Object.defineProperty(HTMLElement.prototype, "offsetWidth", originals.offsetWidth);
  if (originals.scrollHeight !== undefined) Object.defineProperty(Element.prototype, "scrollHeight", originals.scrollHeight);
  vi.restoreAllMocks();
});

function stubButton(rect: { top: number; left: number }) {
  const button = screen.getByRole("button", { name: "Add co-author" });
  vi.spyOn(button, "getBoundingClientRect").mockImplementation(
    () =>
      ({
        top: rect.top,
        bottom: rect.top + 20,
        left: rect.left,
        right: rect.left + 120,
        width: 120,
        height: 20,
        x: rect.left,
        y: rect.top,
        toJSON: () => ({}),
      }) as DOMRect,
  );
  return button;
}

function popover(): HTMLElement {
  return screen.getByRole("listbox", { name: "Recent authors" }).closest("[data-side]") as HTMLElement;
}

describe("CoAuthorPicker placement", () => {
  it("opens above the button when there is no room below", async () => {
    render(<CoAuthorPicker load={async () => authors} exclude={[]} onPick={vi.fn()} />);
    fireEvent.click(stubButton({ top: 740, left: 20 }));
    await screen.findByRole("option", { name: "Ada <ada@example.com>" });
    expect(popover()).toHaveAttribute("data-side", "above");
    expect(popover().style.top).toBe(`${740 - 4 - 200}px`);
    expect(popover().style.left).toBe("20px");
  });

  it("shifts left to stay inside the window at the right edge", async () => {
    render(<CoAuthorPicker load={async () => authors} exclude={[]} onPick={vi.fn()} />);
    fireEvent.click(stubButton({ top: 100, left: 900 }));
    await screen.findByRole("option", { name: "Ada <ada@example.com>" });
    expect(popover()).toHaveAttribute("data-side", "below");
    expect(popover().style.left).toBe(`${1024 - 300 - 4}px`);
  });

  it("re-places itself when the window resizes", async () => {
    render(<CoAuthorPicker load={async () => authors} exclude={[]} onPick={vi.fn()} />);
    const button = stubButton({ top: 740, left: 20 });
    fireEvent.click(button);
    await screen.findByRole("option", { name: "Ada <ada@example.com>" });
    expect(popover()).toHaveAttribute("data-side", "above");

    vi.mocked(button.getBoundingClientRect).mockReturnValue({
      top: 10,
      bottom: 30,
      left: 20,
      right: 140,
      width: 120,
      height: 20,
      x: 20,
      y: 10,
      toJSON: () => ({}),
    } as DOMRect);
    act(() => {
      window.dispatchEvent(new Event("resize"));
    });
    expect(popover()).toHaveAttribute("data-side", "below");
    expect(popover().style.top).toBe("34px");
  });

  it("re-measures once the authors load", async () => {
    let resolve: (value: SignatureInput[]) => void = () => {};
    const load = () =>
      new Promise<SignatureInput[]>((r) => {
        resolve = r;
      });
    render(<CoAuthorPicker load={load} exclude={[]} onPick={vi.fn()} />);
    popoverHeight = 80;
    fireEvent.click(stubButton({ top: 740, left: 20 }));
    await screen.findByRole("status");
    expect(popover().style.top).toBe(`${740 - 4 - 80}px`);

    popoverHeight = 200;
    await act(async () => resolve(authors));
    await screen.findByRole("option", { name: "Ada <ada@example.com>" });
    expect(popover().style.top).toBe(`${740 - 4 - 200}px`);
  });

  it("stops listening for resize and scroll once closed", async () => {
    const remove = vi.spyOn(window, "removeEventListener");
    render(<CoAuthorPicker load={async () => authors} exclude={[]} onPick={vi.fn()} />);
    const button = stubButton({ top: 100, left: 20 });
    fireEvent.click(button);
    await screen.findByRole("option", { name: "Ada <ada@example.com>" });
    fireEvent.click(button);
    expect(remove.mock.calls.map(([type]) => type)).toEqual(expect.arrayContaining(["resize", "scroll"]));
  });
});
