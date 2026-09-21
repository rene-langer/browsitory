import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { ConfirmDialog } from "./ConfirmDialog";
import { FormDialog } from "./FormDialog";

function ConfirmHost() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>Trigger</button>
      {open && <ConfirmDialog ariaLabel="Sure" message="Sure?" confirmLabel="Yes" onConfirm={() => setOpen(false)} onCancel={() => setOpen(false)} />}
    </>
  );
}

function FormHost() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>Trigger</button>
      {open && (
        <FormDialog ariaLabel="Form" onCancel={() => setOpen(false)}>
          <input aria-label="field" />
          <button onClick={() => setOpen(false)}>Done</button>
        </FormDialog>
      )}
    </>
  );
}

describe("dialog focus restoration", () => {
  it("ConfirmDialog returns focus to the invoking control when it closes", () => {
    render(<ConfirmHost />);
    const trigger = screen.getByRole("button", { name: "Trigger" });
    trigger.focus();
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole("button", { name: "Cancel", hidden: true }));
    expect(trigger).toHaveFocus();
  });

  it("FormDialog returns focus to the invoking control when it closes", () => {
    render(<FormHost />);
    const trigger = screen.getByRole("button", { name: "Trigger" });
    trigger.focus();
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole("button", { name: "Done", hidden: true }));
    expect(trigger).toHaveFocus();
  });
});
