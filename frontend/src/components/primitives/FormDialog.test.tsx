import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FormDialog } from "./FormDialog";

describe("FormDialog", () => {
  it("renders a labelled dialog and focuses its first field on open", () => {
    render(
      <FormDialog title="Edit thing" onCancel={vi.fn()}>
        <form>
          <input aria-label="first" />
          <input aria-label="second" />
        </form>
      </FormDialog>,
    );
    expect(screen.getByRole("dialog", { name: "Edit thing", hidden: true })).toBeInTheDocument();
    expect(screen.getByLabelText("first")).toHaveFocus();
  });

  it("starts on the checked radio of a group, not on the frame or the group's first radio (#1029)", () => {
    render(
      <FormDialog title="Reset" onCancel={vi.fn()}>
        <form>
          <fieldset>
            <legend>Mode</legend>
            <input type="radio" name="mode" aria-label="Soft" />
            <input type="radio" name="mode" aria-label="Mixed" defaultChecked />
          </fieldset>
          <button type="submit">Reset</button>
        </form>
      </FormDialog>,
    );
    expect(screen.getByLabelText("Mixed")).toHaveFocus();
  });

  it("skips a disabled or hidden field for the first enabled one (#1029)", () => {
    render(
      <FormDialog title="Pick" onCancel={vi.fn()}>
        <input type="hidden" name="token" />
        <input aria-label="off" disabled />
        <input aria-label="on" />
      </FormDialog>,
    );
    expect(screen.getByLabelText("on")).toHaveFocus();
  });

  it("prefers a data-autofocus element over the first field", () => {
    render(
      <FormDialog title="Pick" onCancel={vi.fn()}>
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
      <FormDialog title="Edit thing" onCancel={onCancel}>
        <input aria-label="first" />
      </FormDialog>,
    );
    const dialog = screen.getByRole("dialog", { name: "Edit thing", hidden: true });
    const notPrevented = fireEvent(dialog, new Event("cancel", { cancelable: true }));
    expect(notPrevented).toBe(false);
    expect(onCancel).toHaveBeenCalledOnce();
  });
});
