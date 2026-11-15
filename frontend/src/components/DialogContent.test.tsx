import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { cascadedValue, declarationOf, rulesOf, subjectOf } from "../test/cssCascade";
import { OpenPreferencesContext } from "../state/openPreferences";
import { MergeDialog } from "./MergeDialog";
import { OperationStatusStrip } from "./OperationStatusStrip";
import { PullDivergedDialog } from "./PullDivergedDialog";
import { ResetDialog } from "./ResetDialog";
import { ShortcutSheet } from "./ShortcutSheet";
import cloneCss from "./CloneDialog.module.css?raw";
import shortcutSheetCss from "./ShortcutSheet.module.css?raw";

const classRules = (css: string, className: string) =>
  rulesOf(css).filter(({ selector }) => subjectOf(selector) === `.${className}`);

describe("Reset dialog (#988, #1018)", () => {
  const renderReset = () =>
    render(<ResetDialog branch="main" shortId="abc1234" onReset={vi.fn()} onCancel={vi.fn()} />);

  it("is titled with the branch and the target", () => {
    renderReset();
    const dialog = screen.getByRole("dialog", { name: "Reset main to abc1234" });
    expect(within(dialog).getByRole("heading", { name: "Reset main to abc1234" })).toBeInTheDocument();
  });

  it("asks what happens to the changes, each explanation on its own line", () => {
    renderReset();
    expect(screen.getByRole("group", { name: "What happens to your changes" })).toBeInTheDocument();
    for (const detail of [
      "Keep all changes staged.",
      "Keep changes in the working tree, unstaged.",
      "Discard all changes to tracked files.",
    ]) {
      expect(screen.getByText(detail)).toBeInTheDocument();
    }
  });

  it("names the button after the mode, and marks a hard reset as a danger", () => {
    renderReset();
    expect(screen.getByRole("button", { name: "Reset mixed" })).toHaveAttribute("data-tone", "primary");
    fireEvent.click(screen.getByRole("radio", { name: /Hard/ }));
    const hard = screen.getByRole("button", { name: "Reset hard" });
    expect(hard).toHaveAttribute("data-tone", "danger");
    fireEvent.click(hard);
    const confirm = screen.getByRole("dialog", { name: "Reset hard to abc1234" });
    expect(within(confirm).getByRole("button", { name: "Reset hard" })).toHaveAttribute("data-tone", "danger");
  });
});

describe("Merge dialog (#1018)", () => {
  it("labels the group How to merge and ties the Squash hint to its checkbox", async () => {
    render(
      <MergeDialog
        branchName="feature"
        currentBranchName="main"
        loadOptions={async () => ({ fastForward: "Allow", squash: false })}
        onMerge={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByRole("group", { name: "How to merge" })).toBeInTheDocument();
    const squash = await screen.findByRole("checkbox", { name: "Squash" });
    expect(squash).toHaveAccessibleDescription("Stages the combined changes; you write the commit. The choices above don't apply.");
    await vi.waitFor(() => expect(screen.getByRole("button", { name: "Merge" })).toBeEnabled());
    expect(screen.getByRole("button", { name: "Merge" })).toHaveAttribute("data-tone", "primary");
  });
});

describe("Pull diverged dialog (#1019)", () => {
  it("names the branches in the title, counts the commits, and matches the Pull menu", () => {
    render(
      <PullDivergedDialog
        upstreamRef="refs/remotes/origin/main"
        branchName="main"
        repoName="trailmap"
        ahead={1}
        behind={2}
        operationDisabled={false}
        operationDisabledReason={null}
        onMerge={vi.fn()}
        onRebase={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    const dialog = screen.getByRole("dialog", { name: "main and origin/main have diverged" });
    expect(within(dialog).getByRole("heading", { name: "main and origin/main have diverged" })).toBeInTheDocument();
    expect(dialog).toHaveTextContent("main has 1 commit that origin/main doesn't, and origin/main has 2 that main doesn't.");
    expect(dialog).not.toHaveTextContent(/into it|its commits/);
    expect(within(dialog).getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Pull with merge",
      "Pull with rebase",
      "Cancel",
    ]);
    expect(
      within(dialog).getByRole("checkbox", { name: "Always do this for trailmap (change it in the Pull menu)" }),
    ).not.toBeChecked();
  });
});

describe("danger tone on panes (#988)", () => {
  it("marks Abort merge as a danger", () => {
    render(
      <OperationStatusStrip merging operation="Merge" rebaseProgress={null} conflictCount={1} onAbortMerge={vi.fn()} onAbortRebase={vi.fn()} />,
    );
    expect(screen.getByRole("button", { name: "Abort merge" })).toHaveAttribute("data-tone", "danger");
  });
});

describe("Keyboard shortcut sheet (#1016)", () => {
  it("keeps Change shortcuts… and Close in a footer below the scrolling list", () => {
    render(
      <OpenPreferencesContext.Provider value={vi.fn()}>
        <ShortcutSheet onClose={vi.fn()} />
      </OpenPreferencesContext.Provider>,
    );
    expect(screen.getByRole("heading", { level: 2, name: "Keyboard shortcuts" })).toBeInTheDocument();
    const close = screen.getByRole("button", { name: "Close" });
    const footer = close.closest("footer");
    expect(footer).not.toBeNull();
    expect(within(footer!).getByRole("button", { name: "Change shortcuts…" })).toBeInTheDocument();
  });

  it("sizes the key column to its longest keys instead of a fixed width", () => {
    const list = classRules(shortcutSheetCss, "list");
    expect(cascadedValue(list, "display")).toBe("grid");
    expect(cascadedValue(list, "grid-template-columns")).toMatch(/^max-content\b/);
    expect(shortcutSheetCss).not.toMatch(/12rem/);
  });
});

describe("Clone dialog fits its box (#974)", () => {
  it("asks for no minimum width wider than the dialog's content", () => {
    for (const { body } of classRules(cloneCss, "form")) {
      expect(declarationOf(body, "min-width")).toBeNull();
    }
  });

  it("lets the Parent folder field shrink beside Browse…", () => {
    expect(cascadedValue(classRules(cloneCss, "grow"), "min-width")).toBe("0");
  });
});
