import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { PatchPreview } from "../ipc/RepoClient";
import { ApplyPatchDialog } from "./ApplyPatchDialog";

const SERIES: PatchPreview = {
  parts: [
    {
      mail: { subject: "Fix the parser", body: "", authorName: "Ada Lovelace", authorEmail: "ada@example.com", timestamp: 0 },
      files: [{ path: "src/parse.rs", oldPath: null, status: "Modified", additions: 3, deletions: 1, binary: false }],
    },
    {
      mail: { subject: "Add a logo", body: "", authorName: "Ada Lovelace", authorEmail: "ada@example.com", timestamp: null },
      files: [
        { path: "logo.png", oldPath: null, status: "Added", additions: 0, deletions: 0, binary: true },
        { path: "b.txt", oldPath: "a.txt", status: "Renamed", additions: 0, deletions: 0, binary: false },
      ],
    },
  ],
  problem: null,
};

const PLAIN: PatchPreview = {
  parts: [{ mail: null, files: [{ path: "a.txt", oldPath: null, status: "Modified", additions: 1, deletions: 1, binary: false }] }],
  problem: null,
};

function renderDialog(props: Partial<Parameters<typeof ApplyPatchDialog>[0]> = {}) {
  const onApply = vi.fn();
  const onCancel = vi.fn();
  render(
    <ApplyPatchDialog
      preview={SERIES}
      hasStagedChanges={false}
      blockedReason={null}
      onApply={onApply}
      onCancel={onCancel}
      {...props}
    />,
  );
  return { onApply, onCancel };
}

describe("ApplyPatchDialog (#658)", () => {
  it("lists each mail's subject, author and files", () => {
    renderDialog();
    const dialog = screen.getByRole("dialog", { name: "Apply patch" });
    expect(within(dialog).getByText("2 commits as git format-patch mails")).toBeInTheDocument();
    expect(within(dialog).getByText("Fix the parser")).toBeInTheDocument();
    expect(within(dialog).getAllByText(/Ada Lovelace <ada@example.com>/)).toHaveLength(2);
    const files = within(dialog).getByRole("list", { name: "Files of Add a logo" });
    expect(within(files).getByText("Binary")).toBeInTheDocument();
    expect(within(files).getByText("a.txt → b.txt")).toBeInTheDocument();
    expect(within(dialog).getByText("+3")).toBeInTheDocument();
    expect(within(dialog).getByText(/each later one is checked when it is applied/)).toBeInTheDocument();
  });

  it("applies the way the button says, and Cancel closes", () => {
    const { onApply, onCancel } = renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    fireEvent.click(screen.getByRole("button", { name: "Apply and stage" }));
    fireEvent.click(screen.getByRole("button", { name: "Apply as 2 commits" }));
    expect(onApply.mock.calls).toEqual([["WorkTree"], ["Stage"], ["Commits"]]);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("offers no commits for a plain diff, nor with staged changes", () => {
    renderDialog({ preview: PLAIN });
    expect(screen.getByText("A plain diff (no author or message)")).toBeInTheDocument();
    const commit = screen.getByRole("button", { name: "Apply as commit" });
    expect(commit).toBeDisabled();
    expect(commit).toHaveAttribute("title", "Only git format-patch mails carry an author and a message to commit with");
    expect(screen.getByRole("button", { name: "Apply" })).toBeEnabled();
  });

  it("disables applying as commits while changes are staged", () => {
    const { onApply } = renderDialog({ hasStagedChanges: true });
    const commits = screen.getByRole("button", { name: "Apply as 2 commits" });
    expect(commits).toBeDisabled();
    expect(commits.getAttribute("title")).toMatch(/staged changes/);
    fireEvent.click(commits);
    expect(onApply).not.toHaveBeenCalled();
  });

  it("says why the patch doesn't apply and offers nothing to apply", () => {
    renderDialog({ preview: { ...SERIES, problem: "The patch names '../x', which is outside the repository." } });
    expect(screen.getByRole("alert")).toHaveTextContent("outside the repository");
    for (const name of ["Apply", "Apply and stage", "Apply as 2 commits"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
  });

  it("disables applying with the reason while the repository is busy", () => {
    renderDialog({ blockedReason: "A rebase is in progress." });
    expect(screen.getByRole("button", { name: "Apply" })).toHaveAttribute("title", "A rebase is in progress.");
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
  });
});
