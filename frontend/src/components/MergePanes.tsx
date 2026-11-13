import { ArrowLeft, ArrowRight } from "lucide-react";
import { Fragment, memo, type CSSProperties, type ReactNode } from "react";
import type { ConflictSegment } from "../ipc/RepoClient";
import { segmentLines, type ConflictTokens } from "../lib/conflictHighlight";
import { segmentResult, sideTaken, type BlockState, type ConflictBlock } from "../lib/conflictMerge";
import type { BlockWordMarks } from "../lib/conflictWordDiff";
import type { Token } from "../lib/highlight";
import type { Segment } from "../lib/wordDiff";
import { MarkedLine } from "./MarkedLine";
import styles from "./MergePanes.module.css";

/**
 * The three-pane merge editor (#875): the whole file as ours | result | theirs. Every row (an
 * unchanged run, or one conflict's sides and result) spans all the panes, so the panes stay
 * aligned whatever a block's lengths, and they scroll as one. Show base adds the common
 * ancestor as a fourth pane on the left (#877). The result is editable in every row shown in
 * full (#878): an unresolved block holds git's conflict markers, and each line is marked with
 * where it came from.
 */

/** A pane's column. */
export type MergePane = "base" | "ours" | "result" | "theirs";

/** The panes, left to right. Every row lays out one cell per entry, so adding one is enough. */
const MERGE_PANES: readonly MergePane[] = ["ours", "result", "theirs"];
/** With Show base (#877): the base first, before the side each block starts from. */
const MERGE_PANES_WITH_BASE: readonly MergePane[] = ["base", ...MERGE_PANES];

/** Where a result line came from (#878): a side taken, typed, or a block still in conflict. */
export type LineOrigin = "ours" | "theirs" | "base" | "edited" | "conflict";

/** Where each segment starts in each pane, 1-based, by segment index; one more entry for the end. */
type Starts = Record<MergePane, number>[];

function paneStarts(segments: readonly ConflictSegment[], results: readonly string[]): Starts {
  const next = { ours: 1, result: 1, theirs: 1, base: 1 };
  const starts = segments.map((segment, index) => {
    const start = { ...next };
    if (segment.kind === "Clean") {
      const count = segmentLines(segment.content).length;
      next.ours += count;
      next.theirs += count;
      next.base += count;
    } else {
      next.ours += segmentLines(segment.ours).length;
      next.theirs += segmentLines(segment.theirs).length;
      next.base += segmentLines(segment.base).length;
    }
    next.result += segmentLines(results[index]).length;
    return start;
  });
  starts.push(next);
  return starts;
}

/** Each line's origin in a block's result, and the highlighting of the sides it took. */
function blockOrigins(
  segment: ConflictBlock,
  state: BlockState,
  index: number,
  tokens: ConflictTokens | null,
): { origins: LineOrigin | LineOrigin[]; tokens: readonly Token[][] | undefined } {
  if (state.kind === "unchosen") return { origins: "conflict", tokens: undefined };
  if (state.kind === "edited") return { origins: "edited", tokens: undefined };
  const lines = (side: "ours" | "theirs") => Array<LineOrigin>(segmentLines(segment[side]).length).fill(side);
  switch (state.choice) {
    case "ours":
    case "theirs":
    case "base":
      return { origins: state.choice, tokens: tokens?.[state.choice][index] };
    case "oursThenTheirs":
      return {
        origins: [...lines("ours"), ...lines("theirs")],
        tokens: tokens === null ? undefined : [...(tokens.ours[index] ?? []), ...(tokens.theirs[index] ?? [])],
      };
    case "theirsThenOurs":
      return {
        origins: [...lines("theirs"), ...lines("ours")],
        tokens: tokens === null ? undefined : [...(tokens.theirs[index] ?? []), ...(tokens.ours[index] ?? [])],
      };
  }
}

/** One line with its number; memoised, so typing re-renders only the lines it changes (#878). */
const Line = memo(function Line({
  line,
  tokens,
  number,
  origin,
  marks,
}: {
  line: string;
  tokens: readonly Token[] | undefined;
  number: number;
  origin: LineOrigin | undefined;
  /** The words that differ from the other side's line (#879). */
  marks: readonly Segment[] | undefined;
}) {
  return (
    <span className={styles.line} data-line={number} data-origin={origin}>
      <span className={styles.number} aria-hidden="true">
        {number}
      </span>
      <span className={styles.text}>
        <MarkedLine text={line} tokens={tokens} marks={marks} />
      </span>
    </span>
  );
});

/** Lines `from` … `to` of `text`, each with its number in the pane (counted from `start`). */
const Lines = memo(function Lines({
  text,
  tokens,
  start,
  from = 0,
  to,
  origins,
  marks,
}: {
  text: string;
  tokens: readonly Token[][] | undefined;
  start: number;
  from?: number;
  to?: number;
  /** Every line's origin, or one for all of them. */
  origins?: LineOrigin | readonly LineOrigin[];
  /** Every line's changed words (#879), by line. */
  marks?: readonly (Segment[] | undefined)[];
}) {
  const lines = segmentLines(text).slice(from, to);
  return (
    <pre className={styles.lines}>
      {lines.map((line, i) => (
        <Line
          key={i}
          line={line}
          tokens={tokens?.[from + i]}
          number={start + from + i}
          origin={typeof origins === "string" ? origins : origins?.[from + i]}
          marks={marks?.[from + i]}
        />
      ))}
    </pre>
  );
});

/**
 * A run of the result, editable (#878): a text field over its numbered, highlighted lines, the
 * field's text transparent so the lines show through and stay level with the side panes.
 */
function EditableLines({
  index,
  text,
  tokens,
  start,
  origins,
  onEdit,
}: {
  index: number;
  text: string;
  tokens: readonly Token[][] | undefined;
  start: number;
  origins: LineOrigin | readonly LineOrigin[] | undefined;
  onEdit(index: number, text: string): void;
}) {
  return (
    <div className={styles.editable}>
      <div aria-hidden="true">
        <Lines text={text} tokens={tokens} start={start} origins={origins} />
      </div>
      <textarea
        aria-label="Result"
        className={styles.input}
        spellCheck={false}
        value={text}
        onChange={(event) => onEdit(index, event.target.value)}
      />
    </div>
  );
}

export interface MergePanesProps {
  segments: readonly ConflictSegment[];
  stateAt(index: number): BlockState;
  /** Each pane's heading, e.g. "Ours (main)". */
  labels: Record<Exclude<MergePane, "base">, string>;
  tokens: ConflictTokens | null;
  showBase: boolean;
  /** Unchanged runs shown in full, by segment index. */
  expanded: ReadonlySet<number>;
  onExpand(index: number): void;
  /** Lines of an unchanged run kept next to a conflict when the rest is collapsed. */
  contextLines: number;
  /** The block's focusable section, with its title and choices, around `children`. */
  renderBlock(segment: ConflictBlock, index: number, className: string, children: ReactNode): ReactNode;
  /** Text typed into the result of a segment, a block or an unchanged run (#878). */
  onEdit(index: number, text: string): void;
  /** Takes a side into the block's result: the panes' arrows (#876). */
  onTake(index: number, side: "ours" | "theirs"): void;
  /** The block's title, e.g. "Conflict 1 of 3", for its arrows. */
  blockTitle(index: number): string;
  /** Each block's changed words between ours and theirs (#879), by segment index. */
  wordMarks: readonly (BlockWordMarks | null)[];
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
  onTake,
  blockTitle,
  wordMarks,
}: MergePanesProps) {
  const results = segments.map((segment, index) => segmentResult(segment, stateAt(index)));
  const starts = paneStarts(segments, results);
  const end = starts[segments.length];
  const digits = String(Math.max(end.ours, end.result, end.theirs, end.base)).length;
  const panes = showBase ? MERGE_PANES_WITH_BASE : MERGE_PANES;
  const headings: Record<MergePane, string> = { base: "Base", ...labels };
  const gridStyle = {
    "--merge-panes": panes.length,
    // Room for the numbers, the space after them and the origin bar before them (#878).
    "--merge-gutter": `calc(${Math.max(digits, 2) + 1}ch + 4px)`,
  } as CSSProperties;

  /**
   * One row, a cell per pane. A block's side cells are named after their pane for screen readers
   * (#880); an unchanged run's (`run`, its segment index) are hidden from them, since the result
   * holds the same lines and the file would otherwise be read three or four times.
   */
  const row = (
    key: string,
    cells: Record<MergePane, ReactNode>,
    chosen: ReadonlySet<MergePane> = new Set(),
    run?: number,
  ) => (
    <div key={key} className={styles.row} data-run={run}>
      {panes.map((pane) => {
        const side = pane !== "result";
        return (
          <div
            key={pane}
            data-pane={pane}
            data-chosen={chosen.has(pane) ? "true" : undefined}
            role={side && run === undefined ? "group" : undefined}
            aria-label={side && run === undefined ? headings[pane] : undefined}
            aria-hidden={side && run !== undefined ? true : undefined}
            className={`${styles.cell} ${chosen.has(pane) ? styles.chosen : ""}`}
          >
            {cells[pane]}
          </div>
        );
      })}
    </div>
  );

  /** Part of an unchanged run, read-only: the lines next to a collapsed middle. */
  const cleanRow = (key: string, content: string, index: number, from: number, to: number) => {
    const cleanTokens = tokens?.ours[index];
    const start = starts[index];
    return row(
      key,
      {
        base: <Lines text={content} tokens={cleanTokens} start={start.base} from={from} to={to} />,
        ours: <Lines text={content} tokens={cleanTokens} start={start.ours} from={from} to={to} />,
        result: <Lines text={content} tokens={cleanTokens} start={start.result} from={from} to={to} />,
        theirs: <Lines text={content} tokens={cleanTokens} start={start.theirs} from={from} to={to} />,
      },
      undefined,
      index,
    );
  };

  /** A whole unchanged run, its result editable (#878). */
  const editableCleanRow = (content: string, index: number) => {
    const cleanTokens = tokens?.ours[index];
    const start = starts[index];
    const edited = stateAt(index).kind === "edited";
    // `data-run` lets F7 and Shift+F7 go on from this run while its result has the focus (#879).
    return row(
      String(index),
      {
        base: <Lines text={content} tokens={cleanTokens} start={start.base} />,
        ours: <Lines text={content} tokens={cleanTokens} start={start.ours} />,
        result: (
          <EditableLines
            index={index}
            text={results[index]}
            tokens={edited ? undefined : cleanTokens}
            start={start.result}
            origins={edited ? "edited" : undefined}
            onEdit={onEdit}
          />
        ),
        theirs: <Lines text={content} tokens={cleanTokens} start={start.theirs} />,
      },
      undefined,
      index,
    );
  };

  const renderClean = (content: string, index: number) => {
    const count = segmentLines(content).length;
    const head = index > 0 ? contextLines : 0;
    const tail = index < segments.length - 1 ? contextLines : 0;
    const hidden = count - head - tail;
    // Typed over: shown in full, as it is now.
    if (expanded.has(index) || hidden <= 2 || stateAt(index).kind === "edited") return editableCleanRow(content, index);
    return (
      <Fragment key={index}>
        {head > 0 && cleanRow(`${index}-head`, content, index, 0, head)}
        <button className={styles.expand} data-run={index} onClick={() => onExpand(index)}>
          Show {hidden} unchanged lines
        </button>
        {tail > 0 && cleanRow(`${index}-tail`, content, index, count - tail, count)}
      </Fragment>
    );
  };

  const side = (
    text: string,
    sideTokens: readonly Token[][] | undefined,
    start: number,
    marks: readonly (Segment[] | undefined)[] | undefined,
  ) =>
    text === "" ? (
      <p className={styles.placeholder}>Empty: no lines on this side</p>
    ) : (
      <Lines text={text} tokens={sideTokens} start={start} marks={marks} />
    );

  /** A block's side with its arrow (#876) on the edge next to the result. */
  const sideWithArrow = (segment: ConflictBlock, index: number, which: "ours" | "theirs") => {
    const taken = sideTaken(stateAt(index), which);
    const lines = which === "ours" ? "our" : "their";
    return (
      <>
        {side(segment[which], tokens?.[which][index], starts[index][which], wordMarks[index]?.[which])}
        <button
          className={`${styles.take} ${which === "ours" ? styles.takeOurs : styles.takeTheirs}`}
          aria-label={which === "ours" ? "Take ours" : "Take theirs"}
          disabled={taken}
          title={
            taken
              ? `${blockTitle(index)}: ${lines} lines are in the result.`
              : `${blockTitle(index)}: put ${lines} lines into the result, after the other side's if it is taken.`
          }
          onClick={() => onTake(index, which)}
        >
          {which === "ours" ? <ArrowRight size={14} aria-hidden="true" /> : <ArrowLeft size={14} aria-hidden="true" />}
        </button>
      </>
    );
  };

  const renderConflict = (segment: ConflictBlock, index: number) => {
    const state = stateAt(index);
    const result = results[index];
    const start = starts[index];
    const choice = state.kind === "chosen" ? state.choice : null;
    const chosen = new Set<MergePane>();
    if (choice === "ours" || choice === "oursThenTheirs" || choice === "theirsThenOurs") chosen.add("ours");
    if (choice === "theirs" || choice === "oursThenTheirs" || choice === "theirsThenOurs") chosen.add("theirs");
    if (choice === "base") chosen.add("base");
    const highlight = blockOrigins(segment, state, index, tokens);
    return renderBlock(
      segment,
      index,
      styles.block,
      <>
        {row(
          "panes",
          {
            base:
              segment.base === "" ? (
                <p className={styles.placeholder}>Empty: no common lines</p>
              ) : (
                <Lines text={segment.base} tokens={tokens?.base[index]} start={start.base} />
              ),
            ours: sideWithArrow(segment, index, "ours"),
            result: (
              <>
                <EditableLines
                  index={index}
                  text={result}
                  tokens={highlight.tokens}
                  start={start.result}
                  origins={highlight.origins}
                  onEdit={onEdit}
                />
                {result === "" && <p className={styles.placeholder}>Empty: the block&apos;s lines are removed.</p>}
              </>
            ),
            theirs: sideWithArrow(segment, index, "theirs"),
          },
          chosen,
        )}
      </>,
    );
  };

  return (
    <div className={styles.panes} style={gridStyle}>
      <div className={`${styles.row} ${styles.header}`}>
        {panes.map((pane) => (
          <div key={pane} className={styles.label} data-pane-header={pane}>
            {headings[pane]}
          </div>
        ))}
      </div>
      {segments.map((segment, index) =>
        segment.kind === "Clean" ? renderClean(segment.content, index) : renderConflict(segment, index),
      )}
    </div>
  );
}
