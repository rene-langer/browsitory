import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";
import { RemotePanel } from "./RemotePanel";

const origin: RemoteInfo = {
  name: "origin",
  fetchUrl: "../origin.git",
  pushUrl: "../push-origin.git",
};

const upstream: UpstreamInfo = {
  localBranch: "main",
  remoteName: "origin",
  remoteBranch: "main",
};

function renderPanel(overrides: Partial<Parameters<typeof RemotePanel>[0]> = {}) {
  return render(
    <RemotePanel
      remotes={[origin]}
      upstream={null}
      onAddRemote={vi.fn()}
      onRenameRemote={vi.fn()}
      onUpdateRemoteUrls={vi.fn()}
      onRemoveRemote={vi.fn()}
      onSetUpstream={vi.fn()}
      onClearUpstream={vi.fn()}
      {...overrides}
    />,
  );
}

describe("RemotePanel", () => {
  it("adds a remote using the labelled URL form", () => {
    const onAddRemote = vi.fn();
    renderPanel({ onAddRemote });

    fireEvent.change(screen.getByLabelText("Remote name"), { target: { value: "backup" } });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    fireEvent.change(screen.getByLabelText("Push URL (optional)"), { target: { value: "../push-backup.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));

    expect(onAddRemote).toHaveBeenCalledWith("backup", "../backup.git", "../push-backup.git");
  });

  it("sets the current branch upstream from the selected remote and branch", () => {
    const onSetUpstream = vi.fn();
    renderPanel({ onSetUpstream });

    fireEvent.change(screen.getByLabelText("Upstream remote"), { target: { value: "origin" } });
    fireEvent.change(screen.getByLabelText("Upstream branch"), { target: { value: "main" } });
    fireEvent.click(screen.getByRole("button", { name: "Set upstream" }));

    expect(onSetUpstream).toHaveBeenCalledWith("origin", "main");
  });

  it("keeps the removal dialog open until clearing the upstream completes", async () => {
    const onRemoveRemote = vi.fn();
    let resolveClear!: () => void;
    const onClearUpstream = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveClear = resolve;
        }),
    );
    renderPanel({ upstream, onRemoveRemote, onClearUpstream });

    fireEvent.click(screen.getByRole("button", { name: "Remove origin" }));

    expect(screen.getByText(/clear main's upstream/i)).toBeInTheDocument();
    expect(onRemoveRemote).not.toHaveBeenCalled();

    fireEvent.click(
      within(screen.getByRole("alertdialog", { name: "Remove remote confirmation" })).getByRole("button", {
        name: "Clear upstream",
      }),
    );
    expect(onClearUpstream).toHaveBeenCalledOnce();
    expect(screen.getByRole("alertdialog", { name: "Remove remote confirmation" })).toBeInTheDocument();

    await act(async () => {
      resolveClear();
    });
    expect(screen.queryByRole("alertdialog", { name: "Remove remote confirmation" })).not.toBeInTheDocument();
  });

  it("removes a remote after explicit confirmation when it has no upstream", () => {
    const onRemoveRemote = vi.fn();
    renderPanel({ onRemoveRemote });

    fireEvent.click(screen.getByRole("button", { name: "Remove origin" }));
    expect(screen.getByText(/remove remote origin/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Confirm remove" }));

    expect(onRemoveRemote).toHaveBeenCalledWith("origin");
  });
});
