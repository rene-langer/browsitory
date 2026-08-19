import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
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
});
