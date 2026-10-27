import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { HookFailureDialog } from "./HookFailureDialog";

const failure = {
  hook: "pre-commit",
  output: "lint failed\n",
  retry: { kind: "commit" as const, message: "m" },
  dialogOpen: true,
};

describe("HookFailureDialog", () => {
  it("shows the hook, its output and the retry", () => {
    const onRetry = vi.fn();
    render(<HookFailureDialog failure={failure} onRetry={onRetry} onClose={() => {}} />);

    expect(screen.getByRole("dialog", { name: "pre-commit hook failed" })).toBeInTheDocument();
    expect(screen.getByText("The pre-commit hook failed.")).toBeInTheDocument();
    expect(screen.getByLabelText("Hook output")).toHaveTextContent("lint failed");
    fireEvent.click(screen.getByRole("button", { name: "Commit without hooks" }));
    expect(onRetry).toHaveBeenCalled();
  });

  it("names the push retry after a failed pre-push", () => {
    render(
      <HookFailureDialog
        failure={{ ...failure, hook: "pre-push", retry: { kind: "push", push: { operation: "PushBranch", remoteName: "origin" } } }}
        onRetry={() => {}}
        onClose={() => {}}
      />,
    );
    expect(screen.getByRole("button", { name: "Push without hooks" })).toBeInTheDocument();
  });

  it("says when there was no output and hides the retry for prepare-commit-msg", () => {
    const onClose = vi.fn();
    render(
      <HookFailureDialog failure={{ ...failure, hook: "prepare-commit-msg", output: "" }} onRetry={() => {}} onClose={onClose} />,
    );

    expect(screen.getByLabelText("Hook output")).toHaveTextContent("(no output)");
    expect(screen.queryByRole("button", { name: /without hooks/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalled();
  });
});
