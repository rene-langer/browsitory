import { describe, expect, it, vi } from "vitest";
import type { UserCommand, UserCommandContext } from "../ipc/RepoClient";
import {
  NO_SELECTION,
  userCommandDisabledReason,
  userCommandMenuItems,
  userCommandPaletteCommands,
} from "./userCommands";

function command(name: string, overrides: Partial<UserCommand> = {}): UserCommand {
  return {
    id: `id-${name}`,
    name,
    commandLine: "true",
    workingDirectory: null,
    targets: [],
    confirm: false,
    showOutput: true,
    refresh: true,
    variables: [],
    problem: null,
    ...overrides,
  };
}

const commit: UserCommandContext = { ...NO_SELECTION, commitId: "abc" };

describe("userCommandDisabledReason (#895)", () => {
  it("is null when every variable can be filled", () => {
    expect(userCommandDisabledReason(command("A", { variables: ["repo"] }), NO_SELECTION)).toBeNull();
    expect(userCommandDisabledReason(command("A", { variables: ["commit", "shortCommit"] }), commit)).toBeNull();
  });

  it("a branch or ref also fills the commit, its ref and branch", () => {
    const branch = { ...NO_SELECTION, branch: "main" };
    const tag = { ...NO_SELECTION, refName: "refs/tags/v1" };
    const remote = { ...NO_SELECTION, refName: "refs/remotes/origin/main" };
    const uses = command("A", { variables: ["commit", "ref"] });
    expect(userCommandDisabledReason(uses, branch)).toBeNull();
    expect(userCommandDisabledReason(uses, tag)).toBeNull();
    const needsBranch = command("B", { variables: ["branch"] });
    expect(userCommandDisabledReason(needsBranch, remote)).toBeNull();
    expect(userCommandDisabledReason(needsBranch, { ...NO_SELECTION, refName: "refs/heads/x" })).toBeNull();
    expect(userCommandDisabledReason(needsBranch, tag)).toBe("Needs a branch");
  });

  it("says what is missing", () => {
    expect(userCommandDisabledReason(command("A", { variables: ["commit"] }), NO_SELECTION)).toBe("Needs a commit");
    expect(userCommandDisabledReason(command("A", { variables: ["shortCommit"] }), NO_SELECTION)).toBe("Needs a commit");
    expect(userCommandDisabledReason(command("A", { variables: ["branch"] }), commit)).toBe("Needs a branch");
    expect(userCommandDisabledReason(command("A", { variables: ["ref"] }), commit)).toBe("Needs a branch or tag");
    expect(userCommandDisabledReason(command("A", { variables: ["paths"] }), commit)).toBe("Needs a file");
    expect(userCommandDisabledReason(command("A", { variables: ["path"] }), commit)).toBe("Needs one file");
    const two = { ...NO_SELECTION, paths: ["a", "b"] };
    expect(userCommandDisabledReason(command("A", { variables: ["path"] }), two)).toBe("Needs one file");
    expect(userCommandDisabledReason(command("A", { variables: ["paths"] }), two)).toBeNull();
  });

  it("a command that can't be read gives its problem", () => {
    expect(userCommandDisabledReason(command("A", { problem: "Unknown variable ${x}." }), commit)).toBe(
      "Unknown variable ${x}.",
    );
  });
});

describe("userCommandMenuItems (#895)", () => {
  it("lists the commands set for the menu, in a group of their own, disabled with the reason", () => {
    const onRun = vi.fn();
    const commands = [
      command("Show", { targets: ["Commit", "Branch"], variables: ["commit"] }),
      command("Lint", { targets: ["File"], variables: ["paths"] }),
      command("Needs file", { targets: ["Commit"], variables: ["path"] }),
    ];
    const items = userCommandMenuItems(commands, "Commit", commit, onRun);
    expect(items.map((item) => [item.label, item.group, item.disabled ?? false, item.title])).toEqual([
      ["Show", "user-commands", false, undefined],
      ["Needs file", "user-commands", true, "Needs one file"],
    ]);
    items[0].onSelect();
    expect(onRun).toHaveBeenCalledWith(commands[0], commit);
    expect(userCommandMenuItems(commands, "Tag", commit, onRun)).toEqual([]);
  });
});

describe("userCommandPaletteCommands (#895)", () => {
  it("offers every command as Run: <name>, unavailable ones with the reason", () => {
    const onRun = vi.fn();
    const commands = [command("Show", { variables: ["commit"] }), command("Lint", { variables: ["paths"] })];
    const palette = userCommandPaletteCommands(commands, commit, onRun);
    expect(palette.map((entry) => [entry.id, entry.label, entry.group, entry.disabledReason])).toEqual([
      ["user-command:id-Show", "Run: Show", "General", undefined],
      ["user-command:id-Lint", "Run: Lint", "General", "Needs a file"],
    ]);
    expect(palette[0].keywords).toEqual(expect.arrayContaining(["run", "command", "Show"]));
    palette[0].run();
    expect(onRun).toHaveBeenCalledWith(commands[0], commit);
  });
});
