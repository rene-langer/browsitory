import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  OperationLogEntry,
  OperationLogPage,
  OperationOutcome,
  ProgramRun,
  RepoClient,
  TransferErrorKind,
} from "../ipc/RepoClient";
import { copyText } from "../lib/clipboard";
import { Overlay } from "./primitives/Overlay";
import { Panel } from "./primitives/Panel";
import styles from "./OperationLogDialog.module.css";

/** How many entries one page asks for. */
export const OPERATION_LOG_PAGE = 100;

/** The name the backend gives a fetch the "Fetch automatically" setting started. */
const AUTOMATIC_FETCH = "Fetch all (automatic)";

const OUTCOME_ICON: Record<OperationOutcome, string> = {
  Running: "…",
  Succeeded: "✓",
  Stopped: "‖",
  Failed: "✕",
  Cancelled: "⊘",
};

const TRANSFER_ERROR: Record<TransferErrorKind, string> = {
  NonFastForward: "The remote has commits this branch doesn't",
  RejectedRemoteRef: "The remote rejected the reference",
  StaleLease: "The remote branch changed since the last fetch",
  MissingCredential: "No saved credential",
  CredentialStoreFailure: "The credential store failed",
  SshAgentFailure: "The SSH agent failed",
  TransferFailed: "The transfer failed",
  Cancelled: "Cancelled",
  HookFailed: "The pre-push hook failed",
};

/** A cancel is the user's own doing, not a failure. */
function isFailure(entry: OperationLogEntry): boolean {
  return entry.outcome === "Failed";
}

function title(entry: OperationLogEntry): string {
  return entry.target === null ? entry.name : `${entry.name} ${entry.target}`;
}

function formatTime(startedAt: number): string {
  return new Date(startedAt).toLocaleTimeString();
}

function formatDuration(ms: number | null): string {
  if (ms === null) return "running";
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(1)} s`;
}

/** The last part of a program's path: "pre-push" for `/repo/.git/hooks/pre-push`. */
function programName(program: string): string {
  return program.split(/[\\/]/).pop() || program;
}

/** Newer entries first; an entry already listed is replaced by its newer copy. */
function merge(current: OperationLogEntry[], fresh: OperationLogEntry[]): OperationLogEntry[] {
  const byId = new Map(current.map((entry) => [entry.id, entry]));
  for (const entry of fresh) byId.set(entry.id, entry);
  return [...byId.values()].sort((a, b) => b.id - a.id);
}

/**
 * The operation log (#889): what the app ran in this repository, newest first, kept by the
 * backend in memory for this session (#887). Opened from the palette, the Help menu and the error
 * banner's "Show details" (`focus: "latestFailed"` selects the newest failed operation). It
 * follows the log live while open.
 */
export function OperationLogDialog({
  client,
  repoPath,
  focus,
  onClose,
}: {
  client: RepoClient;
  repoPath: string;
  focus: "latest" | "latestFailed";
  /** Run again (#890), stubbed for the tests. */
  runAgain?: { run: (rerun: import("../ipc/RepoClient").Rerun) => void; blockedReason: (rerun: import("../ipc/RepoClient").Rerun) => string | null };
  onClose: () => void;
}) {
  const [entries, setEntries] = useState<OperationLogEntry[] | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [failedOnly, setFailedOnly] = useState(false);
  const [hideAutomatic, setHideAutomatic] = useState(false);
  // The repository key the backend names this log by; change events carry the same key.
  const repoKey = useRef<string | null>(null);
  const focused = useRef(false);

  const showNewest = useCallback(
    (page: OperationLogPage) => {
      repoKey.current = page.repo;
      setEntries((current) => merge(current ?? [], page.entries));
      // A live refresh only adds newer entries; whether older ones follow stays as it was.
      if (!focused.current) {
        focused.current = true;
        setHasMore(page.hasMore);
        const first =
          focus === "latestFailed" ? (page.entries.find(isFailure) ?? page.entries[0]) : page.entries[0];
        setSelectedId(first?.id ?? null);
      }
    },
    [focus],
  );

  useEffect(() => {
    const loadNewest = () =>
      client.listOperations(repoPath, null, OPERATION_LOG_PAGE).then(showNewest, (err: unknown) => {
        setError(`Couldn't read the operation log: ${String(err)}`);
      });
    void loadNewest();
    return client.subscribeOperationLog((change) => {
      if (repoKey.current !== null && change.repo === repoKey.current) void loadNewest();
    });
  }, [client, repoPath, showNewest]);

  const loadOlder = async () => {
    const oldest = entries?.[entries.length - 1];
    if (oldest === undefined) return;
    try {
      const page = await client.listOperations(repoPath, oldest.id, OPERATION_LOG_PAGE);
      setEntries((current) => merge(current ?? [], page.entries));
      setHasMore(page.hasMore);
    } catch (err) {
      setError(`Couldn't read the operation log: ${String(err)}`);
    }
  };

  const shown = useMemo(
    () =>
      (entries ?? []).filter(
        (entry) => (!failedOnly || isFailure(entry)) && (!hideAutomatic || entry.name !== AUTOMATIC_FETCH),
      ),
    [entries, failedOnly, hideAutomatic],
  );
  const selected = shown.find((entry) => entry.id === selectedId) ?? null;

  return (
    <Overlay onClose={onClose} ariaLabel="Operation log">
      <Panel
        title="Operation log"
        footer={
          <button type="button" onClick={onClose}>
            Close
          </button>
        }
        bodyClassName={styles.body}
      >
        <div className={styles.filters}>
          <label>
            <input type="checkbox" checked={failedOnly} onChange={(event) => setFailedOnly(event.target.checked)} />{" "}
            Failed only
          </label>
          <label>
            <input
              type="checkbox"
              checked={hideAutomatic}
              onChange={(event) => setHideAutomatic(event.target.checked)}
            />{" "}
            Hide automatic fetches
          </label>
        </div>
        {error !== null && (
          <p role="alert" className={styles.error}>
            {error}
          </p>
        )}
        {entries === null ? (
          error === null && <p className={styles.muted}>Loading…</p>
        ) : entries.length === 0 ? (
          <p className={styles.muted}>No operations yet.</p>
        ) : (
          <div className={styles.columns}>
            <div className={styles.listColumn}>
              {shown.length === 0 ? (
                <p className={styles.muted}>No operations match the filters.</p>
              ) : (
                <ul className={styles.list} aria-label="Operations">
                  {shown.map((entry) => (
                    <li key={entry.id}>
                      <button
                        type="button"
                        className={`${styles.row} ${isFailure(entry) ? styles.failed : ""}`}
                        aria-current={entry.id === selectedId ? "true" : undefined}
                        aria-label={`${title(entry)}, ${entry.outcome}, ${formatTime(entry.startedAt)}, ${formatDuration(entry.durationMs)}`}
                        onClick={() => setSelectedId(entry.id)}
                      >
                        <span className={styles.icon} aria-hidden="true">
                          {OUTCOME_ICON[entry.outcome]}
                        </span>
                        <span className={styles.rowTitle}>{title(entry)}</span>
                        <span className={styles.rowMeta}>
                          {formatTime(entry.startedAt)} · {formatDuration(entry.durationMs)}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              {hasMore && (
                <button type="button" onClick={() => void loadOlder()}>
                  Load older operations
                </button>
              )}
            </div>
            {selected !== null && <OperationDetails entry={selected} />}
          </div>
        )}
      </Panel>
    </Overlay>
  );
}

function OperationDetails({ entry }: { entry: OperationLogEntry }) {
  return (
    <section className={styles.details} aria-label="Operation details">
      <h3 className={styles.detailsTitle}>{title(entry)}</h3>
      <dl className={styles.facts}>
        <dt>Result</dt>
        <dd>{entry.outcome}</dd>
        <dt>Started</dt>
        <dd>{new Date(entry.startedAt).toLocaleString()}</dd>
        <dt>Duration</dt>
        <dd>{formatDuration(entry.durationMs)}</dd>
        {entry.transferError !== null && (
          <>
            <dt>Transfer</dt>
            <dd>{TRANSFER_ERROR[entry.transferError]}</dd>
          </>
        )}
        {entry.params.map((param) => (
          <ParamRow key={param.name} name={param.name} value={param.value} />
        ))}
      </dl>
      {entry.message !== null && <p className={styles.message}>{entry.message}</p>}
      {entry.remoteFailures.length > 0 && (
        <>
          <h4 className={styles.subTitle}>Remotes that failed</h4>
          <ul className={styles.failures}>
            {entry.remoteFailures.map((failure) => (
              <li key={failure.remoteName}>
                {failure.remoteName}: {failure.message || TRANSFER_ERROR[failure.error]}
              </li>
            ))}
          </ul>
        </>
      )}
      {entry.runs.map((run, index) => (
        <RunOutput key={index} run={run} />
      ))}
    </section>
  );
}

function ParamRow({ name, value }: { name: string; value: string }) {
  return (
    <>
      <dt>{name}</dt>
      <dd>{value}</dd>
    </>
  );
}

function RunOutput({ run }: { run: ProgramRun }) {
  const name = programName(run.program);
  const [copyError, setCopyError] = useState<string | null>(null);
  return (
    <div className={styles.run}>
      <div className={styles.runHeader}>
        <code className={styles.command}>{[run.program, ...run.args].join(" ")}</code>
        <span className={styles.runStatus}>{run.status}</span>
        <button
          type="button"
          aria-label={`Copy output of ${name}`}
          onClick={() =>
            void copyText(run.output).then(
              () => setCopyError(null),
              (err: unknown) => setCopyError(String(err)),
            )
          }
        >
          Copy
        </button>
      </div>
      <pre className={styles.output} aria-label={`Output of ${name}`} tabIndex={0}>
        {run.output === "" ? "(no output)" : run.output}
      </pre>
      {copyError !== null && <p role="alert">{copyError}</p>}
    </div>
  );
}
