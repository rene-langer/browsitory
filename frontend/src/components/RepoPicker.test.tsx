import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { RepoPicker } from "./RepoPicker";
import { fakeRepoClient } from "../test/fakeRepoClient";

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(overrides, "empty");
}

function workspaceProps() {
  return {
    onOpenWorkspace: vi.fn(),
    workspaces: [],
    workspacesLoading: false,
    workspacesError: null,
    onDismissWorkspacesError: vi.fn(),
    onCreateWorkspace: vi.fn(),
    onEditWorkspace: vi.fn(),
    onDeleteWorkspace: vi.fn(),
  };
}

describe("RepoPicker", () => {
  it("leads each recent repo with its folder name and explains workspaces", async () => {
    const client = fakeClient({ listRecentRepos: async () => ["/home/me/code/widget"] });
    render(<RepoPicker client={client} onOpenRepo={vi.fn()} {...workspaceProps()} />);
    expect(await screen.findByText("widget")).toBeInTheDocument();
    expect(screen.getByText("/home/me/code/widget")).toBeInTheDocument();
    expect(screen.getByText(/A workspace is a saved group of repositories/)).toBeInTheDocument();
  });

  it("leads a recent repo with its tab name when it has one (#625)", async () => {
    const client = fakeClient({ listRecentRepos: async () => ["/work/one/app", "/work/two/app"] });
    const aliases = new Map([["/work/one/app", "app (main)"]]);
    render(<RepoPicker client={client} onOpenRepo={vi.fn()} {...workspaceProps()} aliases={aliases} />);
    expect(await screen.findByText("app (main)")).toBeInTheDocument();
    expect(screen.getByText("app")).toBeInTheDocument();
    expect(screen.getByText("/work/one/app")).toBeInTheDocument();
  });

  it("renders each recent repo and opens it on click", async () => {
    const client = fakeClient({
      listRecentRepos: async () => ["/repo/a", "/repo/b"],
    });
    const onOpenRepo = vi.fn();

    render(<RepoPicker client={client} onOpenRepo={onOpenRepo} {...workspaceProps()} />);

    expect(await screen.findByText("/repo/a")).toBeInTheDocument();
    expect(await screen.findByText("/repo/b")).toBeInTheDocument();

    fireEvent.click(screen.getByText("/repo/a"));

    expect(onOpenRepo).toHaveBeenCalledTimes(1);
    expect(onOpenRepo).toHaveBeenCalledWith("/repo/a");
  });

  it("shows a message when there are no recent repos", async () => {
    const client = fakeClient({
      listRecentRepos: async () => [],
    });

    render(<RepoPicker client={client} onOpenRepo={vi.fn()} {...workspaceProps()} />);

    expect(await screen.findByText("No recent repositories")).toBeInTheDocument();
  });

  it("Open folder button opens the picked path", async () => {
    const client = fakeClient({
      listRecentRepos: async () => [],
      pickRepoFolder: async () => "/picked/repo",
    });
    const onOpenRepo = vi.fn();

    render(<RepoPicker client={client} onOpenRepo={onOpenRepo} {...workspaceProps()} />);

    fireEvent.click(screen.getByText("Open folder"));

    await waitFor(() => {
      expect(onOpenRepo).toHaveBeenCalledWith("/picked/repo");
    });
  });

  it("Open folder button does nothing when the dialog is cancelled", async () => {
    const client = fakeClient({
      listRecentRepos: async () => [],
      pickRepoFolder: async () => null,
    });
    const onOpenRepo = vi.fn();

    render(<RepoPicker client={client} onOpenRepo={onOpenRepo} {...workspaceProps()} />);

    fireEvent.click(screen.getByText("Open folder"));

    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(onOpenRepo).not.toHaveBeenCalled();
  });

  it("renders the error instead of the recent list when listRecentRepos rejects", async () => {
    const client = fakeClient({
      listRecentRepos: async () => {
        throw new Error("config unreadable");
      },
    });

    render(<RepoPicker client={client} onOpenRepo={vi.fn()} {...workspaceProps()} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("config unreadable");
  });
});

describe("RepoPicker recent repositories (#690)", () => {
  it("removes a recent repository from the list without opening it", async () => {
    const removeRecentRepo = vi.fn(async () => {});
    const onOpenRepo = vi.fn();
    const client = fakeClient({ listRecentRepos: async () => ["/repo/a", "/repo/b"], removeRecentRepo });
    render(<RepoPicker client={client} onOpenRepo={onOpenRepo} {...workspaceProps()} />);

    fireEvent.click(await screen.findByRole("button", { name: "Remove a from recent repositories" }));

    await waitFor(() => expect(screen.queryByText("/repo/a")).toBeNull());
    expect(removeRecentRepo).toHaveBeenCalledWith("/repo/a");
    expect(screen.getByText("/repo/b")).toBeInTheDocument();
    expect(onOpenRepo).not.toHaveBeenCalled();
  });

  it("keeps the entry and shows the error when removing fails", async () => {
    const client = fakeClient({
      listRecentRepos: async () => ["/repo/a"],
      removeRecentRepo: async () => {
        throw new Error("config read-only");
      },
    });
    render(<RepoPicker client={client} onOpenRepo={vi.fn()} {...workspaceProps()} />);

    fireEvent.click(await screen.findByRole("button", { name: "Remove a from recent repositories" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("config read-only");
    expect(screen.getByText("/repo/a")).toBeInTheDocument();
  });

  it("marks a recent repository whose folder is gone, and still opens it on click", async () => {
    const onOpenRepo = vi.fn();
    const client = fakeClient({
      listRecentRepos: async () => ["/repo/a", "/repo/gone"],
      listMissingRecentRepos: async () => ["/repo/gone"],
    });
    render(<RepoPicker client={client} onOpenRepo={onOpenRepo} {...workspaceProps()} />);

    const gone = await screen.findByRole("button", { name: /^gone Folder not found/ });
    expect(screen.getByRole("button", { name: /^a\/repo\/a$/ })).toBeInTheDocument();
    expect(screen.getAllByText("Folder not found")).toHaveLength(1);
    fireEvent.click(gone);
    expect(onOpenRepo).toHaveBeenCalledWith("/repo/gone");
  });

  it("lists every recent repository as present when the missing check fails", async () => {
    const client = fakeClient({
      listRecentRepos: async () => ["/repo/a"],
      listMissingRecentRepos: async () => {
        throw new Error("no");
      },
    });
    render(<RepoPicker client={client} onOpenRepo={vi.fn()} {...workspaceProps()} />);

    expect(await screen.findByText("/repo/a")).toBeInTheDocument();
    expect(screen.queryByText("Folder not found")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("filters recent repositories and workspaces by name or path", async () => {
    const client = fakeClient({ listRecentRepos: async () => ["/code/widget", "/code/gadget"] });
    const aliases = new Map([["/code/gadget", "Gizmo"]]);
    render(
      <RepoPicker
        client={client}
        onOpenRepo={vi.fn()}
        {...workspaceProps()}
        aliases={aliases}
        workspaces={[
          { id: "w1", name: "Widgets", rootPath: "/ws/one", memberPaths: [] },
          { id: "w2", name: "Tools", rootPath: "/ws/two", memberPaths: [] },
        ]}
      />,
    );
    await screen.findByText("/code/widget");
    const filter = screen.getByRole("searchbox", { name: "Filter repositories and workspaces" });

    fireEvent.change(filter, { target: { value: "WIDG" } });
    expect(screen.getByText("/code/widget")).toBeInTheDocument();
    expect(screen.queryByText("/code/gadget")).toBeNull();
    expect(screen.getByText("Widgets")).toBeInTheDocument();
    expect(screen.queryByText("Tools")).toBeNull();

    fireEvent.change(filter, { target: { value: "gizmo" } });
    expect(screen.getByText("/code/gadget")).toBeInTheDocument();
    expect(screen.getAllByText("Nothing matches “gizmo”.")).toHaveLength(1);

    fireEvent.change(filter, { target: { value: "nothing" } });
    expect(screen.getAllByText("Nothing matches “nothing”.")).toHaveLength(2);
  });

  it("shows no filter while there is nothing to filter", async () => {
    render(<RepoPicker client={fakeClient()} onOpenRepo={vi.fn()} {...workspaceProps()} />);
    await screen.findByText("No recent repositories");
    expect(screen.queryByRole("searchbox")).toBeNull();
  });
});

describe("RepoPicker welcome (#688)", () => {
  it("leads with the welcome, its start actions, the palette key and the user guide", async () => {
    const onDismiss = vi.fn();
    const onOpenUserGuide = vi.fn();
    const onCloneRepository = vi.fn();
    render(
      <RepoPicker
        client={fakeClient()}
        onOpenRepo={vi.fn()}
        {...workspaceProps()}
        onCloneRepository={onCloneRepository}
        welcome={{ onDismiss, onOpenUserGuide }}
      />,
    );
    const welcome = screen.getByRole("region", { name: "Welcome to Browsitory" });
    // The start actions sit in the welcome, not twice on the page.
    expect(screen.getAllByRole("button", { name: "Open folder" })).toHaveLength(1);
    fireEvent.click(within(welcome).getByRole("button", { name: "Clone…" }));
    expect(onCloneRepository).toHaveBeenCalledOnce();
    expect(within(welcome).getByText(/to search every command/)).toBeInTheDocument();
    fireEvent.click(within(welcome).getByRole("button", { name: "User guide" }));
    expect(onOpenUserGuide).toHaveBeenCalledOnce();
    fireEvent.click(within(welcome).getByRole("button", { name: "Dismiss" }));
    expect(onDismiss).toHaveBeenCalledOnce();
    await screen.findByText("No recent repositories");
  });

  it("shows no welcome without the prop", async () => {
    render(<RepoPicker client={fakeClient()} onOpenRepo={vi.fn()} {...workspaceProps()} />);
    await screen.findByText("No recent repositories");
    expect(screen.queryByRole("region", { name: "Welcome to Browsitory" })).toBeNull();
  });
});

describe("RepoPicker workspaces", () => {
  const workspace = {
    id: "ws-1",
    name: "Services",
    rootPath: "/projects",
    memberPaths: ["/projects/a", "/projects/b"],
  };

  it("renders each saved workspace by name, with its root path as a tooltip", () => {
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[workspace]}
        workspacesLoading={false}
        workspacesError={null}
        onDismissWorkspacesError={vi.fn()}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );

    expect(screen.getByText("Services")).toBeInTheDocument();
    expect(screen.getByTitle("/projects")).toBeInTheDocument();
  });

  it("Open all calls onOpenWorkspace with the workspace", () => {
    const onOpenWorkspace = vi.fn();
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={onOpenWorkspace}
        workspaces={[workspace]}
        workspacesLoading={false}
        workspacesError={null}
        onDismissWorkspacesError={vi.fn()}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Open all" }));

    expect(onOpenWorkspace).toHaveBeenCalledWith(workspace);
  });

  it("Delete asks for confirmation before calling onDeleteWorkspace", async () => {
    const onDeleteWorkspace = vi.fn().mockResolvedValue(undefined);
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[workspace]}
        workspacesLoading={false}
        workspacesError={null}
        onDismissWorkspacesError={vi.fn()}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={onDeleteWorkspace}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Delete Services" }));
    expect(onDeleteWorkspace).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Delete workspace" }));
    await waitFor(() => expect(onDeleteWorkspace).toHaveBeenCalledWith("ws-1"));
  });

  it("New workspace… shows the WorkspaceEditor in create mode", () => {
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[]}
        workspacesLoading={false}
        workspacesError={null}
        onDismissWorkspacesError={vi.fn()}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "New workspace…" }));

    expect(screen.getByText("New workspace")).toBeInTheDocument();
  });

  it("opens the selected members with the saved workspace id after creation", async () => {
    const client = fakeClient({
      listRecentRepos: async () => [],
      pickRepoFolder: async () => "/projects/root",
      scanReposInRoot: async () => ["/projects/root/a", "/projects/root/b"],
    });
    const onCreateWorkspace = vi.fn().mockResolvedValue("ws-saved");
    const onOpenWorkspace = vi.fn();
    render(
      <RepoPicker
        client={client}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={onOpenWorkspace}
        workspaces={[]}
        workspacesLoading={false}
        workspacesError={null}
        onDismissWorkspacesError={vi.fn()}
        onCreateWorkspace={onCreateWorkspace}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "New workspace…" }));
    fireEvent.click(screen.getByText("Choose root folder"));
    await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(2));
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() =>
      expect(onOpenWorkspace).toHaveBeenCalledWith({
        id: "ws-saved",
        name: "root",
        rootPath: "/projects/root",
        memberPaths: ["/projects/root/a", "/projects/root/b"],
      }),
    );
    expect(screen.queryByText("New workspace")).not.toBeInTheDocument();
  });
});

describe("clone and new repository entry points", () => {
  it("shows Clone… and New repository… when handlers are given", () => {
    const onClone = vi.fn();
    const onNew = vi.fn();
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[]}
        workspacesLoading={false}
        workspacesError={null}
        onDismissWorkspacesError={vi.fn()}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
        onCloneRepository={onClone}
        onNewRepository={onNew}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Clone…" }));
    fireEvent.click(screen.getByRole("button", { name: "New repository…" }));
    expect(onClone).toHaveBeenCalled();
    expect(onNew).toHaveBeenCalled();
  });

  it("hides them without handlers", () => {
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[]}
        workspacesLoading={false}
        workspacesError={null}
        onDismissWorkspacesError={vi.fn()}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button", { name: "Clone…" })).not.toBeInTheDocument();
  });
});
