import { Fragment, type CSSProperties, type ReactNode } from "react";
import type { ConflictSegment } from "../ipc/RepoClient";
import { segmentLines, type ConflictTokens } from "../lib/conflictHighlight";
import { blockResult, type BlockState, type ConflictBlock } from "../lib/conflictMerge";
import type { Token } from "../lib/highlight";
import styles from "./MergePanes.module.css";

/**
 * The three-pane merge editor (#875): the whole file as ours | result | theirs. Every row (an
 * unchanged run, or one conflict's sides and result) spans all the panes, so the panes stay
 * aligned whatever a block's lengths, and they scroll as one.
 */

/** A pane's column; the base view (#877) is meant to join as a fourth. */
export type MergePane = "ours" | "result" | "theirs";

/** The panes, left to right. Every row lays out one cell per entry, so adding one is enough. */
const MERGE_PANES: readonly MergePane[] = ["ours", "result", "theirs"];

/** Where each segment starts in each pane, 1-based, by segment index; one more entry for the end. */
type Starts = Record<MergePane | "base", number>[];

function paneStarts(segments: readonly ConflictSegment[], results: readonly (string | null)[]): Starts {
  const next = { ours: 1, result: 1, theirs: 1, base: 1 };
  const starts = segments.map((segment, index) => {
    const start = { ...next };
    if (segment.kind === "Clean") {
      const count = segmentLines(segment.content).length;
      next.ours += count;
      next.result += count;
      next.theirs += count;
      next.base += count;
    } else {
      next.ours += segmentLines(segment.ours).length;
      next.theirs += segmentLines(segment.theirs).length;
      next.base += segmentLines(segment.base).length;
      next.result += segmentLines(results[index] ?? "").length;
    }
    return start;
  });
  starts.push(next);
  return starts;
}

function TokenLine({ tokens }: { tokens: readonly Token[] }) {
  return (
    <>
      {tokens.map((token, j) => (
        <span key={j} style={token.color !== undefined ? { color: token.color } : undefined}>
          {token.content}
        </span>
      ))}
    </>
  );
}

/** Lines `from` … `to` of `text`, each with its number in the pane (counted from `start`). */
function Lines({
  text,
  tokens,
  start,
  from = 0,
  to,
}: {
  text: string;
  tokens: readonly Token[][] | undefined;
  start: number;
  from?: number;
  to?: number;
}) {
  const lines = segmentLines(text).slice(from, to);
  return (
    <pre className={styles.lines}>
      {lines.map((line, i) => {
        const lineTokens = tokens?.[from + i];
        return (
          <span key={i} className={styles.line} data-line={start + from + i}>
            <span className={styles.number} aria-hidden="true">
              {start + from + i}
            </span>
            <span className={styles.text}>
              {lineTokens === undefined ? line : <TokenLine tokens={lineTokens} />}
            </span>
          </span>
        );
      })}
    </pre>
  );
}

export interface MergePanesProps {
  segments: readonly ConflictSegment[];
  stateAt(index: number): BlockState;
  /** Each pane's heading, e.g. "Ours (main)". */
  labels: Record<MergePane, string>;
  tokens: ConflictTokens | null;
  showBase: boolean;
  /** Unchanged runs shown in full, by segment index. */
  expanded: ReadonlySet<number>;
  onExpand(index: number): void;
  /** Lines of an unchanged run kept next to a conflict when the rest is collapsed. */
  contextLines: number;
  /** The block's focusable section, with its title and choices, around `children`. */
  renderBlock(segment: ConflictBlock, index: number, className: string, children: ReactNode): ReactNode;
  onEdit(index: number, text: string): void;
}

export function MergePanes({
  segments,
  stateAt,
  labels,
  tokens,
  showBase,
  expanded,
  onExpand,
  contextLines,
  renderBlock,
  onEdit,
}: MergePanesProps) {
  const results = segments.map((segment, index) =>
    segment.kind === "Clean" ? null : blockResult(segment, stateAt(index)),
  );
  const starts = paneStarts(segments, results);
  const end = starts[segments.length];
  const digits = String(Math.max(end.ours, end.result, end.theirs, end.base)).length;
  const gridStyle = {
    "--merge-panes": MERGE_PANES.length,
    "--merge-gutter": `${Math.max(digits, 2) + 1}ch`,
  } as CSSProperties;

  const row = (key: string, cells: Record<MergePane, ReactNode>, chosen: ReadonlySet<MergePane> = new Set()) => (
    <div key={key} className={styles.row}>
      {MERGE_PANES.map((pane) => (
        <div key={pane} data-pane={pane} className={`${styles.cell} ${chosen.has(pane) ? styles.chosen : ""}`}>
          {cells[pane]}
        </div>
      ))}
    </div>
  );

  const cleanRow = (key: string, content: string, index: number, from: number, to: number) => {
    const cleanTokens = tokens?.ours[index];
    const start = starts[index];
    return row(key, {
      ours: <Lines text={content} tokens={cleanTokens} start={start.ours} from={from} to={to} />,
      result: <Lines text={content} tokens={cleanTokens} start={start.result} from={from} to={to} />,
      theirs: <Lines text={content} tokens={cleanTokens} start={start.theirs} from={from} to={to} />,
    });
  };

  const renderClean = (content: string, index: number) => {
    const count = segmentLines(content).length;
    const head = index > 0 ? contextLines : 0;
    const tail = index < segments.length - 1 ? contextLines : 0;
    const hidden = count - head - tail;
    if (expanded.has(index) || hidden <= 2) return cleanRow(String(index), content, index, 0, count);
    return (
      <Fragment key={index}>
        {head > 0 && cleanRow(`${index}-head`, content, index, 0, head)}
        <button className={styles.expand} onClick={() => onExpand(index)}>
          Show {hidden} unchanged lines
        </button>
        {tail > 0 && cleanRow(`${index}-tail`, content, index, count - tail, count)}
      </Fragment>
    );
  };

  const side = (text: string, sideTokens: readonly Token[][] | undefined, start: number) =>
    text === "" ? (
      <p className={styles.placeholder}>Empty: no lines on this side</p>
    ) : (
      <Lines text={text} tokens={sideTokens} start={start} />
    );

  const renderConflict = (segment: ConflictBlock, index: number) => {
    const state = stateAt(index);
    const result = results[index] ?? null;
    const start = starts[index];
    const choice = state.kind === "chosen" ? state.choice : null;
    const chosen = new Set<MergePane>();
    if (choice === "ours" || choice === "oursThenTheirs" || choice === "theirsThenOurs") chosen.add("ours");
    if (choice === "theirs" || choice === "oursThenTheirs" || choice === "theirsThenOurs") chosen.add("theirs");
    const resultLines = result === null ? 0 : segmentLines(result).length;
    return renderBlock(
      segment,
      index,
      styles.block,
      <>
        {showBase && (
          <div className={`${styles.base} ${choice === "base" ? styles.chosen : ""}`}>
            <div className={styles.label}>Base</div>
            {segment.base === "" ? (
              <p className={styles.placeholder}>Empty: no common lines</p>
            ) : (
              <Lines text={segment.base} tokens={tokens?.base[index]} start={start.base} />
            )}
          </div>
        )}
        {row(
          "panes",
          {
            ours: side(segment.ours, tokens?.ours[index], start.ours),
            result: (
              <>
                <div className={styles.resultLines}>
                  <div className={styles.gutter} aria-hidden="true">
                    {Array.from({ length: resultLines }, (_, i) => (
                      <span key={i} className={styles.number} data-line={start.result + i}>
                        {start.result + i}
                      </span>
                    ))}
                  </div>
                  <textarea
                    aria-label="Result"
                    className={styles.result}
                    spellCheck={false}
                    wrap="off"
                    placeholder="Not chosen yet: take a side, or type the result."
                    rows={Math.max(1, resultLines)}
                    value={result ?? ""}
                    onChange={(event) => onEdit(index, event.target.value)}
                  />
                </div>
                {result === "" && <p className={styles.placeholder}>Empty: the block&apos;s lines are removed.</p>}
              </>
            ),
            theirs: side(segment.theirs, tokens?.theirs[index], start.theirs),
          },
          chosen,
        )}
      </>,
    );
  };

  return (
    <div className={styles.panes} style={gridStyle}>
      <div className={`${styles.row} ${styles.header}`}>
        {MERGE_PANES.map((pane) => (
          <div key={pane} className={styles.label} data-pane-header={pane}>
            {labels[pane]}
          </div>
        ))}
      </div>
      {segments.map((segment, index) =>
        segment.kind === "Clean" ? renderClean(segment.content, index) : renderConflict(segment, index),
      )}
    </div>
  );
}
