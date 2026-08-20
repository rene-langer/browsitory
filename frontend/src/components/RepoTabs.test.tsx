import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RepoTabs } from "./RepoTabs";

const repos = [
  { path: "/repos/widget", displayName: "widget" },
  { path: "/repos/gadget", displayName: "gadget" },
];

const noneBusy = new Set<string>();

describe("RepoTabs", () => {
  it("renders one tab per open repo, marking the active one", () => {
    render(
      <RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} onSwitchTo={vi.fn()} onClose={vi.fn()} onAddTab={vi.fn()} />,
    );
    const active = screen.getByRole("tab", { name: /gadget/i, selected: true });
    expect(active).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /widget/i, selected: false })).toBeInTheDocument();
  });

  it("clicking a tab calls onSwitchTo with its path", () => {
    const onSwitchTo = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} onSwitchTo={onSwitchTo} onClose={vi.fn()} onAddTab={vi.fn()} />);
    screen.getByRole("tab", { name: /widget/i }).click();
    expect(onSwitchTo).toHaveBeenCalledWith("/repos/widget");
  });

  it("clicking a tab's close control calls onClose with its path, not onSwitchTo", () => {
    const onClose = vi.fn();
    const onSwitchTo = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} onSwitchTo={onSwitchTo} onClose={onClose} onAddTab={vi.fn()} />);
    screen.getByRole("button", { name: /close widget/i }).click();
    expect(onClose).toHaveBeenCalledWith("/repos/widget");
    expect(onSwitchTo).not.toHaveBeenCalled();
  });

  it("the trailing add button calls onAddTab", () => {
    const onAddTab = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} onSwitchTo={vi.fn()} onClose={vi.fn()} onAddTab={onAddTab} />);
    screen.getByRole("button", { name: "Open another repository" }).click();
    expect(onAddTab).toHaveBeenCalled();
  });

  it("renders nothing when no repos are open", () => {
    const { container } = render(
      <RepoTabs openRepos={[]} activePath={null} busyPaths={noneBusy} onSwitchTo={vi.fn()} onClose={vi.fn()} onAddTab={vi.fn()} />,
    );
    expect(container.firstElementChild).toBeNull();
  });

  it("disables the close button for a tab in busyPaths, and clicking it does not call onClose", () => {
    const onClose = vi.fn();
    render(
      <RepoTabs
        openRepos={repos}
        activePath="/repos/gadget"
        busyPaths={new Set(["/repos/widget"])}
        onSwitchTo={vi.fn()}
        onClose={onClose}
        onAddTab={vi.fn()}
      />,
    );
    const closeButton = screen.getByRole("button", { name: /close widget/i });
    expect(closeButton).toBeDisabled();
    closeButton.click();
    expect(onClose).not.toHaveBeenCalled();
  });
});
