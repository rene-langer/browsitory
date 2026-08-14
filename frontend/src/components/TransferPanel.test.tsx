import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TransferPanel } from "./TransferPanel";

describe("TransferPanel", () => {
  it("renders object and byte progress for an active transfer", () => {
    render(
      <TransferPanel
        progress={{
          operationId: "op-1",
          phase: "Receiving",
          current: 2,
          total: 4,
          receivedBytes: 1024,
          message: null,
        }}
      />,
    );

    expect(screen.getByText("2 / 4 objects")).toBeInTheDocument();
    expect(screen.getByText("1.0 KB received")).toBeInTheDocument();
  });

  it("does not render a transfer message from the event payload", () => {
    render(
      <TransferPanel
        progress={{
          operationId: "op-1",
          phase: "Receiving",
          current: 2,
          total: 4,
          receivedBytes: 1024,
          message: "https://alice:secret@example.test/repo.git",
        }}
      />,
    );

    expect(screen.queryByText("https://alice:secret@example.test/repo.git")).not.toBeInTheDocument();
  });
});
