import type { Token } from "../lib/highlight";
import { markRuns, type Segment } from "../lib/wordDiff";

/**
 * One line of a merge editor side (#879): plain, coloured with its syntax `tokens`, and with the
 * words that differ from the other side (`marks`) in `<mark>`. Without either, the bare text.
 */
export function MarkedLine({
  text,
  tokens,
  marks,
}: {
  text: string;
  tokens: readonly Token[] | undefined;
  marks: readonly Segment[] | undefined;
}) {
  if (tokens === undefined && marks === undefined) return <>{text}</>;
  return (
    <>
      {markRuns(text, tokens, marks).map((run, i) => {
        const style = run.color === undefined ? undefined : { color: run.color };
        return run.changed ? (
          <mark key={i} style={style}>
            {run.text}
          </mark>
        ) : (
          <span key={i} style={style}>
            {run.text}
          </span>
        );
      })}
    </>
  );
}
