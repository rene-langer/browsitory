import type { ConflictSegment } from "../ipc/RepoClient";

export type ConflictBlock = Extract<ConflictSegment, { kind: "Conflict" }>;

/** A side, or both in either order, taken for one conflicting block (#720). */
export type BlockChoice = "ours" | "theirs" | "oursThenTheirs" | "theirsThenOurs" | "base";

/** Where one block stands: nothing chosen yet, a side taken, or a result typed by hand. */
export type BlockState =
  | { kind: "unchosen" }
  | { kind: "chosen"; choice: BlockChoice }
  | { kind: "edited"; text: string };

export function choiceText(block: ConflictBlock, choice: BlockChoice): string {
  switch (choice) {
    case "ours":
      return block.ours;
    case "theirs":
      return block.theirs;
    case "oursThenTheirs":
      return block.ours + block.theirs;
    case "theirsThenOurs":
      return block.theirs + block.ours;
    case "base":
      return block.base;
  }
}

/** The block's result, or `null` while nothing is chosen. */
export function blockResult(block: ConflictBlock, state: BlockState): string | null {
  switch (state.kind) {
    case "unchosen":
      return null;
    case "chosen":
      return choiceText(block, state.choice);
    case "edited":
      return state.text;
  }
}

/** Whether `side`'s lines are in the block's result, alone or with the other side's. */
export function sideTaken(state: BlockState, side: "ours" | "theirs"): boolean {
  if (state.kind !== "chosen") return false;
  return state.choice === side || state.choice === "oursThenTheirs" || state.choice === "theirsThenOurs";
}

/**
 * Takes `side` into the block's result (#876, the panes' arrows): after the other side when that
 * one alone is taken, as is when `side` already is; over the base or a typed result it replaces
 * them, as the Accept buttons do.
 */
export function takeSide(state: BlockState, side: "ours" | "theirs"): BlockState {
  if (sideTaken(state, side)) return state;
  if (state.kind === "chosen" && state.choice === "ours") return { kind: "chosen", choice: "oursThenTheirs" };
  if (state.kind === "chosen" && state.choice === "theirs") return { kind: "chosen", choice: "theirsThenOurs" };
  return { kind: "chosen", choice: side };
}

function withFinalNewline(text: string): string {
  return text === "" || text.endsWith("\n") ? text : `${text}\n`;
}

/**
 * The whole file: clean runs as they are, each block's result, and git's conflict markers around
 * a block that has no result yet. Each segment carries its own line terminators (see
 * `parse_conflict_markers` in `crates/git-core/src/merge.rs`), so they are joined with "".
 */
export function assembleResult(segments: ConflictSegment[], states: BlockState[]): string {
  return segments
    .map((segment, index) => {
      if (segment.kind === "Clean") return segment.content;
      const result = blockResult(segment, states[index] ?? { kind: "unchosen" });
      if (result !== null) return result;
      return (
        `<<<<<<< ours\n${withFinalNewline(segment.ours)}||||||| base\n${withFinalNewline(segment.base)}` +
        `=======\n${withFinalNewline(segment.theirs)}>>>>>>> theirs\n`
      );
    })
    .join("");
}

/**
 * Whether the text still holds a conflict's start or end marker: a line of exactly seven `<` or
 * `>`, alone or followed by a space. A line of `=` alone (a Markdown heading underline) doesn't
 * count.
 */
export function hasConflictMarkers(text: string): boolean {
  return /^(<{7}|>{7})(?![<>])( |\r?$)/m.test(text);
}
