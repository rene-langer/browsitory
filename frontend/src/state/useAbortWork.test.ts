import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { RebaseInfo, StatusEntry } from "../ipc/RepoClient";
import { type AbortWorkState, useAbortWork } from "./useAbortWork";

const conflicted = (path: string): StatusEntry => ({ path, oldPath: null, staged: false, kind: "Conflicted" });
const modified = (path: string): StatusEntry => ({ path, oldPath: null, staged: true, kind: "Modified" });

const rebase = (overrides: Partial<RebaseInfo> = {}): RebaseInfo => ({
  branch: "topic",
  onto: "o",
  currentStep: 1,
  totalSteps: 3,
  editing: null,
  splitParts: 0,
  health: { kind: "Paused" },
  ...overrides,
});

const state = (overrides: Partial<AbortWorkState> = {}): AbortWorkState => ({
  loaded: true,
  status: [],
  mergeMessage: null,
  headInfo: { operation: null, sequence: null, rebase: null },
  editMessage: "unchanged",
  ...overrides,
});

function render(initial: AbortWorkState, mergeEditorEdited = false) {
  return renderHook(({ s, edited }) => useAbortWork(s, edited), { initialProps: { s: initial, edited: mergeEditorEdited } });
}

describe("useAbortWork (#979)", () => {
  it("keeps counting a file resolved at an earlier rebase step", () => {
    const step1 = state({ status: [conflicted("a.txt")], headInfo: { operation: null, sequence: null, rebase: rebase() } });
    const { result, rerender } = render(step1);
    // a.txt resolved and the step continued; the next step stops on b.txt.
    rerender({
      s: state({ status: [conflicted("b.txt")], headInfo: { operation: null, sequence: null, rebase: rebase({ currentStep: 2 }) } }),
      edited: false,
    });
    expect(result.current().resolvedFiles).toBe(1);
    // b.txt resolved too.
    rerender({
      s: state({ status: [modified("b.txt")], headInfo: { operation: null, sequence: null, rebase: rebase({ currentStep: 2 }) } }),
      edited: false,
    });
    expect(result.current().resolvedFiles).toBe(2);
  });

  it("forgets the resolutions of an operation that ended", () => {
    const { result, rerender } = render(
      state({ status: [conflicted("a.txt")], headInfo: { operation: null, sequence: null, rebase: rebase() } }),
    );
    rerender({ s: state(), edited: false });
    rerender({ s: state({ status: [conflicted("c.txt")], headInfo: { operation: "Merge", sequence: null, rebase: null } }), edited: false });
    expect(result.current().resolvedFiles).toBe(0);
  });

  it("reports the merge editor's unsaved edits as they are when Abort is clicked", () => {
    const merging = state({ status: [conflicted("a.txt")], headInfo: { operation: "Merge", sequence: null, rebase: null } });
    const { result, rerender } = render(merging, true);
    expect(result.current().mergeEditorEdits).toBe(true);
    rerender({ s: merging, edited: false });
    expect(result.current().mergeEditorEdits).toBe(false);
  });

  it("counts an edit stop's changed message, not a blank one (A13)", () => {
    const stop = (editMessage: AbortWorkState["editMessage"]) =>
      state({ editMessage, headInfo: { operation: null, sequence: null, rebase: rebase({ editing: "c1", totalSteps: 1 }) } });
    const { result, rerender } = render(stop("changed"));
    expect(result.current().messageChanged).toBe(true);
    rerender({ s: stop("unchanged"), edited: false });
    expect(result.current().messageChanged).toBe(false);
    rerender({ s: stop("blank"), edited: false });
    expect(result.current().messageChanged).toBe(false);
  });
});
