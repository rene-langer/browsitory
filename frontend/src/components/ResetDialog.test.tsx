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
    expect(screen.getByText("Discards all uncommitted changes. Cannot be undone.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Reset hard" }));
    expect(onReset).toHaveBeenCalledWith("hard");
  });

  it("names the branch and target", () => {
    render(<ResetDialog branch="main" shortId="abc1234" onReset={() => {}} onCancel={() => {}} />);
    expect(screen.getByText(/Reset main to abc1234/)).toBeInTheDocument();
  });
});
