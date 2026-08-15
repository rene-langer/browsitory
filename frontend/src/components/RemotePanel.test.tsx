import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, expectTypeOf, it, vi } from "vitest";
import type { RemoteInfo, RepoClient, UpstreamInfo } from "../ipc/RepoClient";
import { RemotePanel } from "./RemotePanel";

type IsOptional<T, K extends keyof T> = Pick<T, K> extends Required<Pick<T, K>> ? false : true;

const origin: RemoteInfo = {
  name: "origin",
  fetchUrl: "../origin.git",
  pushUrl: "../push-origin.git",
  authMode: null,
  authUsername: null,
};

const backup: RemoteInfo = {
  name: "backup",
  fetchUrl: "../backup.git",
  pushUrl: "../push-backup.git",
  authMode: null,
  authUsername: null,
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
      remoteUpstreams={{ origin: [] }}
      onAddRemote={vi.fn()}
      onRenameRemote={vi.fn().mockResolvedValue(true)}
      onUpdateRemoteUrls={vi.fn()}
      onRemoveRemote={vi.fn().mockResolvedValue(undefined)}
      onSaveHttpsCredential={vi.fn().mockResolvedValue(undefined)}
      onForgetHttpsCredential={vi.fn().mockResolvedValue(undefined)}
      onSetRemoteAuthMode={vi.fn().mockResolvedValue(true)}
      onSetUpstream={vi.fn()}
      onClearUpstream={vi.fn()}
      onFetchRemote={vi.fn()}
      fetchDisabled={false}
      onPushCurrentBranch={vi.fn()}
      pushDisabled={false}
      onPull={vi.fn()}
      pullDisabled={false}
      pendingPull={null}
      pullOutcome={null}
      onMergePull={vi.fn()}
      onRebasePull={vi.fn()}
      onCancelPull={vi.fn()}
      {...overrides}
    />,
  );
}

describe("RemotePanel", () => {
  it("submits the token only to the save callback and clears the input", async () => {
    const onSaveHttpsCredential = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onSaveHttpsCredential });

    fireEvent.click(screen.getByRole("button", { name: "Credentials for origin" }));
    fireEvent.change(screen.getByLabelText("HTTPS username"), { target: { value: "rene" } });
    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "token-123" } });
    fireEvent.click(screen.getByRole("button", { name: "Save HTTPS credential" }));

    await waitFor(() => {
      expect(onSaveHttpsCredential).toHaveBeenCalledWith("origin", "rene", "token-123");
    });
    expect(screen.getByLabelText("Access token")).toHaveValue("");
  });

  it("uses the SSH agent without rendering a token input", async () => {
    const onSetRemoteAuthMode = vi.fn().mockResolvedValue(true);
    renderPanel({ onSetRemoteAuthMode });

    fireEvent.click(screen.getByRole("button", { name: "Credentials for origin" }));
    fireEvent.change(screen.getByLabelText("Authentication for origin"), {
      target: { value: "SshAgent" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Use SSH agent" }));

    await waitFor(() => {
      expect(onSetRemoteAuthMode).toHaveBeenCalledWith("origin", "SshAgent", null);
    });
    expect(screen.queryByLabelText("Access token")).not.toBeInTheDocument();
  });

  it("clears the token after a failed credential save", async () => {
    const onSaveHttpsCredential = vi.fn().mockRejectedValue(new Error("keychain unavailable"));
    renderPanel({ onSaveHttpsCredential });

    fireEvent.click(screen.getByRole("button", { name: "Credentials for origin" }));
    fireEvent.change(screen.getByLabelText("HTTPS username"), { target: { value: "rene" } });
    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "token-123" } });
    fireEvent.click(screen.getByRole("button", { name: "Save HTTPS credential" }));

    await waitFor(() => {
      expect(screen.getByLabelText("Access token")).toHaveValue("");
    });
  });

  it("does not save a token when HTTPS authentication setup fails", async () => {
    const onSetRemoteAuthMode = vi.fn().mockResolvedValue(false);
    const onSaveHttpsCredential = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onSetRemoteAuthMode, onSaveHttpsCredential });

    fireEvent.click(screen.getByRole("button", { name: "Credentials for origin" }));
    fireEvent.change(screen.getByLabelText("HTTPS username"), { target: { value: "rene" } });
    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "token-123" } });
    fireEvent.click(screen.getByRole("button", { name: "Save HTTPS credential" }));

    await waitFor(() => {
      expect(onSetRemoteAuthMode).toHaveBeenCalledWith("origin", "HttpsToken", "rene");
    });
    expect(onSaveHttpsCredential).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Access token")).toHaveValue("");
  });

  it("pushes the current branch to a chosen remote and disables Push during an operation", () => {
    const onPushCurrentBranch = vi.fn();
    renderPanel({ onPushCurrentBranch, pushDisabled: true });

    expect(screen.getByRole("button", { name: "Push branch to origin" })).toBeDisabled();

    renderPanel({ onPushCurrentBranch, pushDisabled: false });
    screen.getAllByRole("button", { name: "Push branch to origin" })[1].click();

    expect(onPushCurrentBranch).toHaveBeenCalledWith("origin");
  });

  it("fetches the selected remote and disables Fetch while an operation is active", () => {
    const onFetchRemote = vi.fn();
    renderPanel({ onFetchRemote, fetchDisabled: true });

    expect(screen.getByRole("button", { name: "Fetch origin" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add remote" })).toBeDisabled();

    renderPanel({ onFetchRemote, fetchDisabled: false });
    screen.getAllByRole("button", { name: "Fetch origin" })[1].click();
    expect(screen.getAllByRole("button", { name: "Add remote" })[1]).toBeEnabled();

    expect(onFetchRemote).toHaveBeenCalledWith("origin");
  });

  it("offers merge or rebase only after a divergent pull", () => {
    const onMergePull = vi.fn();
    const onRebasePull = vi.fn();
    const onCancelPull = vi.fn();
    renderPanel({
      pendingPull: { upstreamRef: "refs/remotes/origin/main" },
      onMergePull,
      onRebasePull,
      onCancelPull,
    });

    const dialog = screen.getByRole("dialog", { name: "Pull has diverged" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Merge" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Rebase" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(onMergePull).toHaveBeenCalledWith("refs/remotes/origin/main");
    expect(onRebasePull).toHaveBeenCalledWith("refs/remotes/origin/main");
    expect(onCancelPull).toHaveBeenCalledOnce();
  });

  it("keeps Pull disabled while a reconciliation choice is open and focuses Cancel", async () => {
    renderPanel({
      upstream,
      pendingPull: { upstreamRef: "refs/remotes/origin/main" },
    });

    expect(screen.getByRole("button", { name: "Pull" })).toBeDisabled();
    const dialog = screen.getByRole("dialog", { name: "Pull has diverged" });
    await waitFor(() => {
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    });
  });

  it("disables divergent pull choices while another repository operation is active", () => {
    renderPanel({
      upstream,
      pullDisabled: true,
      pendingPull: { upstreamRef: "refs/remotes/origin/main" },
    });

    const dialog = screen.getByRole("dialog", { name: "Pull has diverged" });
    expect(within(dialog).getByRole("button", { name: "Merge" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Rebase" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeEnabled();
  });

  it("renders a successful up-to-date pull outcome", () => {
    renderPanel({ upstream, pullOutcome: { kind: "UpToDate" } });

    expect(screen.getByRole("status")).toHaveTextContent("Already up to date.");
  });

  it("requires upstream discovery in both the client and panel contracts", () => {
    expectTypeOf<IsOptional<RepoClient, "getRemoteUpstreams">>().toEqualTypeOf<false>();
    expectTypeOf<
      IsOptional<Parameters<typeof RemotePanel>[0], "remoteUpstreams">
    >().toEqualTypeOf<false>();
  });

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

  it("does not update an existing remote's URLs when rename fails", async () => {
    const onRenameRemote = vi.fn().mockResolvedValue(false);
    const onUpdateRemoteUrls = vi.fn();
    renderPanel({ remotes: [origin, backup], onRenameRemote, onUpdateRemoteUrls });

    fireEvent.click(screen.getByRole("button", { name: "Edit origin" }));
    const editForm = screen.getByRole("form", { name: "Edit origin" });
    fireEvent.change(within(editForm).getByLabelText("Remote name"), {
      target: { value: "backup" },
    });
    fireEvent.change(within(editForm).getByLabelText("Fetch URL"), {
      target: { value: "../replacement.git" },
    });
    fireEvent.change(within(editForm).getByLabelText("Push URL (optional)"), {
      target: { value: "../replacement-push.git" },
    });
    fireEvent.click(within(editForm).getByRole("button", { name: "Save remote" }));

    await waitFor(() => expect(onRenameRemote).toHaveBeenCalledWith("origin", "backup"));
    expect(onUpdateRemoteUrls).not.toHaveBeenCalled();
    const backupItem = screen.getByText("backup", { selector: "strong" }).closest("li");
    expect(backupItem).not.toBeNull();
    expect(within(backupItem!).getByText("Fetch: ../backup.git")).toBeInTheDocument();
    expect(within(backupItem!).getByText("Push: ../push-backup.git")).toBeInTheDocument();
  });

  it("shows the explicit all-branch removal route for a remote with upstreams", async () => {
    const onRemoveRemote = vi.fn().mockResolvedValue(undefined);
    const onClearUpstream = vi.fn().mockResolvedValue(undefined);
    const topicUpstream: UpstreamInfo = {
      localBranch: "topic",
      remoteName: "origin",
      remoteBranch: "topic",
    };
    renderPanel({
      upstream,
      remoteUpstreams: { origin: [upstream, topicUpstream] },
      onRemoveRemote,
      onClearUpstream,
    });

    fireEvent.click(screen.getByRole("button", { name: "Remove origin" }));

    expect(screen.getByText(/clear upstreams for main, topic/i)).toBeInTheDocument();
    expect(onRemoveRemote).not.toHaveBeenCalled();

    fireEvent.click(within(screen.getByRole("alertdialog", { name: "Remove remote confirmation" })).getByRole("button", { name: "Confirm remove" }));
    expect(onRemoveRemote).toHaveBeenCalledWith("origin", true);
  });

  it("removes a remote after explicit confirmation when it has no upstream", () => {
    const onRemoveRemote = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onRemoveRemote });

    fireEvent.click(screen.getByRole("button", { name: "Remove origin" }));
    expect(screen.getByText(/remove remote origin/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Confirm remove" }));

    expect(onRemoveRemote).toHaveBeenCalledWith("origin", false);
  });
});
