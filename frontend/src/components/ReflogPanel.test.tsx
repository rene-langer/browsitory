import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ReflogEntry } from "../ipc/RepoClient";
import { ReflogPanel } from "./ReflogPanel";

const entry: ReflogEntry = {
  reference: "refs/heads/main",
  oldId: "1111111111111111111111111111111111111111",
  newId: "2222222222222222222222222222222222222222",
  committerName: "Ada Lovelace",
  committerEmail: "ada@example.com",
  timestamp: 1_710_000_000,
  message: "commit: add recovery",
  summary: "add recovery",
};

function renderPanel(
  overrides: Partial<Parameters<typeof ReflogPanel>[0]> = {},
) {
  localStorage.removeItem("sidebar-reflog");
  const result = render(
    <ReflogPanel
      references={["HEAD", "refs/heads/main", "refs/remotes/origin/main"]}
      selectedReference="refs/heads/main"
      entries={[entry]}
      onSelectReference={vi.fn().mockResolvedValue(undefined)}
      onRestore={vi.fn().mockResolvedValue(undefined)}
      operationDisabled={false}
      {...overrides}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Reflog" }));
  return result;
}

describe("ReflogPanel", () => {
  it("lists HEAD and local branches with each selected entry's recovery details", () => {
    renderPanel();

    const selector = screen.getByRole("combobox", { name: "Reflog reference" });
    expect(within(selector).getByRole("option", { name: "HEAD" })).toBeInTheDocument();
    expect(within(selector).getByRole("option", { name: "refs/heads/main" })).toBeInTheDocument();
    expect(within(selector).queryByRole("option", { name: "refs/remotes/origin/main" })).not.toBeInTheDocument();
    expect(screen.getByText(`Old ID: ${entry.oldId}`)).toBeInTheDocument();
    expect(screen.getByText(`New ID: ${entry.newId}`)).toBeInTheDocument();
    expect(screen.getByText("Ada Lovelace <ada@example.com>")).toBeInTheDocument();
    expect(screen.getByText(new Date(entry.timestamp * 1000).toLocaleString())).toBeInTheDocument();
    expect(screen.getByText(entry.message)).toBeInTheDocument();
    expect(screen.getByText(entry.summary!)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Restore refs/heads/main to ${entry.newId}` })).toBeEnabled();
  });

  it("requires a named dialog confirmation that identifies the target before restoring", () => {
    const onRestore = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onRestore });

    fireEvent.click(screen.getByRole("button", { name: `Restore refs/heads/main to ${entry.newId}` }));

    expect(onRestore).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog", { name: "Restore refs/heads/main" });
    expect(within(dialog).getByText("Restore refs/heads/main to 2222222222222222222222222222222222222222?")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(onRestore).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: `Restore refs/heads/main to ${entry.newId}` }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Restore refs/heads/main" })).getByRole("button", { name: "Restore reflog entry" }));
    expect(onRestore).toHaveBeenCalledWith("refs/heads/main", entry.newId);
  });

  it("disables restore controls while a repository mutation is pending", () => {
    renderPanel({ operationDisabled: true });

    expect(screen.getByRole("button", { name: `Restore refs/heads/main to ${entry.newId}` })).toBeDisabled();
  });

  it("keeps repeated local-ref transitions distinct with labeled targets and unique restore controls", () => {
    const olderEntry: ReflogEntry = {
      ...entry,
      oldId: "3333333333333333333333333333333333333333",
      newId: "4444444444444444444444444444444444444444",
      timestamp: entry.timestamp - 60,
      message: "commit: older recovery",
      summary: "older recovery",
    };
    renderPanel({ entries: [entry, olderEntry] });

    expect(screen.getByText(`Old ID: ${entry.oldId}`)).toBeInTheDocument();
    expect(screen.getByText(`Old ID: ${olderEntry.oldId}`)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Restore refs/heads/main to ${entry.newId}` })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Restore refs/heads/main to ${olderEntry.newId}` })).toBeInTheDocument();
  });
});
