import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { TransferProgress } from "../ipc/RepoClient";
import type { TransferSubject } from "../lib/transferCopy";
import { RepoActivity } from "./RepoActivity";

const progress = (overrides: Partial<TransferProgress> = {}): TransferProgress => ({
  operationId: "push-1",
  operation: "PushBranch",
  phase: "Receiving",
  errorKind: null,
  current: 0,
  total: 0,
  receivedBytes: 0,
  message: null,
  hookOutput: null,
  ...overrides,
});

const pushSubject: TransferSubject = {
  kind: "push",
  localBranch: "main",
  target: "origin/main",
  ahead: 2,
  setUpstream: false,
  force: false,
};

describe("RepoActivity", () => {
  it("is an empty live region when nothing runs", () => {
    render(<RepoActivity subject={null} progress={null} pending={false} onCancel={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("");
    expect(screen.queryByRole("progressbar")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("names the transfer before its first event, with no Cancel yet", () => {
    render(<RepoActivity subject={pushSubject} progress={null} pending onCancel={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("Pushing main to origin/main");
    const bar = screen.getByRole("progressbar", { name: "Pushing main to origin/main" });
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(bar).toHaveAttribute("aria-valuetext", "Connecting…");
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
  });

  it("shows objects and bytes as the bar's value, and cancels the running operation", () => {
    const onCancel = vi.fn();
    render(
      <RepoActivity
        subject={{ kind: "fetch", remoteName: "origin" }}
        progress={progress({ operationId: "fetch-1", operation: "Fetch", current: 25, total: 100, receivedBytes: 2048 })}
        pending
        onCancel={onCancel}
      />,
    );
    const bar = screen.getByRole("progressbar", { name: "Fetching from origin" });
    expect(bar).toHaveAttribute("aria-valuenow", "25");
    expect(bar).toHaveAttribute("aria-valuetext", "25 / 100 objects, 2.0 KB received");
    expect(screen.getByRole("group", { name: "Fetching from origin" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledWith("fetch-1");
  });

  it("says sent for a push", () => {
    render(
      <RepoActivity subject={pushSubject} progress={progress({ current: 1, total: 4, receivedBytes: 10 })} pending onCancel={vi.fn()} />,
    );
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuetext", "1 / 4 objects, 10 B sent");
  });

  it("shows the remote's own status line while there are no counts", () => {
    render(
      <RepoActivity subject={pushSubject} progress={progress({ message: "Compressing objects: 40%" })} pending onCancel={vi.fn()} />,
    );
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuetext", "Compressing objects: 40%");
  });

  it("names a running pre-push or post-merge hook, keeping Cancel", () => {
    const onCancel = vi.fn();
    const { rerender } = render(
      <RepoActivity subject={pushSubject} progress={progress({ phase: "RunningHook" })} pending onCancel={onCancel} />,
    );
    expect(screen.getByText("Running pre-push hook…")).toBeInTheDocument();
    rerender(
      <RepoActivity
        subject={{ kind: "pull", target: "origin/main" }}
        progress={progress({ operationId: "pull-1", operation: "Pull", phase: "RunningHook" })}
        pending
        onCancel={onCancel}
      />,
    );
    expect(screen.getByText("Running post-merge hook…")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledWith("pull-1");
  });

  it("latches Cancel once pressed, and re-arms it for the next transfer", () => {
    const onCancel = vi.fn();
    const { rerender } = render(
      <RepoActivity subject={pushSubject} progress={progress({ current: 1, total: 4 })} pending onCancel={onCancel} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    // Progress keeps arriving until git reaches its next callback: still latched.
    rerender(<RepoActivity subject={pushSubject} progress={progress({ current: 2, total: 4 })} pending onCancel={onCancel} />);
    const latched = screen.getByRole("button", { name: "Cancelling…" });
    expect(latched).toBeDisabled();
    fireEvent.click(latched);
    expect(onCancel).toHaveBeenCalledTimes(1);

    rerender(<RepoActivity subject={pushSubject} progress={progress({ operationId: "push-2" })} pending onCancel={onCancel} />);
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
  });

  it("shows another action in flight quietly: not announced, no Cancel", () => {
    render(<RepoActivity subject={null} progress={null} pending onCancel={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("");
    expect(screen.getByRole("progressbar", { name: "Working…" })).toBeInTheDocument();
    expect(screen.queryByRole("button")).toBeNull();
  });
});
