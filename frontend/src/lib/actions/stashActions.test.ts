import { describe, expect, it } from "vitest";
import type { AppState } from "../../state/useAppState";
import { stashActions, stashBlockers } from "./stashActions";

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
    const actions = stashActions({ apply: "Resolve conflicts first.", drop: null }, { index: 0, message: "WIP", commitId: "s" });
    expect(actions.apply).toEqual({ label: "Apply", paletteLabel: "Apply stash: WIP", enabled: false, reason: "Resolve conflicts first." });
    expect(actions.pop).toMatchObject({ label: "Pop", paletteLabel: "Pop stash: WIP", enabled: false });
    expect(actions.drop).toEqual({ label: "Drop…", paletteLabel: "Drop stash: WIP", enabled: true, reason: undefined });
  });
});
