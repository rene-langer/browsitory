import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../../ipc/RepoClient";
import type { PaletteAction } from "./action";
import { branchActions, rebaseOntoAction } from "./branchActions";
import { commitActions } from "./commitActions";
import { addRemoteAction, remoteActions } from "./remoteActions";
import { stashActions } from "./stashActions";
import { worktreeActions } from "./worktreeActions";

/**
 * #990: one name per action. `lib/actions` gives each action's menu label and palette label; the
 * palette's is the menu's plus the object it acts on, so the two agree word for word ("here" or a
 * path stands for the object in the menu), and both end in "…" exactly when a dialog or
 * confirmation follows.
 */
function agree(action: Pick<PaletteAction, "label" | "paletteLabel">) {
  const words = (label: string) => label.replace(/…$/, "").split(" ").map((word) => word.replace(/:$/, ""));
  const palette = words(action.paletteLabel);
  let at = 0;
  for (const word of words(action.label)) {
    if (word === "here" || word.startsWith("/")) continue;
    while (at < palette.length && palette[at] !== word) at++;
    if (at === palette.length) return false;
    at++;
  }
  return action.label.endsWith("…") === action.paletteLabel.endsWith("…");
}

const commit = (id: string, parentIds: string[]): GraphCommit => ({
  id,
  shortId: id.slice(0, 7),
  summary: id,
  authorName: "A",
  authorEmail: "a@x",
  timestamp: 0,
  parentIds,
  branchRefs: [],
  remoteBranchRefs: [],
});
const free = { blocked: false, reason: null };
const stash = { index: 0, message: "WIP", commitId: "s", branch: null, timestamp: 0, fileCount: 1, hasStagedChanges: true };
const worktree = { name: "wt", path: "/repo-wt", head: "topic", isMain: false, isLocked: false, isPrunable: false };

function everyPaletteAction(): [string, Pick<PaletteAction, "label" | "paletteLabel">][] {
  const actions: [string, Pick<PaletteAction, "label" | "paletteLabel">][] = [];
  const add = (prefix: string, record: object) => {
    for (const [name, action] of Object.entries(record)) {
      if (action !== null && typeof action === "object" && "paletteLabel" in action) actions.push([`${prefix}.${name}`, action]);
    }
  };
  for (const [name, parents, branch] of [
    ["plain", ["p"], "main"],
    ["merge", ["p1", "p2"], "main"],
    ["detached", ["p"], null],
  ] as const) {
    add(`commit(${name})`, commitActions({ blockers: free, headId: null, currentBranch: branch }, commit("abc1234", [...parents])));
  }
  add("branch", branchActions({ blockers: free, remoteCount: 1, upstream: null }, { name: "feature", isCurrent: false }));
  add("rebaseOnto", { action: rebaseOntoAction(free, "main", { kind: "local", name: "feature" }) });
  add("remote", remoteActions(free, "origin"));
  add("remote", { add: addRemoteAction() });
  add("stash", stashActions({ apply: null, drop: null }, stash));
  add("worktree", worktreeActions(free, worktree));
  return actions;
}

describe("menu and palette labels (#990)", () => {
  it("covers the actions of every entity", () => {
    expect(everyPaletteAction().length).toBeGreaterThan(20);
  });

  it.each(everyPaletteAction())("%s: the palette label is the menu label plus its object", (_name, action) => {
    expect({ label: action.label, paletteLabel: action.paletteLabel, agree: agree(action) }).toEqual({
      label: action.label,
      paletteLabel: action.paletteLabel,
      agree: true,
    });
  });

  it("the check itself refuses labels that disagree", () => {
    expect(agree({ label: "Push current branch here", paletteLabel: "Push to origin" })).toBe(false);
    expect(agree({ label: "Edit remote", paletteLabel: "Edit remote origin…" })).toBe(false);
    expect(agree({ label: "Reset main to here…", paletteLabel: "Reset to abc1234…" })).toBe(false);
    expect(agree({ label: "Reset main to here…", paletteLabel: "Reset main to abc1234…" })).toBe(true);
  });
});
