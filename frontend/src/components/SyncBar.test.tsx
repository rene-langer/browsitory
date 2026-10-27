import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";
import { SyncBar } from "./SyncBar";

const remote: RemoteInfo = { name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null };
const upstream: UpstreamInfo = {
  localBranch: "main",
  remoteName: "origin",
  remoteBranch: "main",
  ahead: 2,
  behind: 1,
};

function renderBar(overrides: Partial<React.ComponentProps<typeof SyncBar>> = {}) {
  const props = {
    remotes: [remote],
    upstream,
    detachedAt: null,
    operationDisabled: false,
    operationDisabledReason: null,
    onFetch: vi.fn(),
    onPull: vi.fn(),
    onPush: vi.fn(),
    prune: false,
    onFetchAll: vi.fn(),
    onTogglePrune: vi.fn(),
    onPublish: vi.fn(),
    ...overrides,
  };
  render(<SyncBar {...props} />);
  return props;
}

describe("SyncBar", () => {
  it("shows ahead and behind counts", () => {
    renderBar();

    expect(screen.getByText("↑2")).toBeInTheDocument();
    expect(screen.getByText("↓1")).toBeInTheDocument();
  });

  it("hides counts when the tracking ref was never fetched", () => {
    renderBar({ upstream: { ...upstream, ahead: null, behind: null } });

    expect(screen.queryByText(/↑/)).toBeNull();
  });

  it("fetches and pushes against the upstream's remote and pulls the upstream", () => {
    const props = renderBar();

    fireEvent.click(screen.getByRole("button", { name: /Fetch/ }));
    fireEvent.click(screen.getByRole("button", { name: /Pull/ }));
    fireEvent.click(screen.getByRole("button", { name: /Push/ }));

    expect(props.onFetch).toHaveBeenCalledWith("origin");
    expect(props.onPull).toHaveBeenCalledTimes(1);
    expect(props.onPush).toHaveBeenCalledWith("origin");
  });

  it("fetches the only remote when there is no upstream, disables Pull, and offers to publish", () => {
    const props = renderBar({ upstream: null });

    fireEvent.click(screen.getByRole("button", { name: /Fetch/ }));

    expect(props.onFetch).toHaveBeenCalledWith("origin");
    expect(screen.getByRole("button", { name: /Pull/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Push/ })).toBeEnabled();
  });

  it("disables everything with no remotes", () => {
    renderBar({ remotes: [], upstream: null });

    expect(screen.getByRole("button", { name: /Fetch/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Push/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Push/ })).toHaveAttribute("title", "No remote configured.");
  });

  it("opens fetch actions: Fetch all remotes and the Prune checkbox", () => {
    const props = renderBar({ prune: true });
    const chevron = screen.getByRole("button", { name: "More fetch actions" });
    expect(chevron).toHaveAttribute("aria-haspopup", "menu");
    expect(chevron).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(chevron);
    expect(chevron).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(screen.getByRole("menuitem", { name: "Fetch all remotes" }));
    expect(props.onFetchAll).toHaveBeenCalled();
    expect(screen.queryByRole("menu")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "More fetch actions" }));
    const prune = screen.getByRole("menuitemcheckbox", { name: "Prune" });
    expect(prune).toHaveAttribute("aria-checked", "true");
    expect(prune).toHaveAttribute("title", "Remove remote-tracking branches that were deleted on the remote");
    fireEvent.click(prune);
    expect(props.onTogglePrune).toHaveBeenCalledWith(false);
  });

  it("disables fetch actions exactly when Fetch is disabled", () => {
    renderBar({ remotes: [], upstream: null });
    expect(screen.getByRole("button", { name: "More fetch actions" })).toBeDisabled();
  });

  it("disables fetch actions while another operation runs", () => {
    renderBar({ operationDisabled: true, operationDisabledReason: "Busy." });
    expect(screen.getByRole("button", { name: "More fetch actions" })).toBeDisabled();
  });

  it("publishes through the push dialog when the branch has no upstream", () => {
    const props = renderBar({ upstream: null });
    const push = screen.getByRole("button", { name: "Push" });
    expect(push).toBeEnabled();
    expect(push).toHaveAttribute("title", "Push this branch to a remote and set its upstream…");
    fireEvent.click(push);
    expect(props.onPublish).toHaveBeenCalled();
    expect(props.onPush).not.toHaveBeenCalled();
  });

  it("keeps Push disabled without upstream on a detached HEAD", () => {
    renderBar({ upstream: null, detachedAt: "abc1234" });
    expect(screen.getByRole("button", { name: "Push" })).toBeDisabled();
  });

  it("disables all actions while another operation runs, with the reason as a tooltip", () => {
    renderBar({ operationDisabled: true, operationDisabledReason: "A transfer is in progress." });

    const fetch = screen.getByRole("button", { name: /Fetch/ });
    expect(fetch).toBeDisabled();
    expect(fetch).toHaveAttribute("title", "A transfer is in progress.");
  });

  it("shows a detached HEAD and disables push and pull", () => {
    renderBar({ detachedAt: "abc1234" });

    expect(screen.getByText("HEAD detached at abc1234")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Push/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Pull/ })).toBeDisabled();
  });
});

describe("SyncBar leading slot", () => {
  it("renders the leading node", () => {
    renderBar({ leading: <span>lead</span> });
    expect(screen.getByText("lead")).toBeInTheDocument();
  });
});
