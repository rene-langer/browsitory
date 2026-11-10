import { Fragment, useEffect, useState, type KeyboardEvent } from "react";
import type { CommitSignature, CommitStats, ForgeRepository, GraphCommit, RepoClient } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { copyText } from "../lib/clipboard";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { formatAbsoluteDate, formatRelativeDate } from "../lib/formatDate";
import { filesChangedLabel } from "../lib/lineStats";
import { parseMessageLinks } from "../lib/messageLinks";
import { shortKey, signatureLabel } from "../lib/signatureStatus";
import styles from "./CommitHeader.module.css";
import { LineStatsBadge } from "./LineStatsBadge";
import { InlineError } from "./primitives/InlineError";

/**
 * A message with its URLs and issue references as links (#612). The links are spans, not
 * anchors, so the webview never navigates and a copy is plain text; a click or Enter opens one
 * through `onOpen`.
 */
function MessageText({
  text,
  issueForge,
  onOpen,
}: {
  text: string;
  issueForge: ForgeRepository | null;
  onOpen: (url: string) => void;
}) {
  return parseMessageLinks(text, issueForge).map((segment, index) =>
    segment.kind === "text" ? (
      <Fragment key={index}>{segment.text}</Fragment>
    ) : (
      <span
        key={index}
        role="link"
        tabIndex={0}
        className={styles.messageLink}
        title={segment.url}
        onClick={() => onOpen(segment.url)}
        onKeyDown={(event: KeyboardEvent) => {
          if (event.key !== "Enter") return;
          event.preventDefault();
          onOpen(segment.url);
        }}
      >
        {segment.text}
      </span>
    ),
  );
}

/**
 * Metadata block at the top of a selected commit's diff pane: subject, body, author, date, full
 * SHA (copyable), parents and children (#618). Author/date/parents come from the already-loaded
 * `GraphCommit` and the children from the loaded history; only the full message body and the
 * signature status need a `RepoClient` round trip. URLs and, with `issueForge`, issue
 * references in the subject and body are links (#612). `lineStats` adds the commit's totals
 * (#620).
 */
export function CommitHeader({
  repoPath,
  client,
  commitId,
  commit,
  knownCommitIds,
  childIds = [],
  onSelectRow,
  issueForge = null,
  lineStats,
}: {
  repoPath: string;
  client: RepoClient;
  commitId: string;
  // Undefined when the commit is outside the loaded history window.
  commit: GraphCommit | undefined;
  // Ids present in the loaded history; a parent outside it cannot be selected.
  knownCommitIds: ReadonlySet<string>;
  // The loaded commits with this one as a parent (#618).
  childIds?: readonly string[];
  onSelectRow: (row: SelectedRow) => void;
  // The forge repository `#123` refers to (`issueForgeRepository`); `null` leaves it text.
  issueForge?: ForgeRepository | null;
  // "4 files changed, +120 −37" (#620); `undefined` while counting, `null` for a commit over the
  // backend's cap: no row then.
  lineStats?: CommitStats | null;
}) {
  const [message, setMessage] = useState<string | null>(null);
  // undefined = still checking, null = unsigned or the check failed (row hidden).
  const [signature, setSignature] = useState<CommitSignature | null | undefined>(undefined);
  const [copied, setCopied] = useState(false);
  const [openError, setOpenError] = useState<DescribedError | null>(null);

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
    Promise.resolve()
      .then(() => {
        // Reset inside the chain (not synchronously in the effect) so a commit switch shows "checking".
        if (!ignore) setSignature(undefined);
        return client.getCommitSignature(repoPath, commitId);
      })
      .then((next) => {
        if (!ignore) setSignature(next.status === "unsigned" ? null : next);
      })
      .catch(() => {
        // Like the body: the header is complete without it.
        if (!ignore) setSignature(null);
      });
    return () => {
      ignore = true;
    };
  }, [client, repoPath, commitId]);

  const subject = commit?.summary ?? message?.split("\n")[0] ?? "";
  const body = message === null ? "" : message.split("\n").slice(1).join("\n").trim();

  const label =
    signature != null && signature.status !== "unsigned" ? signatureLabel(signature.status) : null;

  const openLink = (url: string) => {
    setOpenError(null);
    Promise.resolve()
      .then(() => client.openExternalUrl(url))
      .catch((error: unknown) => {
        const described = describeError(error);
        setOpenError({ ...described, message: `Couldn't open the link: ${described.message}` });
      });
  };

  const copySha = () => {
    setOpenError(null);
    copyText(commitId)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      })
      .catch((error: unknown) => {
        const described = describeError(error);
        setOpenError({ ...described, message: `Couldn't copy the SHA: ${described.message}` });
      });
  };

  return (
    <section className={styles.header} aria-label="Commit details">
      <h2 className={styles.subject}>
        <MessageText text={subject} issueForge={issueForge} onOpen={openLink} />
      </h2>
      {body !== "" && (
        <pre className={styles.body}>
          <MessageText text={body} issueForge={issueForge} onOpen={openLink} />
        </pre>
      )}
      {openError !== null && (
        <InlineError message={openError.message} hint={openError.hint} onDismiss={() => setOpenError(null)} />
      )}
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
        {signature === undefined && (
          <>
            <dt>Signature</dt>
            <dd className={styles.relative}>Checking signature…</dd>
          </>
        )}
        {signature != null && label !== null && (
          <>
            <dt>Signature</dt>
            <dd>
              <span className={styles.signature} data-tone={label.tone} title={signature.detail ?? undefined}>
                <span aria-hidden="true" className={styles.signatureIcon} />
                {label.text}
              </span>
              {signature.signer !== null && <span>{signature.signer}</span>}
              {signature.key !== null && (
                <code className={styles.sha} title={signature.key}>
                  {shortKey(signature.key)}
                </code>
              )}
            </dd>
          </>
        )}
        {lineStats != null && (
          <>
            <dt>Size</dt>
            <dd>
              {filesChangedLabel(lineStats.filesChanged)},{" "}
              <LineStatsBadge insertions={lineStats.insertions} deletions={lineStats.deletions} />
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
        {childIds.length > 0 && (
          <>
            <dt>{childIds.length === 1 ? "Child" : "Children"}</dt>
            <dd>
              {childIds.map((childId) => {
                const shortId = childId.slice(0, 7);
                return (
                  <button
                    key={childId}
                    type="button"
                    className={styles.linkButton}
                    aria-label={`Go to child ${shortId}`}
                    onClick={() => onSelectRow({ commitId: childId })}
                  >
                    {shortId}
                  </button>
                );
              })}
            </dd>
          </>
        )}
      </dl>
    </section>
  );
}
