import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo } from "../ipc/RepoClient";
import { DeleteRemoteBranchDialog } from "./DeleteRemoteBranchDialog";

const remote = (name: string): RemoteInfo => ({ name, fetchUrl: name, pushUrl: null, authMode: null, authUsername: null });

describe("DeleteRemoteBranchDialog", () => {
  it("confirms a preset branch without listing anything", () => {
    const onDelete = vi.fn();
    const onList = vi.fn(async () => []);
    render(
      <DeleteRemoteBranchDialog
        remotes={[remote("origin")]}
        initialRemote="origin"
        initialBranch="topic"
        onListRemoteBranches={onList}
        onDelete={onDelete}
        onCancel={() => {}}
      />,
    );

    expect(screen.getByRole("dialog", { name: "Delete remote branch" })).toHaveTextContent(
      "Delete origin/topic on the remote? This can't be undone.",
    );
    expect(screen.queryByLabelText("Branch")).toBeNull();
    expect(onList).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "Delete remote branch" }));
    expect(onDelete).toHaveBeenCalledWith("origin", "topic");
  });

  it("lets the palette pick a remote and one of its branches", async () => {
    const onDelete = vi.fn();
    const onList = vi.fn(async (name: string) => (name === "origin" ? ["main", "topic"] : ["mirror-only"]));
    render(
      <DeleteRemoteBranchDialog
        remotes={[remote("origin"), remote("backup")]}
        initialRemote={null}
        initialBranch={null}
        onListRemoteBranches={onList}
        onDelete={onDelete}
        onCancel={() => {}}
      />,
    );

    await waitFor(() => expect(screen.getByLabelText("Branch")).toBeEnabled());
    // Nothing is preselected: a destructive action needs an explicit choice.
    expect(screen.getByLabelText("Branch")).toHaveValue("");
    expect(screen.getByRole("option", { name: "Choose a branch…" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete remote branch" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Branch"), { target: { value: "topic" } });
    expect(screen.getByRole("button", { name: "Delete remote branch" })).toBeEnabled();
    expect(screen.getByRole("dialog", { name: "Delete remote branch" })).toHaveTextContent(
      "Delete origin/topic on the remote? This can't be undone.",
    );
    fireEvent.click(screen.getByRole("button", { name: "Delete remote branch" }));
    expect(onDelete).toHaveBeenCalledWith("origin", "topic");

    fireEvent.change(screen.getByLabelText("Remote"), { target: { value: "backup" } });
    await waitFor(() => expect(screen.getByRole("option", { name: "mirror-only" })).toBeInTheDocument());
    expect(screen.getByLabelText("Branch")).toHaveValue("");
    expect(screen.getByRole("button", { name: "Delete remote branch" })).toBeDisabled();
  });

  it("shows a load failure and keeps Delete disabled", async () => {
    render(
      <DeleteRemoteBranchDialog
        remotes={[remote("origin")]}
        initialRemote={null}
        initialBranch={null}
        onListRemoteBranches={async () => {
          throw new Error("x");
        }}
        onDelete={() => {}}
        onCancel={() => {}}
      />,
    );

    expect(await screen.findByText("Could not list branches for origin.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete remote branch" })).toBeDisabled();
  });

  it("keeps Delete disabled for a remote with no branches", async () => {
    render(
      <DeleteRemoteBranchDialog
        remotes={[remote("origin")]}
        initialRemote={null}
        initialBranch={null}
        onListRemoteBranches={async () => []}
        onDelete={() => {}}
        onCancel={() => {}}
      />,
    );

    await waitFor(() => expect(screen.getByLabelText("Branch")).toBeEnabled());
    expect(screen.getByRole("button", { name: "Delete remote branch" })).toBeDisabled();
  });
});
