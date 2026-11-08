import type { DiffHunk, WordWrap } from "../ipc/RepoClient";

/** A line longer than this (minified or generated code) makes `Auto` wrap the diff. */
export const LONG_LINE_CHARS = 300;

const PROSE_EXTENSIONS = new Set(["md", "markdown", "mdx", "txt", "text", "rst", "adoc", "asciidoc", "org", "tex"]);

function isProse(path: string): boolean {
  const name = path.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 && PROSE_EXTENSIONS.has(name.slice(dot + 1).toLowerCase());
}

function hasLongLine(hunks: DiffHunk[]): boolean {
  return hunks.some((hunk) => hunk.lines.some((line) => line.content.replace(/\r?\n$/, "").length > LONG_LINE_CHARS));
}

/**
 * Whether a diff's lines wrap at the pane's width (#444). `On`/`Off` decide alone. `Auto` wraps
 * the split view (each side has half the width), prose files (Markdown, plain text and the like)
 * and any diff with a line longer than `LONG_LINE_CHARS`; code in the unified view scrolls
 * sideways so its indentation stays readable.
 */
export function wrapLines(
  setting: WordWrap,
  { split, path, hunks }: { split: boolean; path: string | undefined; hunks: DiffHunk[] | null },
): boolean {
  if (setting === "On") return true;
  if (setting === "Off") return false;
  if (split) return true;
  if (path !== undefined && isProse(path)) return true;
  return hunks !== null && hasLongLine(hunks);
}
