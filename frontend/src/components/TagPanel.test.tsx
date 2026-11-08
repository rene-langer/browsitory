import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo, TagInfo } from "../ipc/RepoClient";
import { TagPanel } from "./TagPanel";

const tag: TagInfo = {
  name: "v1.0.0",
  targetId: "abc123",
  commitId: "abc123",
  annotated: false,
  message: null,
  taggerName: null,
  timestamp: null,
};

const origin: RemoteInfo = {
  name: "origin",
  fetchUrl: "../origin.git",
  pushUrl: null,
  authMode: null,
  authUsername: null,
};

const backup: RemoteInfo = {
  name: "backup",
  fetchUrl: "../backup.git",
  pushUrl: null,
  authMode: null,
  authUsername: null,
};

function renderPanel(overrides: Partial<Parameters<typeof TagPanel>[0]> = {}) {
  localStorage.removeItem("sidebar-tags");
  const result = render(
    <TagPanel
      tags={[tag]}
      remotes={[origin]}
      onDelete={vi.fn().mockResolvedValue(undefined)}
      onDeleteFromRemote={vi.fn()}
      onPush={vi.fn()}
      pushDisabled={false}
      operationDisabledReason={null}
      {...overrides}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Tags" }));
  return result;
}

describe("TagPanel", () => {
  it("requires confirmation before deleting a local tag", () => {
    renderPanel();

    fireEvent.click(screen.getByRole("button", { name: "Delete v1.0.0" }));

    expect(screen.getByRole("dialog", { name: "Delete local tag v1.0.0" })).toBeInTheDocument();
  });

  // #438: tags are created from the graph and the palette; the panel is the tag list.
  it("has no create form and points to where tags are created when there are none", () => {
    renderPanel({ tags: [] });

    expect(screen.queryByRole("form", { name: "Create tag" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Create tag" })).toBeNull();
    expect(screen.getByText(/choose Create tag… in its menu in the graph/)).toBeInTheDocument();
  });

  it("filters the list by name, case-insensitively", () => {
    renderPanel({ tags: [tag, { ...tag, name: "release-2" }, { ...tag, name: "V1.1" }] });

    fireEvent.change(screen.getByLabelText("Filter tags"), { target: { value: "v1" } });

    const list = screen.getByRole("list", { name: "Tags" });
    expect(within(list).getByText("v1.0.0")).toBeInTheDocument();
    expect(within(list).getByText("V1.1")).toBeInTheDocument();
    expect(within(list).queryByText("release-2")).toBeNull();

    fireEvent.change(screen.getByLabelText("Filter tags"), { target: { value: "nothing" } });
    expect(screen.getByText("No tags match “nothing”.")).toBeInTheDocument();
  });

  it("opens Delete from remote… for a tag", () => {
    const onDeleteFromRemote = vi.fn();
    renderPanel({ onDeleteFromRemote });

    fireEvent.click(screen.getByRole("button", { name: "Delete v1.0.0 from remote…" }));

    expect(onDeleteFromRemote).toHaveBeenCalledWith("v1.0.0");
  });

  it("disables Delete from remote… without a remote, and explains why", () => {
    renderPanel({ remotes: [] });

    const button = screen.getByRole("button", { name: "Delete v1.0.0 from remote…" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "No remote configured.");
  });

  it("pushes only selected tags to the selected remote", () => {
    const onPush = vi.fn();
    renderPanel({ onPush });

    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    fireEvent.click(screen.getByRole("button", { name: "Push selected tags" }));

    expect(onPush).toHaveBeenCalledWith("origin", ["v1.0.0"]);
  });

  it("offers a separate all-tags action rather than treating no selection as all tags", () => {
    const onPush = vi.fn();
    renderPanel({ onPush });

    expect(screen.getByRole("button", { name: "Push selected tags" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Push all tags" }));

    expect(onPush).toHaveBeenCalledWith("origin", []);
  });

  it("selects a newly available remote after the remotes refresh", () => {
    const onPush = vi.fn();
    const { rerender } = renderPanel({ remotes: [], onPush });

    rerender(
      <TagPanel
        tags={[tag]}
        remotes={[origin, backup]}
        onDelete={vi.fn()}
        onDeleteFromRemote={vi.fn()}
        onPush={onPush}
        pushDisabled={false}
        operationDisabledReason={null}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Push all tags" }));

    expect(onPush).toHaveBeenCalledWith("origin", []);
  });

  it("prunes selections for tags removed by a refresh", () => {
    const onPush = vi.fn();
    const { rerender } = renderPanel({ onPush });

    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    rerender(
      <TagPanel
        tags={[]}
        remotes={[origin]}
        onDelete={vi.fn()}
        onDeleteFromRemote={vi.fn()}
        onPush={onPush}
        pushDisabled={false}
        operationDisabledReason={null}
      />,
    );

    expect(screen.getByRole("button", { name: "Push selected tags" })).toBeDisabled();
  });

  it("disables tag push controls while a repository operation is active", () => {
    renderPanel({ pushDisabled: true });

    expect(screen.getByRole("button", { name: "Push selected tags" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Push all tags" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Delete v1.0.0 from remote…" })).toBeDisabled();
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("explains why the push controls are disabled via their title", () => {
    renderPanel({ pushDisabled: true, operationDisabledReason: "A transfer is in progress." });

    for (const name of ["Push selected tags", "Push all tags", "Delete v1.0.0 from remote…", "Delete v1.0.0"]) {
      expect(screen.getByRole("button", { name })).toHaveAttribute("title", "A transfer is in progress.");
    }
  });

  it("deletes only after the local-delete confirmation", async () => {
    const onDelete = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onDelete });

    fireEvent.click(screen.getByRole("button", { name: "Delete v1.0.0" }));
    const dialog = screen.getByRole("dialog", { name: "Delete local tag v1.0.0" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete tag" }));

    expect(onDelete).toHaveBeenCalledWith("v1.0.0");
  });
});
