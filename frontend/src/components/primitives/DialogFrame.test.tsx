import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { cascadedValue, rulesOf, subjectOf } from "../../test/cssCascade";
import { ConfirmDialog } from "./ConfirmDialog";
import { DialogActions } from "./DialogActions";
import { DialogHeader } from "./DialogHeader";
import { FormDialog } from "./FormDialog";
import dialogActionsCss from "./DialogActions.module.css?raw";
import formDialogCss from "./FormDialog.module.css?raw";

// Every dialog's source, to check that each one is built from the shared frame (#987).
const sources = import.meta.glob<string>(["../../**/*.tsx", "!../../**/*.test.tsx"], {
  query: "?raw",
  import: "default",
  eager: true,
});
const formDialogFiles = Object.entries(sources).filter(
  ([path, source]) => source.includes("<FormDialog") && !path.endsWith("/FormDialog.tsx"),
);
const confirmDialogFiles = Object.entries(sources).filter(
  ([path, source]) => source.includes("<ConfirmDialog") && !path.endsWith("/ConfirmDialog.tsx"),
);

describe("DialogHeader (#987)", () => {
  it("shows the title as the dialog's heading, with an optional line under it", () => {
    render(<DialogHeader title="Push main" description="To origin" />);
    expect(screen.getByRole("heading", { level: 2, name: "Push main" })).toBeInTheDocument();
    expect(screen.getByText("To origin")).toBeInTheDocument();
  });
});

describe("DialogActions (#987)", () => {
  it("holds the dialog's buttons in order", () => {
    render(
      <DialogActions>
        <button type="button">Push</button>
        <button type="button">Cancel</button>
      </DialogActions>,
    );
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual(["Push", "Cancel"]);
  });

  it("puts every dialog's buttons at the right end of one row", () => {
    const actions = rulesOf(dialogActionsCss).filter(({ selector }) => subjectOf(selector) === ".actions");
    expect(cascadedValue(actions, "display")).toBe("flex");
    expect(cascadedValue(actions, "justify-content")).toBe("flex-end");
  });

  it("leaves no rule that puts a form's buttons one per line", () => {
    expect(rulesOf(formDialogCss).filter(({ selector }) => /form\s*>\s*button/.test(selector))).toEqual([]);
  });
});

describe("FormDialog's title (#987)", () => {
  it("shows the title as a visible heading and names the dialog with it", () => {
    render(
      <FormDialog title="Set upstream for main" description="Choose a remote branch." onCancel={vi.fn()}>
        <input aria-label="first" />
      </FormDialog>,
    );
    const dialog = screen.getByRole("dialog", { name: "Set upstream for main", hidden: true });
    expect(within(dialog).getByRole("heading", { level: 2, name: "Set upstream for main", hidden: true })).toBeInTheDocument();
    expect(within(dialog).getByText("Choose a remote branch.")).toBeInTheDocument();
  });

  it.each(formDialogFiles.map(([path, source]) => ({ path, source })))(
    "$path uses the shared title and button row",
    ({ source }) => {
      expect(source).not.toMatch(/<FormDialog[^>]*ariaLabel=/);
      expect(source).toMatch(/<FormDialog[^>]*\btitle=/);
      // The title comes from FormDialog: no hand-rolled heading next to it.
      expect(source).not.toMatch(/<h2\b/);
      expect(source).toContain("<DialogActions");
    },
  );
});

describe("a dialog's name and description come from its heading and text (#987)", () => {
  it("FormDialog: labelled by its heading, described by the line under it", () => {
    render(
      <FormDialog title="Set upstream for main" description="Choose a remote branch." onCancel={vi.fn()}>
        <input aria-label="first" />
      </FormDialog>,
    );
    const dialog = screen.getByRole("dialog", { name: "Set upstream for main", hidden: true });
    const heading = within(dialog).getByRole("heading", { level: 2, hidden: true });
    expect(heading.id).not.toBe("");
    expect(dialog).toHaveAttribute("aria-labelledby", heading.id);
    expect(dialog).toHaveAccessibleDescription("Choose a remote branch.");
  });

  it("FormDialog without a description has none", () => {
    render(
      <FormDialog title="Rename tab" onCancel={vi.fn()}>
        <input aria-label="first" />
      </FormDialog>,
    );
    expect(screen.getByRole("dialog", { name: "Rename tab", hidden: true })).not.toHaveAttribute("aria-describedby");
  });

  it("ConfirmDialog: labelled by its heading, described by its message", () => {
    render(
      <ConfirmDialog
        title="Delete tag v1"
        message={<p>Undo brings it back.</p>}
        confirmLabel="Delete tag"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    const dialog = screen.getByRole("dialog", { name: "Delete tag v1" });
    const heading = within(dialog).getByRole("heading", { level: 2 });
    expect(heading.id).not.toBe("");
    expect(dialog).toHaveAttribute("aria-labelledby", heading.id);
    expect(dialog).toHaveAccessibleDescription("Undo brings it back.");
  });
});

describe("ConfirmDialog's title and tone (#987, #988)", () => {
  it("shows the title as a visible heading", () => {
    render(<ConfirmDialog title="Delete tag v1" message="Undo brings it back." confirmLabel="Delete tag" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    const dialog = screen.getByRole("dialog", { name: "Delete tag v1" });
    expect(within(dialog).getByRole("heading", { level: 2, name: "Delete tag v1" })).toBeInTheDocument();
  });

  it("marks the confirm button as the primary action by default, not as a danger", () => {
    render(<ConfirmDialog title="Run build" message="Runs it." confirmLabel="Run" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Run" })).toHaveAttribute("data-tone", "primary");
    expect(screen.getByRole("button", { name: "Cancel" })).not.toHaveAttribute("data-tone");
  });

  it("marks the confirm button as a danger with tone danger", () => {
    render(
      <ConfirmDialog title="Drop stash wip" message="Drops it." confirmLabel="Drop stash" tone="danger" onConfirm={vi.fn()} onCancel={vi.fn()} />,
    );
    expect(screen.getByRole("button", { name: "Drop stash" })).toHaveAttribute("data-tone", "danger");
  });

  it.each(confirmDialogFiles.map(([path, source]) => ({ path, source })))("$path gives each confirmation a title", ({ source }) => {
    expect(source).not.toMatch(/<ConfirmDialog[^>]*ariaLabel=/);
    const opened = source.split("<ConfirmDialog").length - 1;
    const titled = source.match(/<ConfirmDialog\s+title=/g)?.length ?? 0;
    expect(titled).toBe(opened);
  });
});

describe("the danger and primary tones (#988)", () => {
  it("styles a danger button and a primary button app-wide", async () => {
    const indexCss = (await import("../../index.css?raw")).default;
    const rules = rulesOf(indexCss);
    const danger = rules.filter(({ selector }) => selector.startsWith("button") && selector.includes('[data-tone="danger"]'));
    const primary = rules.filter(({ selector }) => selector.startsWith("button") && selector.includes('[data-tone="primary"]'));
    expect(danger.map(({ body }) => body).join("")).toContain("var(--color-danger-text)");
    expect(primary.map(({ body }) => body).join("")).toContain("var(--color-accent)");
  });

  it("fills a danger button in a dialog's button row", () => {
    const filled = rulesOf(dialogActionsCss).filter(({ selector }) => selector.includes('[data-tone="danger"]'));
    expect(filled.map(({ body }) => body).join("")).toMatch(/background:\s*var\(--color-danger-text\)/);
  });
});
