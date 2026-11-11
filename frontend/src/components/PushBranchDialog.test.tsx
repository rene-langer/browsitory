import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo } from "../ipc/RepoClient";
import { PushBranchDialog } from "./PushBranchDialog";

const remote = (name: string): RemoteInfo => ({ name, fetchUrl: name, pushUrl: null, authMode: null, authUsername: null });

describe("PushBranchDialog", () => {
  it("defaults to origin, the local name, and Set as upstream when the branch has none", () => {
    const onPush = vi.fn();
    render(
      <PushBranchDialog
        localBranch="topic"
        remotes={[remote("backup"), remote("origin")]}
        branchUpstream={null}
        onPush={onPush}
        onCancel={() => {}}
      />,
    );

    expect(screen.getByRole("dialog", { name: "Push topic" })).toBeInTheDocument();
    expect(screen.getByLabelText("Remote")).toHaveValue("origin");
    expect(screen.getByLabelText("Remote branch")).toHaveValue("topic");
    expect(screen.getByLabelText("Set as upstream")).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "Push" }));
    expect(onPush).toHaveBeenCalledWith("origin", "topic", true);
  });

  it("falls back to the first remote without origin", () => {
    render(
      <PushBranchDialog
        localBranch="topic"
        remotes={[remote("backup"), remote("mirror")]}
        branchUpstream={undefined}
        onPush={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(screen.getByLabelText("Remote")).toHaveValue("backup");
  });

  it("starts on the branch's upstream remote, unchecked, and pushes the edited name", () => {
    const onPush = vi.fn();
    render(
      <PushBranchDialog
        localBranch="topic"
        remotes={[remote("origin"), remote("backup")]}
        branchUpstream="backup/topic"
        onPush={onPush}
        onCancel={() => {}}
      />,
    );

    expect(screen.getByLabelText("Remote")).toHaveValue("backup");
    expect(screen.getByLabelText("Set as upstream")).not.toBeChecked();
    fireEvent.change(screen.getByLabelText("Remote branch"), { target: { value: "  renamed " } });
    fireEvent.click(screen.getByRole("button", { name: "Push" }));
    expect(onPush).toHaveBeenCalledWith("backup", "renamed", false);
  });

  it("blocks an empty or invalid remote branch name", () => {
    const onPush = vi.fn();
    render(
      <PushBranchDialog localBranch="topic" remotes={[remote("origin")]} branchUpstream={null} onPush={onPush} onCancel={() => {}} />,
    );

    fireEvent.change(screen.getByLabelText("Remote branch"), { target: { value: "bad name" } });
    expect(screen.getByText("Branch names can't contain spaces.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Push" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Remote branch"), { target: { value: "HEAD" } });
    expect(screen.getByRole("button", { name: "Push" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Remote branch"), { target: { value: "@" } });
    expect(screen.getByText("Branch name can't be just @.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Push" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Remote branch"), { target: { value: "refs/heads/x" } });
    expect(screen.getByText("Give the branch name without refs/.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Push" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Remote branch"), { target: { value: " " } });
    expect(screen.getByRole("button", { name: "Push" })).toBeDisabled();
    fireEvent.submit(screen.getByRole("button", { name: "Push" }).closest("form")!);
    expect(onPush).not.toHaveBeenCalled();
  });

  it("cancels", () => {
    const onCancel = vi.fn();
    render(
      <PushBranchDialog localBranch="topic" remotes={[remote("origin")]} branchUpstream={null} onPush={() => {}} onCancel={onCancel} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });

  // #514: the repository can become busy while the dialog is open.
  it("disables Push with the reason and sends nothing while the repository is busy", () => {
    const onPush = vi.fn();
    render(
      <PushBranchDialog
        localBranch="topic"
        remotes={[remote("origin")]}
        branchUpstream={null}
        blockedReason="A transfer is in progress."
        onPush={onPush}
        onCancel={() => {}}
      />,
    );
    const push = screen.getByRole("button", { name: "Push" });
    expect(push).toBeDisabled();
    expect(push).toHaveAttribute("title", "A transfer is in progress.");
    expect(push).toHaveAccessibleDescription("A transfer is in progress.");
    expect(screen.getByText("A transfer is in progress.")).toBeInTheDocument();
    fireEvent.submit(push.closest("form")!);
    expect(onPush).not.toHaveBeenCalled();
  });
});
