import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { IdentityDialog } from "./IdentityDialog";

function renderDialog(props: Partial<Parameters<typeof IdentityDialog>[0]> = {}) {
  const handlers = { onSave: vi.fn(async () => null as string | null), onCancel: vi.fn() };
  render(<IdentityDialog retrying {...handlers} {...props} />);
  return { dialog: screen.getByRole("dialog", { name: "Set your git identity" }), ...handlers };
}

describe("IdentityDialog (#689)", () => {
  it("saves a name and email for all repositories by default, then commits", async () => {
    const { dialog, onSave } = renderDialog();
    const save = within(dialog).getByRole("button", { name: "Save and commit" });
    expect(save).toBeDisabled();
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: " Ada Lovelace " } });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Email" }), { target: { value: "ada@example.com" } });
    expect(within(dialog).getByRole("radio", { name: /All repositories/ })).toBeChecked();
    fireEvent.click(save);
    await waitFor(() => expect(onSave).toHaveBeenCalledWith("Ada Lovelace", "ada@example.com", true));
  });

  it("saves for this repository only when chosen", async () => {
    const { dialog, onSave } = renderDialog({ retrying: false });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: "Ada" } });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Email" }), { target: { value: "ada@example.com" } });
    fireEvent.click(within(dialog).getByRole("radio", { name: "This repository only" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith("Ada", "ada@example.com", false));
  });

  it("refuses an email with spaces or angle brackets", () => {
    const { dialog } = renderDialog();
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: "Ada" } });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Email" }), { target: { value: "ada <x>" } });
    expect(within(dialog).getByRole("button", { name: "Save and commit" })).toBeDisabled();
    expect(dialog).toHaveTextContent("can't contain < or >");
  });

  it("shows a failed save in the dialog", async () => {
    const onSave = vi.fn(async () => "could not lock config file");
    const { dialog } = renderDialog({ onSave });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: "Ada" } });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Email" }), { target: { value: "ada@example.com" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save and commit" }));
    expect(await within(dialog).findByText("could not lock config file")).toBeInTheDocument();
  });
});
