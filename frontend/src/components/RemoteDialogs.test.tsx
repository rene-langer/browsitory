import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import type { RemoteInfo } from "../ipc/RepoClient";
import type { RemoteDialog } from "../state/workspaceDialog";
import { RemoteDialogs } from "./RemoteDialogs";

type Props = Parameters<typeof RemoteDialogs>[0];

const origin: RemoteInfo = {
  name: "origin",
  fetchUrl: "git@github.com:user/repo.git",
  pushUrl: null,
  authMode: null,
  authUsername: null,
};

function props(overrides: Partial<Props> = {}): Props {
  return {
    remoteDialog: null,
    remotes: [origin],
    remoteUpstreams: {},
    blockedReason: null,
    onRenameRemote: vi.fn().mockResolvedValue(true),
    onUpdateRemoteUrls: vi.fn().mockResolvedValue(undefined),
    onRemoveRemote: vi.fn().mockResolvedValue(undefined),
    onSaveHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onForgetHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onSetRemoteAuthMode: vi.fn().mockResolvedValue(true),
    onClose: vi.fn(),
    onRemoteGone: vi.fn(),
    ...overrides,
  };
}

/**
 * Owns `remotes` and the open dialog like `RepoWorkspace` does, and lists the remote names so a test can
 * wait for a rollback. `remove`/`rename` behave like the optimistic app-state actions: `remotes`
 * changes before the request settles, and a failure rolls it back.
 */
function Harness({
  initial,
  result,
  onRemoteGone,
  onUpdateRemoteUrls = vi.fn().mockResolvedValue(undefined),
  externalChange,
}: {
  initial: RemoteDialog;
  result: boolean;
  onRemoteGone: (name: string) => void;
  onUpdateRemoteUrls?: Props["onUpdateRemoteUrls"];
  externalChange?: (setRemotes: (remotes: RemoteInfo[]) => void) => void;
}) {
  const [remotes, setRemotes] = useState([origin]);
  const [dialog, setDialog] = useState<RemoteDialog | null>(initial);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
  return (
    <>
      <ul aria-label="Remotes">
        {remotes.map((r) => (
          <li key={r.name}>{r.name}</li>
        ))}
      </ul>
      {externalChange && (
        <button type="button" onClick={() => externalChange(setRemotes)}>
          External change
        </button>
      )}
      <RemoteDialogs
        {...props({
          remotes,
          remoteDialog: dialog,
          onClose: () => setDialog(null),
          onRemoteGone,
          onUpdateRemoteUrls,
          onRenameRemote: async (oldName, newName) => {
            setRemotes((prev) => prev.map((r) => (r.name === oldName ? { ...r, name: newName } : r)));
            await settle();
            if (!result) setRemotes((prev) => prev.map((r) => (r.name === newName ? { ...r, name: oldName } : r)));
            return result;
          },
          onRemoveRemote: async (name) => {
            setRemotes((prev) => prev.filter((r) => r.name !== name));
            await settle();
            if (!result) setRemotes([origin]);
          },
        })}
      />
    </>
  );
}

describe("RemoteDialogs", () => {
  it.each([
    ["edit", "Edit origin"],
    ["credentials", "Credentials for origin"],
    ["remove", "Remove remote confirmation"],
  ] as const)("renders the %s dialog for the open remote dialog", async (kind, name) => {
    render(<RemoteDialogs {...props({ remoteDialog: { kind: "remote", action: kind, remoteName: "origin" } })} />);
    expect(await screen.findByRole("dialog", { name, hidden: true })).toBeInTheDocument();
  });

  it("closing the dialog reports onClose", () => {
    const onClose = vi.fn();
    render(<RemoteDialogs {...props({ remoteDialog: { kind: "remote", action: "edit", remoteName: "origin" }, onClose })} />);
    fireEvent.click(screen.getByRole("button", { name: "Cancel", hidden: true }));
    expect(onClose).toHaveBeenCalled();
  });

  it("asks to clear upstreams based on the upstreams when the confirmation renders", () => {
    const base = props({ remoteDialog: { kind: "remote", action: "remove", remoteName: "origin" } });
    const { rerender } = render(<RemoteDialogs {...base} />);
    expect(screen.getByText("Remove remote origin?")).toBeInTheDocument();
    rerender(
      <RemoteDialogs
        {...base}
        remoteUpstreams={{ origin: [{ localBranch: "main", remoteName: "origin", remoteBranch: "main" }] }}
      />,
    );
    expect(screen.getByText(/clear upstreams for main/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Confirm remove", hidden: true }));
    expect(base.onRemoveRemote).toHaveBeenCalledWith("origin", true);
  });

  // #561: the repository can become busy while a dialog is open.
  it("disables Remove with the reason, in the confirmation too", () => {
    const base = props({ remoteDialog: { kind: "remote", action: "remove", remoteName: "origin" } });
    const { rerender } = render(<RemoteDialogs {...base} />);
    rerender(<RemoteDialogs {...base} blockedReason="A merge is in progress." />);
    const confirm = screen.getByRole("button", { name: "Confirm remove", hidden: true });
    expect(confirm).toBeDisabled();
    expect(confirm).toHaveAttribute("title", "A merge is in progress.");
    expect(screen.getByText("A merge is in progress.")).toBeInTheDocument();
    fireEvent.click(confirm);
    expect(base.onRemoveRemote).not.toHaveBeenCalled();
  });

  it("does not save an edit once the repository became busy", () => {
    const base = props({ remoteDialog: { kind: "remote", action: "edit", remoteName: "origin" } });
    const { rerender } = render(<RemoteDialogs {...base} />);
    const dialog = screen.getByRole("dialog", { name: "Edit origin", hidden: true });
    fireEvent.change(within(dialog).getByLabelText("Remote name"), { target: { value: "upstream" } });
    fireEvent.change(within(dialog).getByLabelText("Fetch URL"), { target: { value: "x" } });
    rerender(<RemoteDialogs {...base} blockedReason="A transfer is in progress." />);
    const save = within(dialog).getByRole("button", { name: "Save remote", hidden: true });
    expect(save).toBeDisabled();
    expect(save).toHaveAttribute("title", "A transfer is in progress.");
    expect(within(dialog).getByText("A transfer is in progress.")).toBeInTheDocument();
    fireEvent.submit(save.closest("form")!);
    expect(base.onRenameRemote).not.toHaveBeenCalled();
    expect(base.onUpdateRemoteUrls).not.toHaveBeenCalled();
    expect(base.onClose).not.toHaveBeenCalled();
  });

  it("does not change credentials once the repository became busy", () => {
    const base = props({ remoteDialog: { kind: "remote", action: "credentials", remoteName: "origin" } });
    const { rerender } = render(<RemoteDialogs {...base} />);
    const dialog = screen.getByRole("dialog", { name: "Credentials for origin", hidden: true });
    fireEvent.change(within(dialog).getByLabelText("HTTPS username"), { target: { value: "me" } });
    fireEvent.change(within(dialog).getByLabelText("Access token"), { target: { value: "tok" } });
    rerender(<RemoteDialogs {...base} blockedReason="A rebase is in progress." />);
    for (const name of ["Save HTTPS credential", "Forget HTTPS credential"]) {
      const button = within(dialog).getByRole("button", { name, hidden: true });
      expect(button).toBeDisabled();
      expect(button).toHaveAttribute("title", "A rebase is in progress.");
    }
    fireEvent.submit(within(dialog).getByLabelText("Access token").closest("form")!);
    expect(base.onSetRemoteAuthMode).not.toHaveBeenCalled();
    expect(base.onSaveHttpsCredential).not.toHaveBeenCalled();
    expect(base.onClose).not.toHaveBeenCalled();
    // Not an attempt: the token stays for when the repository is free again.
    expect(within(dialog).getByLabelText("Access token")).toHaveValue("tok");
  });

  it("closes with a notice when opened for a remote that isn't listed", () => {
    const onClose = vi.fn();
    const onRemoteGone = vi.fn();
    render(
      <RemoteDialogs {...props({ remotes: [], remoteDialog: { kind: "remote", action: "edit", remoteName: "origin" }, onClose, onRemoteGone })} />,
    );
    expect(screen.queryByRole("dialog", { hidden: true })).toBeNull();
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onRemoteGone).toHaveBeenCalledWith("origin");
  });

  it.each([
    ["edit", "Edit origin", "removed"],
    ["credentials", "Credentials for origin", "removed"],
    ["remove", "Remove remote confirmation", "removed"],
    ["edit", "Edit origin", "renamed"],
    ["credentials", "Credentials for origin", "renamed"],
  ] as const)("the %s dialog closes with a notice when its remote is %s elsewhere", async (kind, name, change) => {
    const onRemoteGone = vi.fn();
    render(
      <Harness
        initial={{ kind: "remote", action: kind, remoteName: "origin" }}
        result
        onRemoteGone={onRemoteGone}
        externalChange={(setRemotes) => setRemotes(change === "removed" ? [] : [{ ...origin, name: "upstream" }])}
      />,
    );
    expect(screen.getByRole("dialog", { name, hidden: true })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "External change", hidden: true }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name, hidden: true })).toBeNull());
    expect(onRemoteGone).toHaveBeenCalledTimes(1);
    expect(onRemoteGone).toHaveBeenCalledWith("origin");
  });

  // `renameRemote` is optimistic: `remotes` already holds the new name while the request runs, and
  // a failure rolls it back. The dialog must stay mounted with what the user typed throughout, and
  // its own rename is not an outside change.
  it.each([true, false])("the Edit dialog stays open with the typed values while a rename runs (succeeds: %s)", async (result) => {
    const updateUrls = vi.fn().mockResolvedValue(undefined);
    const onRemoteGone = vi.fn();
    render(
      <Harness
        initial={{ kind: "remote", action: "edit", remoteName: "origin" }}
        result={result}
        onRemoteGone={onRemoteGone}
        onUpdateRemoteUrls={updateUrls}
      />,
    );
    const dialog = screen.getByRole("dialog", { name: "Edit origin", hidden: true });
    fireEvent.change(within(dialog).getByLabelText("Remote name"), { target: { value: "upstream" } });
    fireEvent.change(within(dialog).getByLabelText("Fetch URL"), { target: { value: "../new.git" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote", hidden: true }));
    // Mid-rename (remotes already renamed optimistically): same dialog, same typed values.
    const list = screen.getByRole("list", { name: "Remotes", hidden: true });
    expect(within(list).getByText("upstream")).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Edit origin", hidden: true })).toBe(dialog);
    expect(within(dialog).getByLabelText("Fetch URL")).toHaveValue("../new.git");
    if (result) {
      await waitFor(() => expect(screen.queryByRole("dialog", { name: /^Edit /, hidden: true })).toBeNull());
      expect(updateUrls).toHaveBeenCalledWith("upstream", "../new.git", null);
    } else {
      // Rolled back: the list shows "origin" again.
      await waitFor(() => expect(within(list).getByText("origin")).toBeInTheDocument());
      expect(screen.getByRole("dialog", { name: "Edit origin", hidden: true })).toBe(dialog);
      expect(within(dialog).getByLabelText("Remote name")).toHaveValue("upstream");
      expect(within(dialog).getByLabelText("Fetch URL")).toHaveValue("../new.git");
      expect(updateUrls).not.toHaveBeenCalled();
    }
    expect(onRemoteGone).not.toHaveBeenCalled();
  });

  it("after a failed rename, an outside removal still closes the dialog with a notice", async () => {
    const onRemoteGone = vi.fn();
    render(
      <Harness
        initial={{ kind: "remote", action: "edit", remoteName: "origin" }}
        result={false}
        onRemoteGone={onRemoteGone}
        externalChange={(setRemotes) => setRemotes([])}
      />,
    );
    const dialog = screen.getByRole("dialog", { name: "Edit origin", hidden: true });
    fireEvent.change(within(dialog).getByLabelText("Remote name"), { target: { value: "upstream" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote", hidden: true }));
    const list = screen.getByRole("list", { name: "Remotes", hidden: true });
    expect(within(list).getByText("upstream")).toBeInTheDocument();
    await waitFor(() => expect(within(list).getByText("origin")).toBeInTheDocument());
    // Flush the failed rename's continuation (it runs right after the rollback).
    await new Promise((resolve) => setTimeout(resolve, 0));
    fireEvent.click(screen.getByRole("button", { name: "External change", hidden: true }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Edit origin", hidden: true })).toBeNull());
    expect(onRemoteGone).toHaveBeenCalledWith("origin");
  });

  it.each([true, false])("confirming Remove closes without a notice (succeeds: %s)", async (result) => {
    const onRemoteGone = vi.fn();
    render(<Harness initial={{ kind: "remote", action: "remove", remoteName: "origin" }} result={result} onRemoteGone={onRemoteGone} />);
    fireEvent.click(screen.getByRole("button", { name: "Confirm remove", hidden: true }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Remove remote confirmation", hidden: true })).toBeNull(),
    );
    expect(onRemoteGone).not.toHaveBeenCalled();
  });
});
