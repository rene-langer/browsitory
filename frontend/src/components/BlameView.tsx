import { startTransition, useEffect, useState } from "react";
import type { BlameLine } from "../ipc/RepoClient";
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

/**
 * The file's lines colored like the diff (#986): the whole file is tokenized as one block, so
 * multi-line strings and comments come out right. `null` while loading, when highlighting is
 * off, over the diff's line cap, for an unknown language, or on failure (plain text then).
 */
function useBlameTokens(lines: BlameLine[], lang: HighlightLanguage | null, enabled: boolean): Token[][] | null {
  const [tokens, setTokens] = useState<{ lines: BlameLine[]; tokens: Token[][] } | null>(null);
  useEffect(() => {
    if (!enabled || lang === null || lines.length === 0 || lines.length > HIGHLIGHT_LINE_LIMIT) return;
    let cancelled = false;
    const controller = new AbortController();
    highlightLines(lang, lines.map((line) => line.content).join("\n"), controller.signal).then(
      (next) => {
        // A transition: rendering a long file's tokens yields to input instead of blocking it.
        if (!cancelled) startTransition(() => setTokens({ lines, tokens: next }));
      },
      (error: unknown) => {
        if (!cancelled && !isAbortError(error)) logFailure("Syntax highlighting failed", error);
      },
    );
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [lines, lang, enabled]);
  return enabled && tokens !== null && tokens.lines === lines ? tokens.tokens : null;
}

/**
 * A file's blame (#986): line number and code (indentation kept, colored like the diff), with
 * the commit's short id, author and date once at the start of each run of lines from the same
 * commit. Only the short id is a link: it selects that commit.
 */
export function BlameView({
  lines,
  path,
  onSelectRow,
}: {
  lines: BlameLine[];
  /** The file's path, for its syntax highlighting; without it the code is plain text. */
  path?: string;
  onSelectRow: (row: SelectedRow) => void;
}) {
  const dateFormat = useDateFormat();
  const { settings } = useDiffSettingsContext();
  const tokens = useBlameTokens(lines, path === undefined ? null : languageForPath(path), settings.syntaxHighlight);
  return (
    <table className={styles.table}>
      <tbody>
        {lines.map((line, index) => {
          const runStart = index === 0 || lines[index - 1].commitId !== line.commitId;
          const lineTokens = tokens?.[index];
          return (
            <tr
              key={line.lineNumber}
              data-line-number={line.lineNumber}
              className={runStart && index > 0 ? `${styles.row} ${styles.runStart}` : styles.row}
            >
              <td className={styles.commit}>
                {runStart && (
                  <button
                    type="button"
                    className={styles.hash}
                    title={`Go to commit ${line.shortId}`}
                    onClick={() => onSelectRow({ commitId: line.commitId })}
                  >
                    {line.shortId}
                  </button>
                )}
              </td>
              <td className={styles.author} title={runStart ? line.authorName : undefined}>
                {runStart && <span className={styles.authorName}>{line.authorName}</span>}
              </td>
              <td className={styles.date}>
                {runStart && (
                  <span title={formatAbsoluteDate(line.timestamp, dateFormat)}>
                    {formatShortDate(line.timestamp, dateFormat)}
                  </span>
                )}
              </td>
              <td className={styles.lineNumber}>{line.lineNumber}</td>
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
  );
}
