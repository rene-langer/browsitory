import { act, fireEvent, render, screen } from "@testing-library/react";
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

  it("offers the pull strategies in Pull's menu, checking the one in effect (#652)", () => {
    const onPullWith = vi.fn();
    renderBar({ pullStrategy: { current: "Rebase", onPullWith } });

    fireEvent.click(screen.getByRole("button", { name: "Pull options" }));
    expect(screen.getByRole("menuitemcheckbox", { name: "Pull with rebase" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("menuitemcheckbox", { name: "Pull with merge" })).toHaveAttribute("aria-checked", "false");
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Pull, fast-forward only" }));
    expect(onPullWith).toHaveBeenCalledWith("FastForwardOnly");
  });

  it("disables Pull's menu with Pull", () => {
    renderBar({ upstream: null, pullStrategy: { current: null, onPullWith: vi.fn() } });
    expect(screen.getByRole("button", { name: "Pull" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Pull options" })).toBeDisabled();
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

  it("names the push target, or says Push will ask when push.default does not choose one", () => {
    renderBar({ upstream: { ...upstream, localBranch: "fx", remoteBranch: "feature-x", pushBranch: "feature-x" } });
    expect(screen.getByRole("button", { name: "Push" })).toHaveAttribute("title", "Push to origin/feature-x");
  });

  it("asks where to push when push.default does not choose a target", () => {
    const props = renderBar({ upstream: { ...upstream, localBranch: "fx", remoteBranch: "feature-x", pushBranch: null } });
    const push = screen.getByRole("button", { name: "Push" });
    expect(push).toHaveAttribute("title", "Choose where to push fx on origin…");
    fireEvent.click(push);
    expect(props.onPush).toHaveBeenCalledWith("origin");
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

describe("SyncBar last fetch", () => {
  it("shows nothing before the first fetch of the session", () => {
    renderBar({ lastFetchedAt: null });

    expect(screen.queryByText(/^Fetched/)).toBeNull();
  });

  it("shows when the last fetch ran and keeps the label current", () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date("2026-11-08T12:00:00Z"));
      renderBar({ lastFetchedAt: Date.now() - 10_000, autoFetchMinutes: 10 });

      const label = screen.getByText("Fetched just now");
      expect(label).toHaveAttribute("title", expect.stringContaining("Fetches automatically every 10 minutes"));

      act(() => {
        vi.advanceTimersByTime(3 * 60_000);
      });
      expect(screen.getByText("Fetched 3 minutes ago")).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("says automatic fetching is off in the tooltip when it is", () => {
    renderBar({ lastFetchedAt: Date.now() - 2 * 3600_000, autoFetchMinutes: 0 });

    const label = screen.getByText("Fetched 2 hours ago");
    expect(label.getAttribute("title")).not.toContain("automatically");
  });
});

describe("SyncBar leading slot", () => {
  it("renders the leading node", () => {
    renderBar({ leading: <span>lead</span> });
    expect(screen.getByText("lead")).toBeInTheDocument();
  });

  it("disables Pull while a diverged pull waits for an answer (#636)", () => {
    renderBar({ pullPending: true });
    const pull = screen.getByRole("button", { name: "Pull" });
    expect(pull).toBeDisabled();
    expect(pull).toHaveAttribute("title", "Choose how to finish the diverged pull first.");
  });

  it("stashes all, or opens the stash dialog, and follows the stash rule (#637)", () => {
    const stash = { reason: null, onStash: vi.fn(), onStashWithOptions: vi.fn() };
    renderBar({ stash });
    fireEvent.click(screen.getByRole("button", { name: "Stash" }));
    expect(stash.onStash).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "More stash options" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Stash…" }));
    expect(stash.onStashWithOptions).toHaveBeenCalledOnce();
  });

  it("disables Stash and its menu with the reason", () => {
    renderBar({ stash: { reason: "A rebase is in progress.", onStash: vi.fn(), onStashWithOptions: vi.fn() } });
    const stash = screen.getByRole("button", { name: "Stash" });
    expect(stash).toBeDisabled();
    expect(stash).toHaveAttribute("title", "A rebase is in progress.");
    expect(screen.getByRole("button", { name: "More stash options" })).toBeDisabled();
  });
});
