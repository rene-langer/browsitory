import { describe, expect, it } from "vitest";
import type { AppState } from "../../state/useAppState";
import { stashActions, stashBlockers, stashDetails, stashTitle } from "./stashActions";
import { formatRelativeDate } from "../formatDate";

const headInfo: AppState["headInfo"] = {
  headId: "c1",
  detachedAt: null,
  headPushed: false,
  operation: null,
  operationLeftover: false,
  squashMessage: null,
  commitHooks: [],
  sequence: null,
  rebase: null,
  bisect: null,
  terminalSequence: null,
  terminalRebase: null,
};
const idle = { pending: false, transfer: null, mergeMessage: null, rebaseProgress: null, headInfo, status: [] };
const modified = (staged: boolean, kind: "Modified" | "Conflicted" = "Modified") => [{ staged, kind }];

describe("stashBlockers", () => {
  it("allows everything while nothing blocks", () => {
    expect(stashBlockers(idle)).toEqual({ apply: null, drop: null });
    expect(stashBlockers({ ...idle, status: modified(false) })).toEqual({ apply: null, drop: null });
  });

  it("asks to resolve conflicts, then to unstage, before naming the busy operation", () => {
    expect(stashBlockers({ ...idle, mergeMessage: "Merge", status: modified(false, "Conflicted") })).toEqual({
      apply: "Resolve conflicts first.",
      drop: null,
    });
    expect(stashBlockers({ ...idle, status: modified(true) }).apply).toBe("Unstage or commit staged changes first.");
    expect(stashBlockers({ ...idle, rebaseProgress: {} })).toEqual({ apply: "A rebase is in progress.", drop: null });
  });

  it("blocks Drop only while an action or a transfer runs (#521)", () => {
    expect(stashBlockers({ ...idle, pending: true })).toEqual({
      apply: "Another action is in progress.",
      drop: "Another action is in progress.",
    });
  });
});

describe("stashActions", () => {
  it("labels the actions and names the stash in the palette", () => {
    const actions = stashActions({ apply: "Resolve conflicts first.", drop: null }, { index: 0, message: "WIP", commitId: "s", branch: null, timestamp: 1700000000, fileCount: 1, hasStagedChanges: false });
    expect(actions.apply).toEqual({ label: "Apply", paletteLabel: "Apply stash: WIP", enabled: false, reason: "Resolve conflicts first." });
    expect(actions.pop).toMatchObject({ label: "Pop", paletteLabel: "Pop stash: WIP", enabled: false });
    expect(actions.drop).toEqual({ label: "Drop…", paletteLabel: "Drop stash: WIP", enabled: true, reason: undefined });
  });
});

describe("stashTitle and stashDetails (#657)", () => {
  const entry = (message: string, branch: string | null) => ({ message, branch });

  it("drops git's branch prefix from the message", () => {
    expect(stashTitle(entry("On main: fix tests", "main"))).toBe("fix tests");
    expect(stashTitle(entry("WIP on main: abc1234 Add", "main"))).toBe("WIP: abc1234 Add");
    expect(stashTitle(entry("On feat/x: a: b", "feat/x"))).toBe("a: b");
    expect(stashTitle(entry("autostash", null))).toBe("autostash");
    // Another branch's prefix, or nothing after it, keeps the message.
    expect(stashTitle(entry("On other: x", "main"))).toBe("On other: x");
    expect(stashTitle(entry("On main:  ", "main"))).toBe("On main:  ");
  });

  it("joins branch, age and file count", () => {
    const now = 1_700_000_000_000;
    const at = now / 1000 - 2 * 86400;
    const age = formatRelativeDate(at, now);
    expect(stashDetails({ branch: "main", timestamp: at, fileCount: 3 }, now)).toBe(`On main · ${age} · 3 files`);
    expect(stashDetails({ branch: "(no branch)", timestamp: at, fileCount: 1 }, now)).toBe(`Detached HEAD · ${age} · 1 file`);
    expect(stashDetails({ branch: null, timestamp: at, fileCount: 0 }, now)).toBe(`${age} · 0 files`);
  });

  it("offers the unstaged variants only for a stash with staged changes", () => {
    const stash = { index: 2, message: "On main: wip", commitId: "s", branch: "main", timestamp: 1700000000, fileCount: 1, hasStagedChanges: true };
    const actions = stashActions({ apply: null, drop: null }, stash);
    expect(actions.applyUnstaged).toEqual({ label: "Apply as unstaged", paletteLabel: "Apply stash as unstaged: wip", enabled: true, reason: undefined });
    expect(actions.popUnstaged).toMatchObject({ label: "Pop as unstaged", enabled: true });
    const plain = stashActions({ apply: "Resolve conflicts first.", drop: null }, { ...stash, hasStagedChanges: false });
    expect(plain.applyUnstaged).toBeNull();
    expect(plain.popUnstaged).toBeNull();
  });
});
