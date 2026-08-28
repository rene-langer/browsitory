import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AccordionSection } from "./AccordionSection";
import { Sidebar } from "./Sidebar";

describe("Sidebar", () => {
  it("renders its children inside a labeled complementary landmark", () => {
    render(
      <Sidebar>
        <div>section one</div>
        <div>section two</div>
      </Sidebar>,
    );
    // aside, not nav: this sidebar holds panels for branches/remotes/tags/etc, not a set of
    // page-to-page navigation links, so "complementary" is the accurate landmark role.
    const aside = screen.getByRole("complementary", { name: "Repository sections" });
    expect(aside).toBeInTheDocument();
    expect(screen.getByText("section one")).toBeInTheDocument();
    expect(screen.getByText("section two")).toBeInTheDocument();
  });

  it("expands every section when Expand all is clicked", () => {
    render(
      <Sidebar>
        <AccordionSection title="One" storageKey="sidebar-test-one">
          <div>one body</div>
        </AccordionSection>
        <AccordionSection title="Two" storageKey="sidebar-test-two">
          <div>two body</div>
        </AccordionSection>
      </Sidebar>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Expand all sections" }));
    expect(screen.getByRole("button", { name: "One" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "Two" })).toHaveAttribute("aria-expanded", "true");
  });

  it("collapses every section when Collapse all is clicked", () => {
    render(
      <Sidebar>
        <AccordionSection title="One" storageKey="sidebar-test-three" defaultOpen>
          <div>one body</div>
        </AccordionSection>
        <AccordionSection title="Two" storageKey="sidebar-test-four" defaultOpen>
          <div>two body</div>
        </AccordionSection>
      </Sidebar>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Collapse all sections" }));
    expect(screen.getByRole("button", { name: "One" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("button", { name: "Two" })).toHaveAttribute("aria-expanded", "false");
  });

  it("does not render a settings button when panelToggles is omitted", () => {
    render(
      <Sidebar>
        <div>body</div>
      </Sidebar>,
    );
    expect(screen.queryByRole("button", { name: "Sidebar section settings" })).not.toBeInTheDocument();
  });

  it("opens a checkbox popover listing each panel toggle, and calls onToggle when one is checked", () => {
    const onToggle = vi.fn();
    render(
      <Sidebar
        panelToggles={[
          { id: "worktree", label: "Worktrees", visible: true, onToggle },
          { id: "reflog", label: "Reflog", visible: false, onToggle: vi.fn() },
        ]}
      >
        <div>body</div>
      </Sidebar>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Sidebar section settings" }));
    const worktreeCheckbox = screen.getByRole("checkbox", { name: "Worktrees" });
    const reflogCheckbox = screen.getByRole("checkbox", { name: "Reflog" });
    expect(worktreeCheckbox).toBeChecked();
    expect(reflogCheckbox).not.toBeChecked();
    fireEvent.click(worktreeCheckbox);
    expect(onToggle).toHaveBeenCalledWith(false);
  });

  it("closes the settings popover when its button is clicked again", () => {
    render(
      <Sidebar panelToggles={[{ id: "worktree", label: "Worktrees", visible: true, onToggle: vi.fn() }]}>
        <div>body</div>
      </Sidebar>,
    );
    const button = screen.getByRole("button", { name: "Sidebar section settings" });
    fireEvent.click(button);
    expect(screen.getByRole("checkbox", { name: "Worktrees" })).toBeInTheDocument();
    fireEvent.click(button);
    expect(screen.queryByRole("checkbox", { name: "Worktrees" })).not.toBeInTheDocument();
  });
});
