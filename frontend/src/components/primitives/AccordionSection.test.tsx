import { fireEvent, render, screen } from "@testing-library/react";
import { GitBranch } from "lucide-react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AccordionSection } from "./AccordionSection";

describe("AccordionSection", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("defaults to closed and hides its content", () => {
    render(
      <AccordionSection title="Branches" storageKey="test-branches">
        <div>branch list</div>
      </AccordionSection>,
    );
    expect(screen.queryByText("branch list")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Branches" })).toHaveAttribute("aria-expanded", "false");
  });

  it("honors defaultOpen when nothing is stored", () => {
    render(
      <AccordionSection title="Branches" storageKey="test-branches-2" defaultOpen>
        <div>branch list</div>
      </AccordionSection>,
    );
    expect(screen.getByText("branch list")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Branches" })).toHaveAttribute("aria-expanded", "true");
  });

  it("toggles open and closed on click, and persists the state", () => {
    render(
      <AccordionSection title="Branches" storageKey="test-branches-3">
        <div>branch list</div>
      </AccordionSection>,
    );
    const button = screen.getByRole("button", { name: "Branches" });
    fireEvent.click(button);
    expect(screen.getByText("branch list")).toBeInTheDocument();
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(localStorage.getItem("test-branches-3")).toBe("open");

    fireEvent.click(button);
    expect(screen.queryByText("branch list")).not.toBeInTheDocument();
    expect(localStorage.getItem("test-branches-3")).toBe("closed");
  });

  it("restores persisted open state on mount, overriding defaultOpen", () => {
    localStorage.setItem("test-branches-4", "open");
    render(
      <AccordionSection title="Branches" storageKey="test-branches-4" defaultOpen={false}>
        <div>branch list</div>
      </AccordionSection>,
    );
    expect(screen.getByText("branch list")).toBeInTheDocument();
  });

  it("gives the section an accessible name matching its title", () => {
    render(
      <AccordionSection title="Remotes" storageKey="test-remotes">
        <div>remote list</div>
      </AccordionSection>,
    );
    expect(screen.getByRole("region", { name: "Remotes" })).toBeInTheDocument();
  });

  it("wraps its toggle button in a level-2 heading", () => {
    render(
      <AccordionSection title="Branches" storageKey="test-branches-5">
        <div>branch list</div>
      </AccordionSection>,
    );
    const heading = screen.getByRole("heading", { level: 2, name: "Branches" });
    expect(heading).toBeInTheDocument();
    expect(heading).toContainElement(screen.getByRole("button", { name: "Branches" }));
  });

  it("renders an icon and count when provided, both hidden from the accessible tree", () => {
    render(
      <AccordionSection title="Branches" storageKey="test-branches-6" icon={GitBranch} count={3}>
        <div>branch list</div>
      </AccordionSection>,
    );
    const button = screen.getByRole("button", { name: "Branches" });
    expect(button).toHaveTextContent("3");
    expect(button.querySelector("svg")).toBeInTheDocument();
  });

  it("keeps the header's accessible name equal to the title even with icon and count set", () => {
    render(
      <AccordionSection title="Remotes" storageKey="test-remotes-2" icon={GitBranch} count={12}>
        <div>remote list</div>
      </AccordionSection>,
    );
    expect(screen.getByRole("button", { name: "Remotes" })).toBeInTheDocument();
  });

  it("renders the title at the requested heading level", () => {
    render(
      <AccordionSection title="Nested" storageKey="test-nested" headingLevel={3}>
        <div>nested body</div>
      </AccordionSection>,
    );
    expect(screen.getByRole("heading", { level: 3, name: "Nested" })).toBeInTheDocument();
  });

  it("rotates the chevron open/closed via a single icon rather than swapping elements", () => {
    render(
      <AccordionSection title="Branches" storageKey="test-branches-7">
        <div>branch list</div>
      </AccordionSection>,
    );
    const button = screen.getByRole("button", { name: "Branches" });
    const chevron = button.querySelector("svg");
    expect(chevron).not.toBeNull();
    fireEvent.click(button);
    // Same SVG node stays mounted (rotated via CSS class), not swapped for a different icon.
    expect(button.querySelector("svg")).toBe(chevron);
  });

  it("renders header actions beside the toggle without toggling the section", () => {
    const onAction = vi.fn();
    render(
      <AccordionSection
        title="Things"
        storageKey="acc-actions"
        defaultOpen
        actions={
          <button type="button" aria-label="Add thing" onClick={onAction}>
            +
          </button>
        }
      >
        <p>body</p>
      </AccordionSection>,
    );
    const heading = screen.getByRole("heading", { name: /Things/ });
    const add = screen.getByRole("button", { name: "Add thing" });
    expect(add.closest("section")?.firstElementChild).toContainElement(heading);
    expect(add.closest("section")?.firstElementChild).toContainElement(add);
    fireEvent.click(add);
    expect(onAction).toHaveBeenCalledOnce();
    expect(screen.getByText("body")).toBeInTheDocument();
  });

  describe("reveal requests (#494)", () => {
    const originalScrollIntoView = Element.prototype.scrollIntoView;
    afterEach(() => {
      Element.prototype.scrollIntoView = originalScrollIntoView;
    });

    function renderSection(revealRequest: number | undefined) {
      const scrollIntoView = vi.fn();
      Element.prototype.scrollIntoView = scrollIntoView;
      const view = render(
        <AccordionSection title="Tags" storageKey="test-reveal" revealRequest={revealRequest}>
          <div>tag list</div>
        </AccordionSection>,
      );
      const rerender = (next: number | undefined) =>
        view.rerender(
          <AccordionSection title="Tags" storageKey="test-reveal" revealRequest={next}>
            <div>tag list</div>
          </AccordionSection>,
        );
      return { scrollIntoView, rerender };
    }

    it("does not treat the value it mounts with as a request", () => {
      const { scrollIntoView } = renderSection(3);
      expect(screen.queryByText("tag list")).not.toBeInTheDocument();
      expect(scrollIntoView).not.toHaveBeenCalled();
    });

    it("expands, persists and scrolls into view on each new request", () => {
      const { scrollIntoView, rerender } = renderSection(undefined);
      rerender(1);
      expect(screen.getByText("tag list")).toBeInTheDocument();
      expect(localStorage.getItem("test-reveal")).toBe("open");
      expect(scrollIntoView).toHaveBeenCalledTimes(1);
      expect(scrollIntoView.mock.contexts[0]).toBe(screen.getByRole("region", { name: "Tags" }));

      // Collapsed by the user, then revealed again: a new request opens it once more.
      fireEvent.click(screen.getByRole("button", { name: "Tags" }));
      expect(screen.queryByText("tag list")).not.toBeInTheDocument();
      rerender(1);
      expect(screen.queryByText("tag list")).not.toBeInTheDocument();
      rerender(2);
      expect(screen.getByText("tag list")).toBeInTheDocument();
      expect(scrollIntoView).toHaveBeenCalledTimes(2);
    });

    it("ignores a request going away (another section's request)", () => {
      const { scrollIntoView, rerender } = renderSection(undefined);
      rerender(1);
      fireEvent.click(screen.getByRole("button", { name: "Tags" }));
      rerender(undefined);
      expect(screen.queryByText("tag list")).not.toBeInTheDocument();
      expect(scrollIntoView).toHaveBeenCalledTimes(1);
    });
  });
});
