import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TransferPanel } from "./TransferPanel";

describe("TransferPanel", () => {
  it("calls onCancel with the in-flight operation id when Cancel is clicked", () => {
    const onCancel = vi.fn();
    render(
      <TransferPanel
        progress={{
          operationId: "fetch-1",
          operation: "Fetch",
          phase: "Receiving",
          errorKind: null,
          current: 10,
          total: 100,
          receivedBytes: 2048,
          message: null,
        }}
        onCancel={onCancel}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onCancel).toHaveBeenCalledWith("fetch-1");
  });

  it("disables Cancel once it has been pressed, so a second click cannot re-fire", () => {
    const onCancel = vi.fn();
    render(
      <TransferPanel
        progress={{
          operationId: "fetch-1",
          operation: "Fetch",
          phase: "Receiving",
          errorKind: null,
          current: 10,
          total: 100,
          receivedBytes: 2048,
          message: null,
        }}
        onCancel={onCancel}
      />,
    );

    const cancel = screen.getByRole("button", { name: "Cancel" });
    fireEvent.click(cancel);
    fireEvent.click(cancel);

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Cancelling…" })).toBeDisabled();
  });

  it("renders object and byte progress for an active transfer", () => {
    render(
      <TransferPanel
        progress={{
          operationId: "op-1",
          operation: "Fetch",
          phase: "Receiving",
          errorKind: null,
          current: 2,
          total: 4,
          receivedBytes: 1024,
          message: null,
        }}
        onCancel={() => {}}
      />,
    );

    expect(screen.getByText("2 / 4 objects")).toBeInTheDocument();
    expect(screen.getByText("1.0 KB received")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Transfer progress" })).toBeInTheDocument();
  });

  it.each([
    [1536 * 1024, "1.5 MB received"],
    [3 * 1024 * 1024 * 1024, "3.0 GB received"],
  ])("formats %i bytes beyond KB", (receivedBytes, text) => {
    render(
      <TransferPanel
        progress={{
          operationId: "op-1",
          operation: "Fetch",
          phase: "Receiving",
          errorKind: null,
          current: 1,
          total: 2,
          receivedBytes,
          message: null,
        }}
        onCancel={() => {}}
      />,
    );
    expect(screen.getByText(text)).toBeInTheDocument();
  });

  it("does not render a transfer message from the event payload", () => {
    render(
      <TransferPanel
        progress={{
          operationId: "op-1",
          operation: "Fetch",
          phase: "Receiving",
          errorKind: null,
          current: 2,
          total: 4,
          receivedBytes: 1024,
          message: "https://alice:secret@example.test/repo.git",
        }}
        onCancel={() => {}}
      />,
    );

    expect(screen.queryByText("https://alice:secret@example.test/repo.git")).not.toBeInTheDocument();
  });
});
