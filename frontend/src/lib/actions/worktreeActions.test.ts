import { describe, expect, it } from "vitest";
import { pruneWorktreesAction, worktreeActions } from "./worktreeActions";

const linked = { name: "wt", path: "/repo-wt", head: "topic", isMain: false, isLocked: false, isPrunable: false };
const free = { blocked: false, reason: null };
const busy = { blocked: true, reason: "A rebase is in progress." };

describe("worktreeActions", () => {
  it("labels a worktree's actions by path in the panel and by name in the palette", () => {
    const actions = worktreeActions(free, linked);
    expect(actions.open).toEqual({ label: "Open /repo-wt", paletteLabel: "Open worktree wt", enabled: true, reason: undefined });
    expect(actions.remove).toMatchObject({ label: "Remove /repo-wt", paletteLabel: "Remove worktree wt", enabled: true });
  });

  it("names the main worktree's open in the palette without its name (#561)", () => {
    expect(worktreeActions(free, { ...linked, name: "main", path: "/repo", isMain: true }).open).toEqual({
      label: "Open /repo",
      paletteLabel: "Open main worktree",
      enabled: true,
      reason: undefined,
    });
  });

  it("refuses to remove the main worktree, saying why", () => {
    expect(worktreeActions(free, { ...linked, isMain: true }).remove).toMatchObject({
      enabled: false,
      reason: "The main worktree can't be removed.",
    });
  });

  it("disables open, remove and prune with the busy reason", () => {
    const actions = worktreeActions(busy, linked);
    for (const action of [actions.open, actions.remove, pruneWorktreesAction(busy)]) {
      expect(action).toMatchObject({ enabled: false, reason: busy.reason });
    }
  });
});
