import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo, TagInfo } from "../ipc/RepoClient";
import { TagPanel } from "./TagPanel";

const tag: TagInfo = {
  name: "v1.0.0",
  targetId: "abc123",
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
      onCreate={vi.fn()}
      onDelete={vi.fn().mockResolvedValue(undefined)}
      onPush={vi.fn()}
      pushDisabled={false}
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

  it("requires a message before creating an annotated tag", () => {
    const onCreate = vi.fn();
    renderPanel({ onCreate });

    fireEvent.click(screen.getByRole("radio", { name: "Annotated tag" }));
    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v2.0.0" } });
    fireEvent.click(screen.getByRole("button", { name: "Create tag" }));

    expect(onCreate).not.toHaveBeenCalled();
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
        onCreate={vi.fn()}
        onDelete={vi.fn()}
        onPush={onPush}
        pushDisabled={false}
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
        onCreate={vi.fn()}
        onDelete={vi.fn()}
        onPush={onPush}
        pushDisabled={false}
      />,
    );

    expect(screen.getByRole("button", { name: "Push selected tags" })).toBeDisabled();
  });

  it("disables tag push controls while a repository operation is active", () => {
    renderPanel({ pushDisabled: true });

    expect(screen.getByRole("button", { name: "Push selected tags" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Push all tags" })).toBeDisabled();
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
