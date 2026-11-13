import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { UserCommand, UserCommandRun } from "../ipc/RepoClient";
import { UserCommandOutputDialog } from "./UserCommandOutputDialog";

const command: UserCommand = {
  id: "id-1",
  name: "Lint",
  commandLine: "make lint",
  workingDirectory: null,
  targets: [],
  confirm: false,
  showOutput: true,
  refresh: false,
  variables: [],
  problem: null,
};

const base: UserCommandRun = { exitCode: 0, success: true, timedOut: false, cancelled: false, output: "all good\n" };

function renderDialog(result: UserCommandRun | null, error: string | null = null) {
  const onStop = vi.fn();
  const onClose = vi.fn();
  render(<UserCommandOutputDialog output={{ command, result, error }} onStop={onStop} onClose={onClose} />);
  return { onStop, onClose };
}

describe("UserCommandOutputDialog (#895)", () => {
  it("shows a running command with Stop", () => {
    const { onStop, onClose } = renderDialog(null);
    const dialog = screen.getByRole("dialog", { name: "Lint" });
    expect(dialog).toHaveTextContent("Running…");
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(onStop).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("shows the output of a finished command, without Stop", () => {
    renderDialog(base);
    expect(screen.getByRole("status")).toHaveTextContent("Finished");
    expect(screen.getByText("all good")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Stop" })).not.toBeInTheDocument();
  });

  it("says how a command ended badly", () => {
    renderDialog({ ...base, success: false, exitCode: 3, output: "bad\n" });
    expect(screen.getByRole("status")).toHaveTextContent("Failed with exit code 3");
  });

  it("says when it ran out of time or was stopped, and when there was no output", () => {
    renderDialog({ ...base, success: false, exitCode: null, timedOut: true, output: "" });
    expect(screen.getByRole("status")).toHaveTextContent("Took too long and was stopped");
    expect(screen.getByText("No output")).toBeInTheDocument();
  });

  it("says when it was stopped", () => {
    renderDialog({ ...base, success: false, exitCode: null, cancelled: true, output: "" });
    expect(screen.getByRole("status")).toHaveTextContent("Stopped");
  });

  it("shows why it couldn't start", () => {
    renderDialog(null, "Couldn't find the program 'nope'.");
    expect(screen.getByRole("alert")).toHaveTextContent("Couldn't find the program 'nope'.");
    expect(screen.queryByRole("button", { name: "Stop" })).not.toBeInTheDocument();
    expect(screen.queryByText("Running…")).not.toBeInTheDocument();
  });
});
