import type { ContextMenuItem } from "../components/primitives/ContextMenu";
import type {
  UserCommand,
  UserCommandContext,
  UserCommandInput,
  UserCommandTarget,
  UserCommandVariable,
} from "../ipc/RepoClient";
import type { Command } from "./commands";

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

/** Nothing selected: only `${repo}` can be filled. */
export const NO_SELECTION: UserCommandContext = { commitId: null, branch: null, refName: null, paths: [] };

/** Runs `command` on `selection` (asking first, showing output, as the command is set up). */
export type RunUserCommand = (command: UserCommand, selection: UserCommandContext) => void;

export function userCommandDisabledReason(_command: UserCommand, _selection: UserCommandContext): string | null {
  throw new Error("not implemented");
}

export function userCommandMenuItems(
  _commands: UserCommand[],
  _target: UserCommandTarget,
  _selection: UserCommandContext,
  _onRun: RunUserCommand,
): ContextMenuItem[] {
  throw new Error("not implemented");
}

export function userCommandPaletteCommands(
  _commands: UserCommand[],
  _selection: UserCommandContext,
  _onRun: RunUserCommand,
): Command[] {
  throw new Error("not implemented");
}
