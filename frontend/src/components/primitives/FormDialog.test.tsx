import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FormDialog } from "./FormDialog";

describe("FormDialog", () => {
  it("renders a labelled dialog and focuses its first field on open", () => {
    render(
      <FormDialog ariaLabel="Edit thing" onCancel={vi.fn()}>
        <form>
          <input aria-label="first" />
          <input aria-label="second" />
        </form>
      </FormDialog>,
    );
    expect(screen.getByRole("dialog", { name: "Edit thing", hidden: true })).toBeInTheDocument();
    expect(screen.getByLabelText("first")).toHaveFocus();
  });

  it("prefers a data-autofocus element over the first field", () => {
    render(
      <FormDialog ariaLabel="Pick" onCancel={vi.fn()}>
        <input aria-label="first" />
        <button type="button" data-autofocus>
          Cancel
        </button>
      </FormDialog>,
    );
    expect(screen.getByRole("button", { name: "Cancel", hidden: true })).toHaveFocus();
  });

  it("routes the native cancel event (Escape) to onCancel without closing itself", () => {
    const onCancel = vi.fn();
    render(
      <FormDialog ariaLabel="Edit thing" onCancel={onCancel}>
        <input aria-label="first" />
      </FormDialog>,
    );
    const dialog = screen.getByRole("dialog", { name: "Edit thing", hidden: true });
    const notPrevented = fireEvent(dialog, new Event("cancel", { cancelable: true }));
    expect(notPrevented).toBe(false);
    expect(onCancel).toHaveBeenCalledOnce();
  });
});
