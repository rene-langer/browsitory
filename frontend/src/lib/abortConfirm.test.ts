import { describe, expect, it } from "vitest";
import { abortConfirm, mergeMessageConflicts, NO_ABORT_WORK } from "./abortConfirm";

// Abort of a paused merge, rebase, cherry-pick or revert asks first when it would lose work
// that Undo can't restore, and not when nothing was touched (#979).
describe("abortConfirm", () => {
  it("asks nothing when nothing was touched", () => {
    for (const kind of ["Merge", "CherryPick", "Revert", "Rebase"] as const) {
      expect(abortConfirm({ kind }, NO_ABORT_WORK)).toBeNull();
    }
    expect(abortConfirm({ kind: "Edit", commitId: "1a2b3c4d5e" }, NO_ABORT_WORK)).toBeNull();
    expect(abortConfirm({ kind: "Split", commitId: "1a2b3c4d5e" }, NO_ABORT_WORK)).toBeNull();
  });

  it("names the resolutions a merge abort loses", () => {
    expect(abortConfirm({ kind: "Merge" }, { ...NO_ABORT_WORK, resolvedFiles: 1 })).toEqual({
      title: "Abort the merge?",
      body: "Your resolutions in 1 file are lost, and Undo can't bring them back.",
      confirmLabel: "Abort merge",
      cancelLabel: "Cancel",
    });
    expect(abortConfirm({ kind: "CherryPick" }, { ...NO_ABORT_WORK, resolvedFiles: 3 })).toEqual({
      title: "Abort the cherry-pick?",
      body: "Your resolutions in 3 files are lost, and Undo can't bring them back.",
      confirmLabel: "Abort cherry-pick",
      cancelLabel: "Cancel",
    });
    expect(abortConfirm({ kind: "Revert" }, { ...NO_ABORT_WORK, resolvedFiles: 2 })?.confirmLabel).toBe("Abort revert");
  });

  it("names unsaved merge editor edits, alone and with resolutions", () => {
    expect(abortConfirm({ kind: "Rebase" }, { ...NO_ABORT_WORK, mergeEditorEdits: true })).toEqual({
      title: "Abort the rebase?",
      body: "Your unsaved edits in the merge editor are lost, and Undo can't bring them back.",
      confirmLabel: "Abort rebase",
      cancelLabel: "Cancel",
    });
    expect(
      abortConfirm({ kind: "Merge" }, { ...NO_ABORT_WORK, resolvedFiles: 2, mergeEditorEdits: true })?.body,
    ).toBe("Your resolutions in 2 files and your unsaved edits in the merge editor are lost, and Undo can't bring them back.");
  });

  it("names an edit stop's changes and new message, worded as cancelling the edit", () => {
    expect(abortConfirm({ kind: "Edit", commitId: "1a2b3c4d5e" }, { ...NO_ABORT_WORK, changedFiles: true })).toEqual({
      title: "Cancel the edit of 1a2b3c4?",
      body: "Your changes to the files are lost, and Undo can't bring them back.",
      confirmLabel: "Cancel edit",
      cancelLabel: "Keep editing",
    });
    expect(
      abortConfirm({ kind: "Edit", commitId: "1a2b3c4d5e" }, { ...NO_ABORT_WORK, changedFiles: true, messageChanged: true })
        ?.body,
    ).toBe("Your changes to the files and the new commit message are lost, and Undo can't bring them back.");
    expect(abortConfirm({ kind: "Edit", commitId: "1a2b3c4d5e" }, { ...NO_ABORT_WORK, messageChanged: true })?.body).toBe(
      "The new commit message is lost, and Undo can't bring it back.",
    );
  });

  it("names the parts a split abort loses", () => {
    expect(abortConfirm({ kind: "Split", commitId: "1a2b3c4d5e" }, { ...NO_ABORT_WORK, splitParts: 1 })).toEqual({
      title: "Cancel the split of 1a2b3c4?",
      body: "The part already committed is lost, and Undo can't bring it back.",
      confirmLabel: "Cancel split",
      cancelLabel: "Keep splitting",
    });
    expect(
      abortConfirm({ kind: "Split", commitId: "1a2b3c4d5e" }, { ...NO_ABORT_WORK, splitParts: 2, changedFiles: true })?.body,
    ).toBe("The 2 parts already committed and your changes to the files are lost, and Undo can't bring them back.");
  });

  it("lists three kinds of work with commas", () => {
    expect(
      abortConfirm(
        { kind: "Rebase" },
        { ...NO_ABORT_WORK, resolvedFiles: 1, mergeEditorEdits: true, changedFiles: true },
      )?.body,
    ).toBe(
      "Your resolutions in 1 file, your unsaved edits in the merge editor and your changes to the files are lost, and Undo can't bring them back.",
    );
  });
});

describe("mergeMessageConflicts", () => {
  it("reads the conflicted paths git lists in MERGE_MSG", () => {
    expect(mergeMessageConflicts("Merge branch 'x'\n\n# Conflicts:\n#\tsrc/a.js\n#\tb.txt\n")).toEqual(["src/a.js", "b.txt"]);
    // The older format, without the comment signs.
    expect(mergeMessageConflicts("Merge branch 'x'\n\nConflicts:\n\ta.txt\n")).toEqual(["a.txt"]);
  });

  it("reads none from a message without the list, or none at all", () => {
    expect(mergeMessageConflicts("Merge branch 'x'\n")).toEqual([]);
    expect(mergeMessageConflicts(null)).toEqual([]);
    // Text after the list is not a path.
    expect(mergeMessageConflicts("M\n\n# Conflicts:\n#\ta.txt\n#\n# It looks like you may be committing a merge.\n")).toEqual([
      "a.txt",
    ]);
  });
});
