import { createContext, useContext } from "react";
import type { UserCommand } from "../ipc/RepoClient";
import type { RunUserCommand } from "../lib/userCommands";

/**
 * The workspace's user commands (#895) and how to run one, for the file menus deep in the diff
 * panes (the graph and sidebar menus get them as props). `null`: no user commands here (tests,
 * or an untrusted VSCode workspace).
 */
export interface UserCommandsApi {
  commands: UserCommand[];
  run: RunUserCommand;
}

export const UserCommandsContext = createContext<UserCommandsApi | null>(null);

export function useUserCommandsContext(): UserCommandsApi | null {
  return useContext(UserCommandsContext);
}
