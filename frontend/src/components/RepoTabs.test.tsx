import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RepoTabs } from "./RepoTabs";

// The per-tab/group close buttons are `aria-hidden` (kept mouse-only; see the "keeps the per-tab
// close button out of the tablist's accessible children" test below), so role queries need
// `{ hidden: true }` to see them at all.
function closeButtonFor(tabName: string) {
  const tab = screen.getByRole("tab", { name: tabName });
  const wrapper = tab.closest('[role="presentation"]');
  if (wrapper === null) throw new Error(`no presentation wrapper found for tab "${tabName}"`);
  return within(wrapper as HTMLElement).getByRole("button", { hidden: true });
}

// Same story for the group close-all button: `aria-hidden` makes its accessible name compute to
// "" even with `{ hidden: true }` (that option only stops the element being excluded from query
// candidates — it doesn't skip the accname algorithm's own hidden check), so name-based queries
// never match it. Scope to the group header by its visible label text instead.
function groupCloseButtonFor(groupLabel: string) {
  const label = screen.getByText(groupLabel);
  const header = label.closest('[role="presentation"]');
  if (header === null) throw new Error(`no group header found for label "${groupLabel}"`);
  return within(header as HTMLElement).getByRole("button", { hidden: true });
}

const repos = [
  { path: "/repos/widget", displayName: "widget", workspaceId: null },
  { path: "/repos/gadget", displayName: "gadget", workspaceId: null },
];

const noneBusy = new Set<string>();

describe("RepoTabs", () => {
  it("renders one tab per open repo, marking the active one", () => {
    render(
      <RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={vi.fn()} onClose={vi.fn()} onCloseGroup={vi.fn()} onAddTab={vi.fn()} />,
    );
    const active = screen.getByRole("tab", { name: /gadget/i, selected: true });
    expect(active).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /widget/i, selected: false })).toBeInTheDocument();
  });

  it("clicking a tab calls onSwitchTo with its path", () => {
    const onSwitchTo = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={onSwitchTo} onClose={vi.fn()} onCloseGroup={vi.fn()} onAddTab={vi.fn()} />);
    screen.getByRole("tab", { name: /widget/i }).click();
    expect(onSwitchTo).toHaveBeenCalledWith("/repos/widget");
  });

  it("clicking a tab's close control calls onClose with its path, not onSwitchTo", () => {
    const onClose = vi.fn();
    const onSwitchTo = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={onSwitchTo} onClose={onClose} onCloseGroup={vi.fn()} onAddTab={vi.fn()} />);
    closeButtonFor("widget").click();
    expect(onClose).toHaveBeenCalledWith("/repos/widget");
    expect(onSwitchTo).not.toHaveBeenCalled();
  });

  it("keeps the per-tab close button out of the tablist's accessible children", () => {
    render(
      <RepoTabs
        openRepos={[
          { path: "/a", displayName: "a", workspaceId: null },
          { path: "/b", displayName: "b", workspaceId: null },
        ]}
        activePath="/a"
        busyPaths={new Set()}
        workspaceNames={{}}
        onSwitchTo={() => {}}
        onClose={() => {}}
        onCloseGroup={() => {}}
        onAddTab={() => {}}
      />,
    );

    const tablist = screen.getByRole("tablist");
    const closeButtons = within(tablist).queryAllByRole("button", { name: /^Close/ });
    expect(closeButtons).toHaveLength(0);
  });

  it("gives the per-tab close button a title that documents the Ctrl/Cmd+W shortcut, and keeps it out of the tab order", () => {
    render(
      <RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={vi.fn()} onClose={vi.fn()} onCloseGroup={vi.fn()} onAddTab={vi.fn()} />,
    );
    const closeButton = closeButtonFor("widget");
    expect(closeButton).toHaveAttribute("title", "Close (Ctrl/Cmd+W)");
    expect(closeButton).toHaveAttribute("tabindex", "-1");
    expect(closeButton).toHaveAttribute("aria-hidden", "true");
  });

  it("the trailing add button calls onAddTab", () => {
    const onAddTab = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={vi.fn()} onClose={vi.fn()} onCloseGroup={vi.fn()} onAddTab={onAddTab} />);
    screen.getByRole("button", { name: "Open another repository" }).click();
    expect(onAddTab).toHaveBeenCalled();
  });

  it("keeps the add button outside the scrolling tablist", () => {
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={vi.fn()} onClose={vi.fn()} onCloseGroup={vi.fn()} onAddTab={vi.fn()} />);
    const add = screen.getByRole("button", { name: "Open another repository" });
    expect(screen.getByRole("tablist")).not.toContainElement(add);
  });

  it("renders nothing when no repos are open", () => {
    const { container } = render(
      <RepoTabs openRepos={[]} activePath={null} busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={vi.fn()} onClose={vi.fn()} onCloseGroup={vi.fn()} onAddTab={vi.fn()} />,
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
        workspaceNames={{}}
        onSwitchTo={vi.fn()}
        onClose={onClose}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );
    const closeButton = closeButtonFor("widget");
    expect(closeButton).toBeDisabled();
    closeButton.click();
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe("RepoTabs grouping", () => {
  const grouped = [
    { path: "/repos/widget", displayName: "widget", workspaceId: "ws-1" },
    { path: "/repos/gadget", displayName: "gadget", workspaceId: "ws-1" },
    { path: "/repos/solo", displayName: "solo", workspaceId: null },
  ];

  it("wraps a contiguous run of same-workspace tabs in a chip labeled with the workspace name", () => {
    render(
      <RepoTabs
        openRepos={grouped}
        activePath="/repos/widget"
        busyPaths={noneBusy}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );

    expect(screen.getByText("Services")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /widget/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /gadget/i })).toBeInTheDocument();
  });

  it("a standalone tab (no workspaceId) renders with no chip", () => {
    render(
      <RepoTabs
        openRepos={grouped}
        activePath="/repos/widget"
        busyPaths={noneBusy}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );

    expect(screen.getByRole("tab", { name: /solo/i })).toBeInTheDocument();
    expect(groupCloseButtonFor("Services")).toBeInTheDocument();
  });

  it("clicking the chip's close-all control calls onCloseGroup with every path in that run", () => {
    const onCloseGroup = vi.fn();
    render(
      <RepoTabs
        openRepos={grouped}
        activePath="/repos/widget"
        busyPaths={noneBusy}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={onCloseGroup}
        onAddTab={vi.fn()}
      />,
    );

    fireEvent.click(groupCloseButtonFor("Services"));

    expect(onCloseGroup).toHaveBeenCalledWith(["/repos/widget", "/repos/gadget"]);
  });

  it("disables close-all when any repo in the workspace run is busy", () => {
    const onCloseGroup = vi.fn();
    render(
      <RepoTabs
        openRepos={grouped}
        activePath="/repos/widget"
        busyPaths={new Set(["/repos/gadget"])}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={onCloseGroup}
        onAddTab={vi.fn()}
      />,
    );

    const closeGroupButton = groupCloseButtonFor("Services");
    expect(closeGroupButton).toBeDisabled();
    fireEvent.click(closeGroupButton);
    expect(onCloseGroup).not.toHaveBeenCalled();
  });

  it("a tab whose workspaceId has no matching name in workspaceNames renders standalone", () => {
    render(
      <RepoTabs
        openRepos={[{ path: "/repos/orphan", displayName: "orphan", workspaceId: "deleted-ws" }]}
        activePath="/repos/orphan"
        busyPaths={noneBusy}
        workspaceNames={{}}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );

    expect(screen.getByRole("tab", { name: /orphan/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /close deleted-ws/i })).not.toBeInTheDocument();
  });
});
