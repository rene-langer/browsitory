import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import type { RemoteInfo } from "../ipc/RepoClient";
import type { RemoteDialog } from "../state/useAppState";
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
    operationDisabled: false,
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
 * Owns `remotes` and the open dialog like `App` does, and lists the remote names so a test can
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
    render(<RemoteDialogs {...props({ remoteDialog: { kind, remoteName: "origin" } })} />);
    expect(await screen.findByRole("dialog", { name, hidden: true })).toBeInTheDocument();
  });

  it("closing the dialog reports onClose", () => {
    const onClose = vi.fn();
    render(<RemoteDialogs {...props({ remoteDialog: { kind: "edit", remoteName: "origin" }, onClose })} />);
    fireEvent.click(screen.getByRole("button", { name: "Cancel", hidden: true }));
    expect(onClose).toHaveBeenCalled();
  });

  it("asks to clear upstreams based on the upstreams when the confirmation renders", () => {
    const base = props({ remoteDialog: { kind: "remove", remoteName: "origin" } });
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

  it("closes with a notice when opened for a remote that isn't listed", () => {
    const onClose = vi.fn();
    const onRemoteGone = vi.fn();
    render(
      <RemoteDialogs {...props({ remotes: [], remoteDialog: { kind: "edit", remoteName: "origin" }, onClose, onRemoteGone })} />,
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
        initial={{ kind, remoteName: "origin" }}
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
        initial={{ kind: "edit", remoteName: "origin" }}
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
        initial={{ kind: "edit", remoteName: "origin" }}
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
    render(<Harness initial={{ kind: "remove", remoteName: "origin" }} result={result} onRemoteGone={onRemoteGone} />);
    fireEvent.click(screen.getByRole("button", { name: "Confirm remove", hidden: true }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Remove remote confirmation", hidden: true })).toBeNull(),
    );
    expect(onRemoteGone).not.toHaveBeenCalled();
  });
});
