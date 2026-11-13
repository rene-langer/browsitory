import { useCallback, useRef, useState } from "react";
import type { RepoClient, UserCommand, UserCommandContext, UserCommandRun } from "../ipc/RepoClient";
import type { PushToast } from "./useToasts";

/** The output view of a run: `result` `null` while it runs; `error` when it couldn't start. */
export interface UserCommandOutput {
  command: UserCommand;
  result: UserCommandRun | null;
  error: string | null;
}

export interface UserCommandRunner {
  /** Runs `command` on `selection`, asking first when it is set to. */
  run(command: UserCommand, selection: UserCommandContext): void;
  /** The command waiting for "Run" in the confirmation dialog. */
  confirming: { command: UserCommand; selection: UserCommandContext } | null;
  confirm(): void;
  cancelConfirm(): void;
  /** The output view, open for a command that shows its output (or "Show output"). */
  output: UserCommandOutput | null;
  /** Stops the latest run. */
  stop(): void;
  closeOutput(): void;
}

const message = (err: unknown) => (err instanceof Error ? err.message : String(err));

let nextRun = 0;

/**
 * Runs the user commands (#895) of one workspace: the confirmation a command asks for, its output
 * view (open while it runs, with Stop) or a toast when it ends, and the refresh it asks for.
 */
export function useUserCommandRunner({
  client,
  repoPath,
  refresh,
  pushToast,
}: {
  client: RepoClient;
  repoPath: string;
  refresh: () => Promise<void>;
  pushToast: PushToast;
}): UserCommandRunner {
  const [confirming, setConfirming] = useState<UserCommandRunner["confirming"]>(null);
  const [output, setOutput] = useState<UserCommandOutput | null>(null);
  const runId = useRef<string | null>(null);

  const start = useCallback(
    (command: UserCommand, selection: UserCommandContext) => {
      nextRun += 1;
      const id = `${command.id}-${nextRun}`;
      runId.current = id;
      if (command.showOutput) setOutput({ command, result: null, error: null });
      // Only the latest run's view is updated: an older one ending late leaves it alone.
      const show = (next: UserCommandOutput) =>
        setOutput((current) => (runId.current === id || current === null ? next : current));
      void client.runUserCommand(repoPath, command.id, id, selection).then(
        (result) => {
          if (command.showOutput) {
            show({ command, result, error: null });
          } else {
            const showOutput = { label: "Show output", run: () => setOutput({ command, result, error: null }) };
            if (result.cancelled) pushToast(`${command.name} was stopped`, { tone: "info" });
            else if (result.success) pushToast(`${command.name} finished`, { tone: "success" });
            else if (result.timedOut)
              pushToast(`${command.name} took too long and was stopped`, { tone: "error", action: showOutput });
            else
              pushToast(
                result.exitCode === null
                  ? `${command.name} failed`
                  : `${command.name} failed with exit code ${result.exitCode}`,
                { tone: "error", action: showOutput },
              );
          }
          if (command.refresh) void refresh();
        },
        (err: unknown) => {
          if (command.showOutput) show({ command, result: null, error: message(err) });
          else pushToast(`${command.name}: ${message(err)}`, { tone: "error" });
        },
      );
    },
    [client, repoPath, refresh, pushToast],
  );

  const run = useCallback(
    (command: UserCommand, selection: UserCommandContext) => {
      if (command.confirm) setConfirming({ command, selection });
      else start(command, selection);
    },
    [start],
  );

  const confirm = useCallback(() => {
    if (confirming === null) return;
    setConfirming(null);
    start(confirming.command, confirming.selection);
  }, [confirming, start]);

  const stop = useCallback(() => {
    if (runId.current !== null) void client.cancelUserCommand(runId.current);
  }, [client]);

  return {
    run,
    confirming,
    confirm,
    cancelConfirm: () => setConfirming(null),
    output,
    stop,
    closeOutput: () => setOutput(null),
  };
}
