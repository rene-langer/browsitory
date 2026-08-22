import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient, Workspace } from "../ipc/RepoClient";
import { WorkspaceEditor } from "./WorkspaceEditor";

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return {
    pickRepoFolder: vi.fn().mockResolvedValue(null),
    scanReposInRoot: vi.fn().mockResolvedValue([]),
    ...overrides,
  } as unknown as RepoClient;
}

describe("WorkspaceEditor", () => {
  describe("create mode", () => {
    it("shows a root-picker button before any root is chosen", () => {
      render(<WorkspaceEditor client={fakeClient()} onSave={vi.fn()} onCancel={vi.fn()} />);
      expect(screen.getByText("Choose Root Folder")).toBeInTheDocument();
      expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    });

    it("scans the picked root and pre-checks every found repo, defaulting the name to the root's basename", async () => {
      const scanReposInRoot = vi.fn().mockResolvedValue(["/projects/root/a", "/projects/root/b"]);
      const client = fakeClient({
        pickRepoFolder: vi.fn().mockResolvedValue("/projects/root"),
        scanReposInRoot,
      });
      render(<WorkspaceEditor client={client} onSave={vi.fn()} onCancel={vi.fn()} />);
      expect(scanReposInRoot).not.toHaveBeenCalled();

      fireEvent.click(screen.getByText("Choose Root Folder"));

      await waitFor(() => expect(scanReposInRoot).toHaveBeenCalledWith("/projects/root"));
      await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(2));
      for (const checkbox of screen.getAllByRole("checkbox")) {
        expect(checkbox).toBeChecked();
      }
      expect(screen.getByLabelText("Workspace name")).toHaveValue("root");
    });

    it("save calls onSave with the name, root, and only the checked members", async () => {
      const client = fakeClient({
        pickRepoFolder: vi.fn().mockResolvedValue("/projects/root"),
        scanReposInRoot: vi.fn().mockResolvedValue(["/projects/root/a", "/projects/root/b"]),
      });
      const onSave = vi.fn().mockResolvedValue(undefined);
      render(<WorkspaceEditor client={client} onSave={onSave} onCancel={vi.fn()} />);

      fireEvent.click(screen.getByText("Choose Root Folder"));
      await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(2));

      fireEvent.click(screen.getByLabelText("/projects/root/b"));
      fireEvent.change(screen.getByLabelText("Workspace name"), { target: { value: "My Root" } });
      fireEvent.click(screen.getByText("Save"));

      await waitFor(() => expect(onSave).toHaveBeenCalledWith("My Root", "/projects/root", ["/projects/root/a"]));
    });

    it("save is disabled when no member is checked", async () => {
      const client = fakeClient({
        pickRepoFolder: vi.fn().mockResolvedValue("/projects/root"),
        scanReposInRoot: vi.fn().mockResolvedValue(["/projects/root/a"]),
      });
      render(<WorkspaceEditor client={client} onSave={vi.fn()} onCancel={vi.fn()} />);

      fireEvent.click(screen.getByText("Choose Root Folder"));
      await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(1));
      fireEvent.click(screen.getByLabelText("/projects/root/a"));

      expect(screen.getByText("Save")).toBeDisabled();
    });

    it("cancelling the root-picker dialog leaves the picker step showing", async () => {
      const client = fakeClient({ pickRepoFolder: vi.fn().mockResolvedValue(null) });
      render(<WorkspaceEditor client={client} onSave={vi.fn()} onCancel={vi.fn()} />);

      fireEvent.click(screen.getByText("Choose Root Folder"));

      await waitFor(() => expect(client.pickRepoFolder).toHaveBeenCalled());
      expect(screen.getByText("Choose Root Folder")).toBeInTheDocument();
    });
  });

  describe("edit mode", () => {
    const existing: Workspace = {
      id: "ws-1",
      name: "Services",
      rootPath: "/projects/root",
      memberPaths: ["/projects/root/a"],
    };

    it("scans the existing root immediately, pre-checking current members and leaving new finds unchecked", async () => {
      const scanReposInRoot = vi.fn().mockResolvedValue(["/projects/root/a", "/projects/root/c"]);
      const client = fakeClient({
        scanReposInRoot,
      });
      render(<WorkspaceEditor client={client} existing={existing} onSave={vi.fn()} onCancel={vi.fn()} />);

      await waitFor(() => expect(scanReposInRoot).toHaveBeenCalledWith("/projects/root"));
      await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(2));
      expect(screen.getByLabelText("/projects/root/a")).toBeChecked();
      expect(screen.getByLabelText("/projects/root/c")).not.toBeChecked();
      expect(screen.getByLabelText("Workspace name")).toHaveValue("Services");
      expect(screen.queryByText("Choose Root Folder")).not.toBeInTheDocument();
    });

    it("save calls onSave with the immutable existing root", async () => {
      const client = fakeClient({ scanReposInRoot: vi.fn().mockResolvedValue(["/projects/root/a"]) });
      const onSave = vi.fn().mockResolvedValue(undefined);
      render(<WorkspaceEditor client={client} existing={existing} onSave={onSave} onCancel={vi.fn()} />);

      await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(1));
      fireEvent.click(screen.getByText("Save"));

      await waitFor(() => expect(onSave).toHaveBeenCalledWith("Services", "/projects/root", ["/projects/root/a"]));
    });

    it("drops saved members that are no longer returned by an explicit edit scan", async () => {
      const workspaceWithVanishedMember = {
        ...existing,
        memberPaths: ["/projects/root/a", "/projects/root/vanished"],
      };
      const client = fakeClient({
        scanReposInRoot: vi.fn().mockResolvedValue(["/projects/root/a", "/projects/root/c"]),
      });
      const onSave = vi.fn().mockResolvedValue(undefined);
      render(
        <WorkspaceEditor
          client={client}
          existing={workspaceWithVanishedMember}
          onSave={onSave}
          onCancel={vi.fn()}
        />,
      );

      await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(2));
      fireEvent.click(screen.getByText("Save"));

      await waitFor(() =>
        expect(onSave).toHaveBeenCalledWith("Services", "/projects/root", ["/projects/root/a"]),
      );
    });
  });

  it("cancel calls onCancel", () => {
    const onCancel = vi.fn();
    render(<WorkspaceEditor client={fakeClient()} onSave={vi.fn()} onCancel={onCancel} />);
    fireEvent.click(screen.getByText("Cancel"));
    expect(onCancel).toHaveBeenCalled();
  });
});
