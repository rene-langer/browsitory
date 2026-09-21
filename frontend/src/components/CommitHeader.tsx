import { useEffect, useState } from "react";
import type { GraphCommit, RepoClient } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { formatAbsoluteDate, formatRelativeDate } from "../lib/formatDate";
import styles from "./CommitHeader.module.css";

/**
 * Metadata block at the top of a selected commit's diff pane: subject, body, author, date, full
 * SHA (copyable) and parents. Author/date/parents come from the already-loaded `GraphCommit`;
 * only the full message body needs a `RepoClient` round trip.
 */
export function CommitHeader({
  repoPath,
  client,
  commitId,
  commit,
  knownCommitIds,
  onSelectRow,
}: {
  repoPath: string;
  client: RepoClient;
  commitId: string;
  // Undefined when the commit is outside the loaded history window.
  commit: GraphCommit | undefined;
  // Ids present in the loaded history; a parent outside it cannot be selected.
  knownCommitIds: ReadonlySet<string>;
  onSelectRow: (row: SelectedRow) => void;
}) {
  const [message, setMessage] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let ignore = false;
    Promise.resolve()
      .then(() => client.getCommitMessage(repoPath, commitId))
      .then((next) => {
        if (!ignore) setMessage(next);
      })
      .catch(() => {
        // The subject from the graph row is still shown; the body is a nicety.
      });
    return () => {
      ignore = true;
    };
  }, [client, repoPath, commitId]);

  const subject = commit?.summary ?? message?.split("\n")[0] ?? "";
  const body = message === null ? "" : message.split("\n").slice(1).join("\n").trim();

  const copySha = () => {
    void navigator.clipboard
      ?.writeText(commitId)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      })
      .catch(() => {});
  };

  return (
    <section className={styles.header} aria-label="Commit details">
      <h2 className={styles.subject}>{subject}</h2>
      {body !== "" && <pre className={styles.body}>{body}</pre>}
      <dl className={styles.meta}>
        {commit !== undefined && (
          <>
            <dt>Author</dt>
            <dd>
              {commit.authorName} &lt;{commit.authorEmail}&gt;
            </dd>
            <dt>Date</dt>
            <dd>
              <time dateTime={new Date(commit.timestamp * 1000).toISOString()}>
                {formatAbsoluteDate(commit.timestamp)}
              </time>{" "}
              <span className={styles.relative}>({formatRelativeDate(commit.timestamp)})</span>
            </dd>
          </>
        )}
        <dt>SHA</dt>
        <dd>
          <code className={styles.sha}>{commitId}</code>{" "}
          <button type="button" className={styles.linkButton} onClick={copySha} aria-label="Copy full SHA">
            {copied ? "Copied" : "Copy"}
          </button>
        </dd>
        {commit !== undefined && commit.parentIds.length > 0 && (
          <>
            <dt>{commit.parentIds.length === 1 ? "Parent" : "Parents"}</dt>
            <dd>
              {commit.parentIds.map((parentId) => {
                const shortId = parentId.slice(0, 7);
                return knownCommitIds.has(parentId) ? (
                  <button
                    key={parentId}
                    type="button"
                    className={styles.linkButton}
                    aria-label={`Go to parent ${shortId}`}
                    onClick={() => onSelectRow({ commitId: parentId })}
                  >
                    {shortId}
                  </button>
                ) : (
                  <code key={parentId} className={styles.sha}>
                    {shortId}
                  </code>
                );
              })}
            </dd>
          </>
        )}
      </dl>
    </section>
  );
}
