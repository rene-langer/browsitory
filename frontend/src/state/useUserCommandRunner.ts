import type { RepoClient, UserCommand, UserCommandContext, UserCommandRun } from "../ipc/RepoClient";
import type { PushToast } from "./useToasts";

/** The output view of a run: `result` `null` while it runs; `error` when it couldn't start. */
export interface UserCommandOutput {
  command: UserCommand;
  result: UserCommandRun | null;
  error: string | null;
}

export interface UserCommandRunner {
  run(command: UserCommand, selection: UserCommandContext): void;
  confirming: { command: UserCommand; selection: UserCommandContext } | null;
  confirm(): void;
  cancelConfirm(): void;
  output: UserCommandOutput | null;
  stop(): void;
  closeOutput(): void;
}

export function useUserCommandRunner(_options: {
  client: RepoClient;
  repoPath: string;
  refresh: () => Promise<void>;
  pushToast: PushToast;
}): UserCommandRunner {
  throw new Error("not implemented");
}
