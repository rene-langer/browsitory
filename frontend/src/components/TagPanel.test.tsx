import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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

  it("requires a message before creating an annotated tag", () => {
    const onCreate = vi.fn();
    renderPanel({ onCreate });

    fireEvent.click(screen.getByRole("radio", { name: "Annotated tag" }));
    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v2.0.0" } });
    fireEvent.click(screen.getByRole("button", { name: "Create tag" }));

    expect(onCreate).not.toHaveBeenCalled();
  });

  it("clears the form after a successful create", async () => {
    const onCreate = vi.fn().mockResolvedValue(null);
    renderPanel({ onCreate });

    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v2.0.0" } });
    fireEvent.click(screen.getByRole("button", { name: "Create tag" }));

    expect(onCreate).toHaveBeenCalledWith("v2.0.0", null);
    await waitFor(() => expect(screen.getByLabelText("Tag name")).toHaveValue(""));
  });

  // `useAppState`'s `createTag` never rejects — it reports failure by resolving to the message,
  // the same contract `RemotePanel`'s `addRemote` established. See issue #30/UX-002.
  it("shows a failed create-tag's message inline and keeps the entered name", async () => {
    const onCreate = vi.fn().mockResolvedValue("tag already exists");
    renderPanel({ onCreate });

    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v1.0.0" } });
    fireEvent.click(screen.getByRole("button", { name: "Create tag" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("tag already exists");
    expect(screen.getByLabelText("Tag name")).toHaveValue("v1.0.0");
  });

  it("clears the create-tag failure message once the name is edited again", async () => {
    const onCreate = vi.fn().mockResolvedValue("tag already exists");
    renderPanel({ onCreate });

    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v1.0.0" } });
    fireEvent.click(screen.getByRole("button", { name: "Create tag" }));
    await screen.findByRole("alert");

    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v1.0.1" } });

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
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
        onCreate={vi.fn()}
        onDelete={vi.fn()}
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
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("explains why the push controls are disabled via their title", () => {
    renderPanel({ pushDisabled: true, operationDisabledReason: "A transfer is in progress." });

    expect(screen.getByRole("button", { name: "Create tag" })).toHaveAttribute(
      "title",
      "A transfer is in progress.",
    );
    expect(screen.getByRole("button", { name: "Push selected tags" })).toHaveAttribute(
      "title",
      "A transfer is in progress.",
    );
    expect(screen.getByRole("button", { name: "Push all tags" })).toHaveAttribute(
      "title",
      "A transfer is in progress.",
    );
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
