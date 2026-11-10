import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RepoTabs } from "./RepoTabs";
import styles from "./RepoTabs.module.css";
import repoTabsCss from "./RepoTabs.module.css?raw";
import appCss from "../App.module.css?raw";

// The declarations of the plain `.name { ... }` rules in a stylesheet (no compound selectors).
function declarationsOf(css: string, className: string): string[] {
  const rule = new RegExp(`^\\.${className}\\s*\\{([^}]*)\\}`, "gm");
  return [...css.matchAll(rule)].map((match) => match[1]);
}

// A fixed `height`/`max-height` (not `min-height`) on a box around the tabs.
function fixesHeight(declarations: string[]): boolean {
  return declarations.some((block) => /(^|[;{\s])(max-)?height\s*:/.test(block));
}

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
  { path: "/repos/widget", displayName: "widget", alias: null, workspaceId: null },
  { path: "/repos/gadget", displayName: "gadget", alias: null, workspaceId: null },
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
          { path: "/a", displayName: "a", alias: null, workspaceId: null },
          { path: "/b", displayName: "b", alias: null, workspaceId: null },
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

  it("gives the per-tab close button a title that documents the Delete shortcut, and keeps it out of the tab order", () => {
    render(
      <RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={vi.fn()} onClose={vi.fn()} onCloseGroup={vi.fn()} onAddTab={vi.fn()} />,
    );
    const closeButton = closeButtonFor("widget");
    expect(closeButton).toHaveAttribute("title", "Close (Delete)");
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

describe("RepoTabs change marker", () => {
  it("marks a tab whose repository has uncommitted changes, in words as well as a dot (#613)", () => {
    render(
      <RepoTabs
        openRepos={repos}
        activePath="/repos/gadget"
        busyPaths={noneBusy}
        workspaceNames={{}}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
        changedPaths={new Set(["/repos/widget"])}
      />,
    );
    const dirty = screen.getByRole("tab", { name: "widget" });
    expect(dirty).toHaveAccessibleDescription("Uncommitted changes");
    expect(dirty.closest(`.${styles.tab}`)?.querySelector(`.${styles.changeMarker}`)).not.toBeNull();
    const clean = screen.getByRole("tab", { name: "gadget" });
    expect(clean).toHaveAccessibleDescription("/repos/gadget");
    expect(clean.closest(`.${styles.tab}`)?.querySelector(`.${styles.changeMarker}`)).toBeNull();
  });
});

describe("RepoTabs grouping", () => {
  const grouped = [
    { path: "/repos/widget", displayName: "widget", alias: null, workspaceId: "ws-1" },
    { path: "/repos/gadget", displayName: "gadget", alias: null, workspaceId: "ws-1" },
    { path: "/repos/solo", displayName: "solo", alias: null, workspaceId: null },
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
        openRepos={[{ path: "/repos/orphan", displayName: "orphan", alias: null, workspaceId: "deleted-ws" }]}
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

describe("RepoTabs tab menu", () => {
  const renderTabs = (tabMenuItems?: (path: string) => { label: string; onSelect: () => void }[]) =>
    render(
      <RepoTabs
        openRepos={repos}
        activePath="/repos/widget"
        busyPaths={noneBusy}
        workspaceNames={{}}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
        tabMenuItems={tabMenuItems}
      />,
    );

  it("opens a tab menu from right-click and from the keyboard", () => {
    const onPick = vi.fn();
    const tabMenuItems = vi.fn((path: string) => [{ label: "Open terminal here", onSelect: () => onPick(path) }]);
    renderTabs(tabMenuItems);

    fireEvent.contextMenu(screen.getByRole("tab", { name: "widget" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Open terminal here" }));
    expect(onPick).toHaveBeenCalledWith("/repos/widget");
    expect(screen.queryByRole("menu")).toBeNull();

    fireEvent.keyDown(screen.getByRole("tab", { name: "gadget" }), { key: "F10", shiftKey: true });
    expect(screen.getByRole("menuitem", { name: "Open terminal here" })).toBeInTheDocument();
    expect(tabMenuItems).toHaveBeenLastCalledWith("/repos/gadget");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

    fireEvent.keyDown(screen.getByRole("tab", { name: "widget" }), { key: "ContextMenu" });
    expect(tabMenuItems).toHaveBeenLastCalledWith("/repos/widget");
    expect(screen.getByRole("tab", { name: "widget" })).toHaveAttribute("aria-haspopup", "menu");
  });

  it("has no tab menu without tabMenuItems", () => {
    renderTabs();
    fireEvent.contextMenu(screen.getByRole("tab", { name: "widget" }));
    fireEvent.keyDown(screen.getByRole("tab", { name: "widget" }), { key: "F10", shiftKey: true });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("tab", { name: "widget" })).not.toHaveAttribute("aria-haspopup");
  });
});

describe("RepoTabs header height (#606)", () => {
  it("leaves every box around a grouped tab free to grow to the group label plus its tabs", () => {
    render(
      <RepoTabs
        openRepos={[
          { path: "/repos/widget", displayName: "widget", alias: null, workspaceId: "ws-1" },
          { path: "/repos/gadget", displayName: "gadget", alias: null, workspaceId: "ws-1" },
        ]}
        activePath="/repos/widget"
        busyPaths={noneBusy}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );

    // jsdom can't lay out, so check the rules behind each ancestor of the grouped tab instead:
    // the group stacks its label row above its tabs, and none of them may cap that height.
    const tab = screen.getByRole("tab", { name: "widget" });
    for (const name of ["tab", "groupTabs", "group", "tabs", "strip"] as const) {
      expect(tab.closest(`.${styles[name]}`)).not.toBeNull();
      const rules = declarationsOf(repoTabsCss, name);
      expect(rules.length).toBeGreaterThan(0);
      expect({ name, fixed: fixesHeight(rules) }).toEqual({ name, fixed: false });
    }
    expect(screen.getByText("Services").closest(`.${styles.groupHeader}`)).not.toBeNull();
  });

  it("gives the app header row a minimum height, not a fixed one", () => {
    const headerRow = declarationsOf(appCss, "headerRow");
    expect(headerRow).toHaveLength(1);
    expect(fixesHeight(headerRow)).toBe(false);
    expect(headerRow[0]).toMatch(/min-height\s*:/);
  });
});

describe("RepoTabs alias (#625)", () => {
  it("shows the alias as the tab's name and keeps the full path as its title", () => {
    render(
      <RepoTabs
        openRepos={[
          { path: "/work/one/app", displayName: "app (main)", alias: "app (main)", workspaceId: null },
          { path: "/work/two/app", displayName: "app", alias: null, workspaceId: null },
        ]}
        activePath="/work/one/app"
        busyPaths={noneBusy}
        workspaceNames={{}}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );
    expect(screen.getByRole("tab", { name: "app (main)" })).toHaveAttribute("title", "/work/one/app");
    expect(screen.getByRole("tab", { name: "app" })).toHaveAttribute("title", "/work/two/app");
  });

  it("F2 on a focused tab asks to rename that tab", () => {
    const onRenameTab = vi.fn();
    render(
      <RepoTabs
        openRepos={repos}
        activePath="/repos/gadget"
        busyPaths={noneBusy}
        workspaceNames={{}}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
        onRenameTab={onRenameTab}
      />,
    );
    const tab = screen.getByRole("tab", { name: "widget" });
    expect(tab).toHaveAttribute("aria-keyshortcuts", "Delete F2");
    fireEvent.keyDown(tab, { key: "F2" });
    expect(onRenameTab).toHaveBeenCalledWith("/repos/widget");
  });
});

describe("RepoTabs tab list (#625)", () => {
  const many = [
    { path: "/work/api", displayName: "api", alias: null, workspaceId: "ws-1" },
    { path: "/work/web", displayName: "storefront", alias: "storefront", workspaceId: "ws-1" },
    { path: "/other/tools", displayName: "tools", alias: null, workspaceId: null },
  ];

  function renderMany(overrides: { onSwitchTo?: (path: string) => void; changedPaths?: ReadonlySet<string> } = {}) {
    render(
      <RepoTabs
        openRepos={many}
        activePath="/work/api"
        busyPaths={noneBusy}
        workspaceNames={{ "ws-1": "Shop" }}
        onSwitchTo={overrides.onSwitchTo ?? vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
        changedPaths={overrides.changedPaths}
      />,
    );
  }

  const optionPaths = () => screen.queryAllByRole("option").map((option) => option.getAttribute("title"));

  it("opens a list of every open tab, grouped by workspace, with the filter focused", () => {
    renderMany({ changedPaths: new Set(["/other/tools"]) });
    const button = screen.getByRole("button", { name: "List open tabs" });
    expect(button).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(button);

    expect(button).toHaveAttribute("aria-expanded", "true");
    const filter = screen.getByRole("combobox", { name: "Filter open tabs" });
    expect(filter).toHaveFocus();
    const shop = screen.getByRole("group", { name: "Shop" });
    expect(within(shop).getAllByRole("option").map((option) => option.getAttribute("title"))).toEqual(["/work/api", "/work/web"]);
    const loose = screen.getByRole("group", { name: "Not in a workspace" });
    const tools = within(loose).getByRole("option");
    expect(tools).toHaveTextContent("tools, uncommitted changes");
    expect(tools).toHaveTextContent("/other/tools");
    // The current tab is highlighted first and marked.
    const api = within(shop).getAllByRole("option")[0];
    expect(api).toHaveAttribute("aria-selected", "true");
    expect(api).toHaveAttribute("aria-current", "page");
    expect(filter).toHaveAttribute("aria-activedescendant", api.id);
  });

  it("filters by name, folder name and path", () => {
    renderMany();
    fireEvent.click(screen.getByRole("button", { name: "List open tabs" }));
    const filter = screen.getByRole("combobox", { name: "Filter open tabs" });

    fireEvent.change(filter, { target: { value: "STORE" } });
    expect(optionPaths()).toEqual(["/work/web"]);
    // The renamed tab's folder name still finds it.
    fireEvent.change(filter, { target: { value: "web" } });
    expect(optionPaths()).toEqual(["/work/web"]);
    fireEvent.change(filter, { target: { value: "/other" } });
    expect(optionPaths()).toEqual(["/other/tools"]);
    fireEvent.change(filter, { target: { value: "nothing like it" } });
    expect(optionPaths()).toEqual([]);
    expect(screen.getByText("No tab matches")).toBeInTheDocument();
  });

  it("Enter switches to the highlighted tab, moved with the arrow keys, and closes the list", () => {
    const onSwitchTo = vi.fn();
    renderMany({ onSwitchTo });
    fireEvent.click(screen.getByRole("button", { name: "List open tabs" }));
    const filter = screen.getByRole("combobox", { name: "Filter open tabs" });

    fireEvent.keyDown(filter, { key: "ArrowDown" });
    fireEvent.keyDown(filter, { key: "ArrowDown" });
    expect(screen.getByRole("option", { selected: true })).toHaveAttribute("title", "/other/tools");
    fireEvent.keyDown(filter, { key: "ArrowDown" });
    expect(screen.getByRole("option", { selected: true })).toHaveAttribute("title", "/work/api");
    fireEvent.keyDown(filter, { key: "ArrowUp" });
    fireEvent.keyDown(filter, { key: "Enter" });

    expect(onSwitchTo).toHaveBeenCalledWith("/other/tools");
    expect(screen.queryByRole("combobox", { name: "Filter open tabs" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "tools" })).toHaveFocus();
  });

  it("Enter takes the first match of the filter", () => {
    const onSwitchTo = vi.fn();
    renderMany({ onSwitchTo });
    fireEvent.click(screen.getByRole("button", { name: "List open tabs" }));
    const filter = screen.getByRole("combobox", { name: "Filter open tabs" });
    fireEvent.change(filter, { target: { value: "storefront" } });
    fireEvent.keyDown(filter, { key: "Enter" });
    expect(onSwitchTo).toHaveBeenCalledWith("/work/web");
  });

  it("a click on a tab in the list switches to it", () => {
    const onSwitchTo = vi.fn();
    renderMany({ onSwitchTo });
    fireEvent.click(screen.getByRole("button", { name: "List open tabs" }));
    fireEvent.click(screen.getByRole("option", { name: /storefront/ }));
    expect(onSwitchTo).toHaveBeenCalledWith("/work/web");
    expect(screen.queryByRole("listbox", { name: "Open tabs" })).not.toBeInTheDocument();
  });

  it("Escape, a press outside and the button itself close the list without switching", () => {
    const onSwitchTo = vi.fn();
    renderMany({ onSwitchTo });
    const button = screen.getByRole("button", { name: "List open tabs" });

    fireEvent.click(button);
    fireEvent.keyDown(screen.getByRole("combobox", { name: "Filter open tabs" }), { key: "Escape" });
    expect(screen.queryByRole("listbox", { name: "Open tabs" })).not.toBeInTheDocument();
    expect(button).toHaveFocus();

    fireEvent.click(button);
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("listbox", { name: "Open tabs" })).not.toBeInTheDocument();

    fireEvent.click(button);
    fireEvent.mouseDown(button);
    fireEvent.click(button);
    expect(screen.queryByRole("listbox", { name: "Open tabs" })).not.toBeInTheDocument();
    expect(onSwitchTo).not.toHaveBeenCalled();
  });
});
