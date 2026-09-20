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
    operationDisabled: false,
    operationDisabledReason: null,
    onFetch: vi.fn(),
    onPull: vi.fn(),
    onPush: vi.fn(),
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

  it("fetches the only remote when there is no upstream, but disables Pull and Push", () => {
    const props = renderBar({ upstream: null });

    fireEvent.click(screen.getByRole("button", { name: /Fetch/ }));

    expect(props.onFetch).toHaveBeenCalledWith("origin");
    expect(screen.getByRole("button", { name: /Pull/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Push/ })).toBeDisabled();
  });

  it("disables everything with no remotes", () => {
    renderBar({ remotes: [], upstream: null });

    expect(screen.getByRole("button", { name: /Fetch/ })).toBeDisabled();
  });

  it("disables all actions while another operation runs, with the reason as a tooltip", () => {
    renderBar({ operationDisabled: true, operationDisabledReason: "A transfer is in progress." });

    const fetch = screen.getByRole("button", { name: /Fetch/ });
    expect(fetch).toBeDisabled();
    expect(fetch).toHaveAttribute("title", "A transfer is in progress.");
  });
});
