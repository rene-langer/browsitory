import type { UserCommand, UserCommandInput, UserCommandTarget, UserCommandVariable } from "../ipc/RepoClient";

/** Every menu a user command can appear in (#893), in the order Preferences lists them. */
export const USER_COMMAND_TARGETS: readonly UserCommandTarget[] = ["Commit", "Branch", "Tag", "File", "Repository"];

/** Every `${…}` variable, in the order the backend lists them. */
export const USER_COMMAND_VARIABLES: readonly UserCommandVariable[] = [
  "repo",
  "commit",
  "shortCommit",
  "branch",
  "ref",
  "path",
  "paths",
];

/** The stored fields of a listed command, as `setUserCommands` takes them back. */
export function toUserCommandInput(command: UserCommand): UserCommandInput {
  return {
    id: command.id,
    name: command.name,
    commandLine: command.commandLine,
    workingDirectory: command.workingDirectory,
    targets: command.targets,
    confirm: command.confirm,
    showOutput: command.showOutput,
    refresh: command.refresh,
  };
}
