import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RewordDialog } from "./RewordDialog";

function renderDialog(overrides: Partial<Parameters<typeof RewordDialog>[0]> = {}) {
  const props = {
    shortId: "abc1234",
    loadMessage: vi.fn(async () => "Old summary\n\nOld body\n"),
    disabled: false,
    disabledReason: null,
    onReword: vi.fn(),
    onCancel: vi.fn(),
    ...overrides,
  };
  render(<RewordDialog {...props} />);
  return props;
}

describe("RewordDialog", () => {
  it("starts from the commit's full message and rewords with the edited one", async () => {
    const { onReword } = renderDialog();

    const field = await screen.findByDisplayValue(/Old summary/);
    expect(screen.getByRole("dialog", { name: "Reword commit abc1234" })).toBeInTheDocument();
    expect(field).toHaveValue("Old summary\n\nOld body");
    const submit = screen.getByRole("button", { name: "Reword" });
    // Unchanged: nothing to rewrite.
    expect(submit).toBeDisabled();

    fireEvent.change(field, { target: { value: "New summary\n\nNew body  " } });
    fireEvent.click(submit);

    expect(onReword).toHaveBeenCalledWith("New summary\n\nNew body\n");
  });

  it("refuses an empty message", async () => {
    renderDialog();
    const field = await screen.findByDisplayValue(/Old summary/);

    fireEvent.change(field, { target: { value: "   " } });

    expect(screen.getByRole("button", { name: "Reword" })).toBeDisabled();
  });

  it("is blocked while another operation runs, and shows why", async () => {
    renderDialog({ disabled: true, disabledReason: "A merge is running." });
    const field = await screen.findByDisplayValue(/Old summary/);
    fireEvent.change(field, { target: { value: "New" } });

    expect(screen.getByRole("button", { name: "Reword" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Reword" })).toHaveAttribute("title", "A merge is running.");
  });

  it("shows a message that can't be loaded and still lets you cancel", async () => {
    const { onCancel } = renderDialog({ loadMessage: vi.fn(async () => Promise.reject(new Error("gone"))) });

    expect(await screen.findByRole("alert")).toHaveTextContent("gone");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });
});
