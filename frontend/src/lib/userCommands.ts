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

const isBranchRef = (refName: string | null) =>
  refName !== null && (refName.startsWith("refs/heads/") || refName.startsWith("refs/remotes/"));

/**
 * Why `command` can't run on `selection`, `null` when it can. The backend fills a commit from a
 * branch's or ref's tip, a ref from a branch, and a branch from a branch ref, so those count.
 */
export function userCommandDisabledReason(command: UserCommand, selection: UserCommandContext): string | null {
  if (command.problem !== null) return command.problem;
  for (const variable of command.variables) {
    switch (variable) {
      case "repo":
        break;
      case "commit":
      case "shortCommit":
        if (selection.commitId === null && selection.branch === null && selection.refName === null) return "Needs a commit";
        break;
      case "branch":
        if (selection.branch === null && !isBranchRef(selection.refName)) return "Needs a branch";
        break;
      case "ref":
        if (selection.refName === null && selection.branch === null) return "Needs a branch or tag";
        break;
      case "path":
        if (selection.paths.length !== 1) return "Needs one file";
        break;
      case "paths":
        if (selection.paths.length === 0) return "Needs a file";
        break;
    }
  }
  return null;
}

/** The items of the commands set for `target`'s menu, in a group of their own (#895). */
export function userCommandMenuItems(
  commands: UserCommand[],
  target: UserCommandTarget,
  selection: UserCommandContext,
  onRun: RunUserCommand,
): ContextMenuItem[] {
  return commands
    .filter((command) => command.targets.includes(target))
    .map((command) => {
      const reason = userCommandDisabledReason(command, selection);
      return {
        label: command.name,
        group: "user-commands",
        onSelect: () => onRun(command, selection),
        ...(reason === null ? {} : { disabled: true, title: reason }),
      };
    });
}

/** Every command as "Run: <name>" for the palette, an unavailable one with its reason (#895). */
export function userCommandPaletteCommands(
  commands: UserCommand[],
  selection: UserCommandContext,
  onRun: RunUserCommand,
): Command[] {
  return commands.map((command) => {
    const reason = userCommandDisabledReason(command, selection);
    return {
      id: `user-command:${command.id}`,
      label: `Run: ${command.name}`,
      keywords: ["run", "command", command.name],
      group: "General",
      run: () => onRun(command, selection),
      ...(reason === null ? {} : { disabledReason: reason }),
    };
  });
}
