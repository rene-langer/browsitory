import { startTransition, useEffect, useState } from "react";
import type { Blame } from "../ipc/RepoClient";
import { formatAbsoluteDate, formatShortDate } from "../lib/formatDate";
import { HIGHLIGHT_LINE_LIMIT, highlightLines, type Token } from "../lib/highlight";
import { isAbortError } from "../lib/highlightWorkerProtocol";
import { languageForPath, type HighlightLanguage } from "../lib/languageForPath";
import { logFailure } from "../lib/logger";
import { markRuns } from "../lib/wordDiff";
import type { SelectedRow } from "../state/useAppState";
import { useDateFormat } from "../state/useDateFormat";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import styles from "./BlameView.module.css";
import { useVirtualRows } from "./useVirtualRows";

/** Why a file has no blame (#1158): it is over the size limit, or binary. */
export function BlameUnavailableNote({ message }: { message: string }) {
  return <p className={styles.unavailable}>{message}</p>;
}

/**
 * The file's lines colored like the diff (#986): the whole file is tokenized as one block, so
 * multi-line strings and comments come out right. `null` while loading, when highlighting is
 * off, over the diff's line cap, for an unknown language, or on failure (plain text then).
 */
function useBlameTokens(blame: Blame, lang: HighlightLanguage | null, enabled: boolean): Token[][] | null {
  const [tokens, setTokens] = useState<{ blame: Blame; tokens: Token[][] } | null>(null);
  const { lines } = blame;
  useEffect(() => {
    if (!enabled || lang === null || lines.length === 0 || lines.length > HIGHLIGHT_LINE_LIMIT) return;
    let cancelled = false;
    const controller = new AbortController();
    highlightLines(lang, lines.map((line) => line.content).join("\n"), controller.signal).then(
      (next) => {
        // A transition: rendering a long file's tokens yields to input instead of blocking it.
        if (!cancelled) startTransition(() => setTokens({ blame, tokens: next }));
      },
      (error: unknown) => {
        if (!cancelled && !isAbortError(error)) logFailure("Syntax highlighting failed", error);
      },
    );
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [blame, lines, lang, enabled]);
  return enabled && tokens !== null && tokens.blame === blame ? tokens.tokens : null;
}

// The table's rows, from the element around it that carries the window's padding.
const ROW_SELECTOR = ":scope > table > tbody > tr";

/**
 * A file's blame (#986): line number and code (indentation kept, colored like the diff), with
 * the commit's short id, author and date once at the start of each run of lines from the same
 * commit. Only the short id is a link: it selects that commit. Only the lines in view render
 * (#1158); padding around the table stands in for the others.
 */
export function BlameView({
  blame,
  path,
  onSelectRow,
}: {
  blame: Blame;
  /** The file's path, for its syntax highlighting; without it the code is plain text. */
  path?: string;
  onSelectRow: (row: SelectedRow) => void;
}) {
  const dateFormat = useDateFormat();
  const { settings } = useDiffSettingsContext();
  const tokens = useBlameTokens(blame, path === undefined ? null : languageForPath(path), settings.syntaxHighlight);
  const { commits, lines } = blame;
  const [list, setList] = useState<HTMLDivElement | null>(null);
  const { start, end, padTop, padBottom } = useVirtualRows(list, lines.length, ROW_SELECTOR);
  return (
    <div ref={setList} style={{ paddingTop: padTop, paddingBottom: padBottom }}>
      <table className={styles.table}>
        <tbody>
          {lines.slice(start, end).map((line, offset) => {
            const index = start + offset;
            const lineNumber = index + 1;
            const commit = commits[line.commit];
            const runStart = index === 0 || lines[index - 1].commit !== line.commit;
            const lineTokens = tokens?.[index];
            return (
              <tr
                key={lineNumber}
                data-line-number={lineNumber}
                className={runStart && index > 0 ? `${styles.row} ${styles.runStart}` : styles.row}
              >
                <td className={styles.commit}>
                  {runStart && (
                    <button
                      type="button"
                      className={styles.hash}
                      title={`Go to commit ${commit.shortId}`}
                      onClick={() => onSelectRow({ commitId: commit.commitId })}
                    >
                      {commit.shortId}
                    </button>
                  )}
                </td>
                <td className={styles.author} title={runStart ? commit.authorName : undefined}>
                  {runStart && <span className={styles.authorName}>{commit.authorName}</span>}
                </td>
                <td className={styles.date}>
                  {runStart && (
                    <span title={formatAbsoluteDate(commit.timestamp, dateFormat)}>
                      {formatShortDate(commit.timestamp, dateFormat)}
                    </span>
                  )}
                </td>
                <td className={styles.lineNumber}>{lineNumber}</td>
                <td className={styles.code} data-blame-code="">
                  {lineTokens === undefined
                    ? line.content
                    : markRuns(line.content, lineTokens, undefined).map((run, i) => (
                        <span key={i} style={run.color === undefined ? undefined : { color: run.color }}>
                          {run.text}
                        </span>
                      ))}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
