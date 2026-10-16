import type { DiffLine } from "../ipc/RepoClient";
import type { Token } from "../lib/highlight";
import type { Segment } from "../lib/wordDiff";

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

/** A diff line's text: plain, word-diff marked, syntax colored, or both (marks win boundaries). */
export function DiffLineContent({
  line,
  tokens,
  segments,
}: {
  line: DiffLine;
  tokens: Token[] | undefined;
  segments: Segment[] | undefined;
}) {
  if (tokens === undefined && segments === undefined) return <>{line.content}</>;
  const text = segments === undefined ? line.content.replace(/\r?\n$/, "") : segments.map((s) => s.text).join("");
  return (
    <>
      {runs(text, tokens, segments).map((r, i) => {
        const style = r.color === undefined ? undefined : { color: r.color };
        return r.changed ? (
          <mark key={i} style={style}>{r.text}</mark>
        ) : (
          <span key={i} style={style}>{r.text}</span>
        );
      })}
    </>
  );
}
