import type { ReactNode } from "react";
import type { DiffLine } from "../ipc/RepoClient";
import type { Token } from "../lib/highlight";
import type { Segment } from "../lib/wordDiff";
import styles from "./DiffView.module.css";

type Run = { text: string; color: string | undefined; changed: boolean };

function runs(text: string, tokens: Token[] | undefined, segments: Segment[] | undefined): Run[] {
  const tokenRuns = tokens ?? [{ content: text, color: undefined }];
  const segs = segments ?? [{ text, changed: false }];
  const out: Run[] = [];
  let ti = 0;
  let tOffset = 0;
  for (const seg of segs) {
    let remaining = seg.text.length;
    while (remaining > 0 && ti < tokenRuns.length) {
      const tok = tokenRuns[ti];
      const take = Math.min(remaining, tok.content.length - tOffset);
      out.push({ text: tok.content.slice(tOffset, tOffset + take), color: tok.color, changed: seg.changed });
      remaining -= take;
      tOffset += take;
      if (tOffset >= tok.content.length) {
        ti++;
        tOffset = 0;
      }
    }
    if (remaining > 0) out.push({ text: seg.text.slice(seg.text.length - remaining), color: undefined, changed: seg.changed });
  }
  return out;
}

const WHITESPACE: Record<string, { kind: string; className: string }> = {
  " ": { kind: "space", className: styles.wsSpace },
  "\t": { kind: "tab", className: styles.wsTab },
  "\r": { kind: "cr", className: styles.wsCr },
};

/**
 * Where a line's trailing spaces and tabs start (before its line ending), so they can be
 * highlighted; `end` is where the line ending starts.
 */
function trailingWhitespace(text: string): { start: number; end: number } {
  const end = text.replace(/\r?\n$|\r$/, "").length;
  const start = text.slice(0, end).replace(/[ \t]+$/, "").length;
  return { start, end };
}

/**
 * `text` (starting at `offset` in the line) with every space, tab and carriage return in its own
 * span. The span keeps the original character, so copying and `textContent` are unchanged and a
 * tab still advances to the next tab stop; the visible marker is a CSS pseudo-element laid over
 * it, which takes no width and is never copied.
 */
function markWhitespace(text: string, offset: number, trailing: { start: number; end: number } | null): ReactNode[] {
  const out: ReactNode[] = [];
  let plainStart = 0;
  for (let i = 0; i < text.length; i++) {
    const ws = WHITESPACE[text[i]];
    if (ws === undefined) continue;
    if (plainStart < i) out.push(text.slice(plainStart, i));
    const at = offset + i;
    const isTrailing = trailing !== null && at >= trailing.start && at < trailing.end;
    out.push(
      <span
        key={i}
        className={isTrailing ? `${ws.className} ${styles.wsTrailing}` : ws.className}
        data-whitespace={ws.kind}
        data-trailing={isTrailing || undefined}
      >
        {text[i]}
      </span>,
    );
    plainStart = i + 1;
  }
  if (plainStart < text.length) out.push(text.slice(plainStart));
  return out;
}

/**
 * A diff line's text: plain, word-diff marked, syntax colored, or both (marks win boundaries).
 * With `showWhitespace` (#616), spaces, tabs and carriage returns get faint markers, and an added
 * line's trailing whitespace is highlighted.
 */
export function DiffLineContent({
  line,
  tokens,
  segments,
  showWhitespace = false,
}: {
  line: DiffLine;
  tokens: Token[] | undefined;
  segments: Segment[] | undefined;
  showWhitespace?: boolean;
}) {
  if (!showWhitespace && tokens === undefined && segments === undefined) return <>{line.content}</>;
  // With markers on, the line ending stays in the text so a carriage return gets its marker
  // (the highlighter's tokens stop before it; `runs` passes the rest through uncolored).
  const text =
    segments !== undefined
      ? segments.map((s) => s.text).join("")
      : showWhitespace
        ? line.content
        : line.content.replace(/\r?\n$/, "");
  const trailing = showWhitespace && line.origin === "Add" ? trailingWhitespace(text) : null;
  const parts = runs(text, tokens, segments);
  const offsets: number[] = [];
  let at = 0;
  for (const r of parts) {
    offsets.push(at);
    at += r.text.length;
  }
  return (
    <>
      {parts.map((r, i) => {
        const style = r.color === undefined ? undefined : { color: r.color };
        const content = showWhitespace ? markWhitespace(r.text, offsets[i], trailing) : r.text;
        return r.changed ? (
          <mark key={i} style={style}>{content}</mark>
        ) : (
          <span key={i} style={style}>{content}</span>
        );
      })}
    </>
  );
}
