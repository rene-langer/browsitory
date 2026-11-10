import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ResetDialog } from "./ResetDialog";

describe("ResetDialog", () => {
  it("defaults to mixed and resets with the chosen mode", () => {
    const onReset = vi.fn();
    render(<ResetDialog branch="main" shortId="abc1234" onReset={onReset} onCancel={() => {}} />);
    expect(screen.getByRole("radio", { name: /Mixed/ })).toBeChecked();
    fireEvent.click(screen.getByRole("radio", { name: /Soft/ }));
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(onReset).toHaveBeenCalledWith("soft");
  });

  it("asks for confirmation before a hard reset", () => {
    const onReset = vi.fn();
    render(<ResetDialog branch="main" shortId="abc1234" onReset={onReset} onCancel={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: /Hard/ }));
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(onReset).not.toHaveBeenCalled();
    expect(screen.getByText("Discards all uncommitted changes. Undo can bring them back.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Reset hard" }));
    expect(onReset).toHaveBeenCalledWith("hard");
  });

  it("names the branch and target", () => {
    render(<ResetDialog branch="main" shortId="abc1234" onReset={() => {}} onCancel={() => {}} />);
    expect(screen.getByText(/Reset main to abc1234/)).toBeInTheDocument();
  });

  // #514: the repository can become busy while the dialog is open (a terminal rebase picked up
  // by a refresh): Reset is disabled with the reason and Enter sends nothing.
  it("disables Reset with the reason and sends nothing while the repository is busy", () => {
    const onReset = vi.fn();
    render(
      <ResetDialog
        branch="main"
        shortId="abc1234"
        blockedReason="A rebase started in a terminal is in progress."
        onReset={onReset}
        onCancel={() => {}}
      />,
    );
    const reset = screen.getByRole("button", { name: "Reset" });
    expect(reset).toBeDisabled();
    expect(reset).toHaveAttribute("title", "A rebase started in a terminal is in progress.");
    expect(screen.getByText("A rebase started in a terminal is in progress.")).toBeInTheDocument();
    fireEvent.submit(reset.closest("form")!);
    expect(onReset).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
  });

  it("disables Reset hard once the repository becomes busy during the confirmation", () => {
    const onReset = vi.fn();
    const props = { branch: "main", shortId: "abc1234", onReset, onCancel: () => {} };
    const { rerender } = render(<ResetDialog {...props} />);
    fireEvent.click(screen.getByRole("radio", { name: /Hard/ }));
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    rerender(<ResetDialog {...props} blockedReason="A cherry-pick is in progress." />);
    const hard = screen.getByRole("button", { name: "Reset hard" });
    expect(hard).toBeDisabled();
    expect(hard).toHaveAttribute("title", "A cherry-pick is in progress.");
    expect(screen.getByText("A cherry-pick is in progress.")).toBeInTheDocument();
    fireEvent.click(hard);
    expect(onReset).not.toHaveBeenCalled();
  });
});
