import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient, UserCommand, UserCommandRun } from "../ipc/RepoClient";
import { NO_SELECTION } from "../lib/userCommands";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { useUserCommandRunner } from "./useUserCommandRunner";

function command(overrides: Partial<UserCommand> = {}): UserCommand {
  return {
    id: "id-1",
    name: "Show",
    commandLine: "git show ${commit}",
    workingDirectory: null,
    targets: [],
    confirm: false,
    showOutput: true,
    refresh: false,
    variables: ["commit"],
    problem: null,
    ...overrides,
  };
}

const done: UserCommandRun = { exitCode: 0, success: true, timedOut: false, cancelled: false, output: "ok\n" };
const failed: UserCommandRun = { exitCode: 3, success: false, timedOut: false, cancelled: false, output: "bad\n" };
const selection = { ...NO_SELECTION, commitId: "abc" };

function setup(overrides: Partial<RepoClient>) {
  const pushToast = vi.fn();
  const refresh = vi.fn(async () => {});
  const client = fakeRepoClient(overrides);
  const hook = renderHook(() => useUserCommandRunner({ client, repoPath: "/r", refresh, pushToast }));
  return { ...hook, pushToast, refresh };
}

describe("useUserCommandRunner (#895)", () => {
  it("runs a command at once and shows its output, running then finished", async () => {
    let finish!: (run: UserCommandRun) => void;
    const runUserCommand = vi.fn(() => new Promise<UserCommandRun>((resolve) => (finish = resolve)));
    const { result, pushToast } = setup({ runUserCommand });
    act(() => result.current.run(command(), selection));

    expect(runUserCommand).toHaveBeenCalledWith("/r", "id-1", expect.any(String), selection);
    expect(result.current.output).toEqual({ command: command(), result: null, error: null });
    await act(async () => finish(done));
    expect(result.current.output).toEqual({ command: command(), result: done, error: null });
    expect(pushToast).not.toHaveBeenCalled();
    act(() => result.current.closeOutput());
    expect(result.current.output).toBeNull();
  });

  it("gives every run its own id and stops the running one", async () => {
    const runUserCommand = vi.fn(() => new Promise<UserCommandRun>(() => {}));
    const cancelUserCommand = vi.fn(async () => {});
    const { result } = setup({ runUserCommand, cancelUserCommand });
    act(() => result.current.run(command(), selection));
    act(() => result.current.stop());
    const firstId = (runUserCommand.mock.calls[0] as unknown[])[2];
    expect(cancelUserCommand).toHaveBeenCalledWith(firstId);
    act(() => result.current.run(command(), selection));
    expect((runUserCommand.mock.calls[1] as unknown[])[2]).not.toBe(firstId);
  });

  it("asks first when the command is set to, and runs only on confirm", async () => {
    const runUserCommand = vi.fn(async () => done);
    const { result } = setup({ runUserCommand });
    const asking = command({ confirm: true });
    act(() => result.current.run(asking, selection));
    expect(result.current.confirming).toEqual({ command: asking, selection });
    expect(runUserCommand).not.toHaveBeenCalled();
    act(() => result.current.cancelConfirm());
    expect(result.current.confirming).toBeNull();
    expect(runUserCommand).not.toHaveBeenCalled();

    act(() => result.current.run(asking, selection));
    act(() => result.current.confirm());
    expect(result.current.confirming).toBeNull();
    await waitFor(() => expect(runUserCommand).toHaveBeenCalledTimes(1));
  });

  it("without output: a toast says how it went, a failure offers Show output", async () => {
    const runs = [done, failed];
    const runUserCommand = vi.fn(async () => runs.shift()!);
    const quiet = command({ showOutput: false });
    const { result, pushToast } = setup({ runUserCommand });

    await act(async () => result.current.run(quiet, selection));
    expect(result.current.output).toBeNull();
    expect(pushToast).toHaveBeenLastCalledWith("Show finished", { tone: "success" });

    await act(async () => result.current.run(quiet, selection));
    expect(result.current.output).toBeNull();
    expect(pushToast).toHaveBeenLastCalledWith("Show failed with exit code 3", {
      tone: "error",
      action: { label: "Show output", run: expect.any(Function) },
    });
    const [, options] = pushToast.mock.calls[1] as [string, { action: { run: () => void } }];
    act(() => options.action.run());
    expect(result.current.output).toEqual({ command: quiet, result: failed, error: null });
  });

  it("says when a quiet command was stopped or ran out of time", async () => {
    const runs: UserCommandRun[] = [
      { ...failed, exitCode: null, cancelled: true, output: "" },
      { ...failed, exitCode: null, timedOut: true },
    ];
    const runUserCommand = vi.fn(async () => runs.shift()!);
    const quiet = command({ showOutput: false });
    const { result, pushToast } = setup({ runUserCommand });
    await act(async () => result.current.run(quiet, selection));
    expect(pushToast).toHaveBeenLastCalledWith("Show was stopped", { tone: "info" });
    await act(async () => result.current.run(quiet, selection));
    expect(pushToast).toHaveBeenLastCalledWith("Show took too long and was stopped", {
      tone: "error",
      action: { label: "Show output", run: expect.any(Function) },
    });
  });

  it("shows why a command couldn't start, in the output or as a toast", async () => {
    const runUserCommand = vi.fn(async () => {
      throw new Error("Couldn't find the program 'nope'.");
    });
    const { result, pushToast } = setup({ runUserCommand });
    await act(async () => result.current.run(command(), selection));
    expect(result.current.output).toEqual({
      command: command(),
      result: null,
      error: "Couldn't find the program 'nope'.",
    });
    await act(async () => result.current.run(command({ showOutput: false }), selection));
    expect(pushToast).toHaveBeenLastCalledWith("Show: Couldn't find the program 'nope'.", { tone: "error" });
  });

  it("refreshes the repository afterwards when the command is set to, also after a failure", async () => {
    const runs = [done, failed];
    const runUserCommand = vi.fn(async () => runs.shift()!);
    const { result, refresh } = setup({ runUserCommand });
    await act(async () => result.current.run(command({ refresh: false }), selection));
    expect(refresh).not.toHaveBeenCalled();
    await act(async () => result.current.run(command({ refresh: true }), selection));
    expect(refresh).toHaveBeenCalledTimes(1);
  });
});
