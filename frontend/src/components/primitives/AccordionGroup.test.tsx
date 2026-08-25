import { act, fireEvent, render, screen } from "@testing-library/react";
import { useEffect, useRef, useState, type MutableRefObject } from "react";
import { describe, expect, it } from "vitest";
import { AccordionGroup, useAccordionGroup, type AccordionGroupHandle } from "./AccordionGroup";

function Header({ label, open, onOpenChange }: { label: string; open: boolean; onOpenChange: (open: boolean) => void }) {
  const ref = useRef<HTMLButtonElement>(null);
  const group = useAccordionGroup();
  useEffect(() => {
    return group?.register({ ref, setOpen: onOpenChange });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // `isActive` compares ref identity only; it never dereferences `.current`, so reading it
  // during render is safe. `react-hooks/refs` can't verify that statically.
  // eslint-disable-next-line react-hooks/refs
  const tabIndex = group === null ? 0 : group.isActive(ref) ? 0 : -1;
  return (
    <button
      ref={ref}
      type="button"
      aria-expanded={open}
      tabIndex={tabIndex}
      onFocus={() => group?.onHeaderFocus(ref)}
      onKeyDown={(event) => group?.onHeaderKeyDown(event, ref)}
      onClick={() => onOpenChange(!open)}
    >
      {label}
    </button>
  );
}

function ThreeHeaders({ groupRef }: { groupRef?: MutableRefObject<AccordionGroupHandle | null> }) {
  const [open, setOpen] = useState({ a: false, b: false, c: false });
  return (
    <AccordionGroup groupRef={groupRef}>
      <Header label="A" open={open.a} onOpenChange={(v) => setOpen((s) => ({ ...s, a: v }))} />
      <Header label="B" open={open.b} onOpenChange={(v) => setOpen((s) => ({ ...s, b: v }))} />
      <Header label="C" open={open.c} onOpenChange={(v) => setOpen((s) => ({ ...s, c: v }))} />
    </AccordionGroup>
  );
}

describe("AccordionGroup", () => {
  it("gives only the first registered header tabIndex 0 initially", () => {
    render(<ThreeHeaders />);
    expect(screen.getByRole("button", { name: "A" })).toHaveAttribute("tabIndex", "0");
    expect(screen.getByRole("button", { name: "B" })).toHaveAttribute("tabIndex", "-1");
    expect(screen.getByRole("button", { name: "C" })).toHaveAttribute("tabIndex", "-1");
  });

  it("moves the roving tab stop with ArrowDown/ArrowUp, wrapping at the ends", () => {
    render(<ThreeHeaders />);
    const a = screen.getByRole("button", { name: "A" });
    const b = screen.getByRole("button", { name: "B" });
    const c = screen.getByRole("button", { name: "C" });

    act(() => fireEvent.keyDown(a, { key: "ArrowDown" }));
    expect(b).toHaveFocus();
    expect(b).toHaveAttribute("tabIndex", "0");
    expect(a).toHaveAttribute("tabIndex", "-1");

    act(() => fireEvent.keyDown(b, { key: "ArrowDown" }));
    expect(c).toHaveFocus();

    act(() => fireEvent.keyDown(c, { key: "ArrowDown" }));
    expect(a).toHaveFocus();

    act(() => fireEvent.keyDown(a, { key: "ArrowUp" }));
    expect(c).toHaveFocus();
  });

  it("Home/End jump to the first/last header", () => {
    render(<ThreeHeaders />);
    const b = screen.getByRole("button", { name: "B" });
    act(() => fireEvent.keyDown(b, { key: "End" }));
    expect(screen.getByRole("button", { name: "C" })).toHaveFocus();
    act(() => fireEvent.keyDown(screen.getByRole("button", { name: "C" }), { key: "Home" }));
    expect(screen.getByRole("button", { name: "A" })).toHaveFocus();
  });

  it("ArrowDown/ArrowUp/Home/End do not toggle open state", () => {
    render(<ThreeHeaders />);
    const a = screen.getByRole("button", { name: "A" });
    act(() => fireEvent.keyDown(a, { key: "ArrowDown" }));
    expect(a).toHaveAttribute("aria-expanded", "false");
  });

  it("expandAll/collapseAll drive every registered header via the imperative handle", () => {
    const groupRef: MutableRefObject<AccordionGroupHandle | null> = { current: null };
    render(<ThreeHeaders groupRef={groupRef} />);

    act(() => groupRef.current?.expandAll());
    expect(screen.getByRole("button", { name: "A" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "B" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "C" })).toHaveAttribute("aria-expanded", "true");

    act(() => groupRef.current?.collapseAll());
    expect(screen.getByRole("button", { name: "A" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("button", { name: "B" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("button", { name: "C" })).toHaveAttribute("aria-expanded", "false");
  });
});
