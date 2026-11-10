import { ChevronDown, ChevronRight, File, FileCode, Folder, FolderOpen, History, Link, Package } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { FileContent, RepoClient, TreeEntry, TreeListing } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { HIGHLIGHT_LINE_LIMIT, highlightLines, type Token } from "../lib/highlight";
import { formatSize } from "../lib/fileSize";
import { languageForPath } from "../lib/languageForPath";
import { logFailure } from "../lib/logger";
import type { SelectedRow } from "../state/useAppState";
import type { ExternalToolsApi } from "../state/useExternalTools";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import styles from "./CommitTreeBrowser.module.css";
import diffPaneStyles from "./DiffPane.module.css";
import { FolderHistoryButton } from "./FolderHistoryButton";
import { LineHistoryButton } from "./LineHistoryButton";
import { ReadOnlyFileSection } from "./ReadOnlyFileSection";
import { DiffOptionsToolbar } from "./DiffOptionsToolbar";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { InlineError } from "./primitives/InlineError";

/** How long the filter waits after the last keystroke before it searches the tree. */
export const FILTER_DEBOUNCE_MS = 200;
/** A text file shows at most this many lines; the rest are left out with a notice. */
export const MAX_SHOWN_LINES = 20000;

const short = (id: string) => id.slice(0, 7);

type Folder = { listing: TreeListing } | { error: DescribedError };
type Matches = { query: string; result: Folder };

export interface CommitTreeBrowserProps {
  repoPath: string;
  client: RepoClient;
  commitId: string;
  onSelectRow: (row: SelectedRow) => void;
  onOpenFileHistory?: (path: string) => void;
  /** "History of lines" (#447) for lines selected in a file's content. */
  onOpenLineHistory?: (path: string, revision: string, startLine: number, endLine: number) => void;
  /** "Folder history" (#447) on folder rows and in a file's header. */
  onOpenFolderHistory?: (folder: string) => void;
  externalTools?: ExternalToolsApi;
  /** Restores a file version to the work tree; omitted, there is no Restore action. */
  onRestoreFileVersion?: (commitId: string, path: string) => void | Promise<void>;
  /** Why Restore can't run now (`operationBlockers`), else `null`. */
  restoreBlockedReason?: string | null;
}

/**
 * A commit's whole file tree (#446): folders load when first opened, and the filter box searches
 * every path in the tree. Selecting a file shows its content at that commit, read-only, with its
 * actions (`CommitFileView`).
 */
export function CommitTreeBrowser({
  repoPath,
  client,
  commitId,
  onSelectRow,
  onOpenFileHistory,
  onOpenLineHistory,
  onOpenFolderHistory,
  externalTools,
  onRestoreFileVersion,
  restoreBlockedReason = null,
}: CommitTreeBrowserProps) {
  // Loaded folders by path ("" is the root); a requested folder not in the map is still loading.
  const [folders, setFolders] = useState<ReadonlyMap<string, Folder>>(new Map());
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  // Every folder opened so far (and the root); they stay loaded when closed.
  const [requested, setRequested] = useState<ReadonlySet<string>>(new Set([""]));
  // Bumped by Retry, which first drops the failed folder from `folders`.
  const [retryToken, setRetryToken] = useState(0);
  const [selected, setSelected] = useState<TreeEntry | null>(null);
  const [filter, setFilter] = useState("");
  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState<Matches | null>(null);

  useEffect(() => {
    const pending = [...requested].filter((dir) => !folders.has(dir));
    if (pending.length === 0) return;
    let ignore = false;
    for (const dir of pending) {
      client.getCommitTree(repoPath, commitId, dir, null).then(
        (listing) => {
          if (!ignore) setFolders((prev) => new Map(prev).set(dir, { listing }));
        },
        (err: unknown) => {
          if (!ignore) setFolders((prev) => new Map(prev).set(dir, { error: describeError(err) }));
        },
      );
    }
    return () => {
      ignore = true;
    };
    // `folders` only filters what to fetch: a fetch landing must not start another.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, repoPath, commitId, requested, retryToken]);

  useEffect(() => {
    const timer = setTimeout(() => setQuery(filter.trim()), FILTER_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [filter]);

  useEffect(() => {
    if (query === "") return;
    let ignore = false;
    client.getCommitTree(repoPath, commitId, "", query).then(
      (listing) => {
        if (!ignore) setMatches({ query, result: { listing } });
      },
      (err: unknown) => {
        if (!ignore) setMatches({ query, result: { error: describeError(err) } });
      },
    );
    return () => {
      ignore = true;
    };
  }, [client, repoPath, commitId, query, retryToken]);

  const toggleFolder = (dir: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(dir)) next.delete(dir);
      else next.add(dir);
      return next;
    });
    setRequested((prev) => (prev.has(dir) ? prev : new Set(prev).add(dir)));
  };

  const retry = (dir: string) => {
    setFolders((prev) => {
      const next = new Map(prev);
      next.delete(dir);
      return next;
    });
    setRetryToken((n) => n + 1);
  };

  const entryRow = (entry: TreeEntry, level: number, showPath: boolean): ReactNode => {
    if (entry.kind === "Folder") {
      const open = expanded.has(entry.path);
      return (
        <li key={entry.path} role="treeitem" aria-expanded={open} aria-level={level} aria-selected={false}>
          <button
            type="button"
            className={styles.row}
            style={{ paddingInlineStart: `calc(${level - 1} * var(--space-3) + var(--space-1))` }}
            onClick={() => toggleFolder(entry.path)}
          >
            {open ? <ChevronDown size={14} aria-hidden="true" /> : <ChevronRight size={14} aria-hidden="true" />}
            {open ? <FolderOpen size={14} aria-hidden="true" /> : <Folder size={14} aria-hidden="true" />}
            <span className={styles.name}>{entry.name}</span>
          </button>
          {onOpenFolderHistory !== undefined && (
            <button
              type="button"
              className={styles.folderHistory}
              aria-label={`History of folder ${entry.path}`}
              title={`Folder history: commits that changed anything in ${entry.path}/`}
              onClick={() => onOpenFolderHistory(entry.path)}
            >
              <History size={14} aria-hidden="true" />
            </button>
          )}
          {open && folderBody(entry.path, level + 1)}
        </li>
      );
    }
    const isSelected = selected?.path === entry.path;
    const Icon = entry.kind === "Symlink" ? Link : entry.kind === "Submodule" ? Package : entry.kind === "Executable" ? FileCode : File;
    return (
      <li key={entry.path} role="treeitem" aria-level={level} aria-selected={isSelected}>
        <button
          type="button"
          className={styles.row}
          aria-current={isSelected ? "true" : undefined}
          style={{ paddingInlineStart: `calc(${level - 1} * var(--space-3) + var(--space-1) + 14px)` }}
          onClick={() => setSelected(entry)}
        >
          <Icon size={14} aria-hidden="true" />
          <span className={styles.name}>{showPath ? entry.path : entry.name}</span>
          {entry.size !== null && <span className={styles.size}>{formatSize(entry.size)}</span>}
        </button>
      </li>
    );
  };

  /** A folder's rows, or the message shown instead (loading, an error, empty). */
  const folderContent = (dir: string, level: number): { rows: ReactNode[] } | { message: ReactNode } => {
    const folder = folders.get(dir);
    if (folder === undefined) {
      return {
        message: (
          <p role="status" className={styles.note}>
            Loading…
          </p>
        ),
      };
    }
    if ("error" in folder) {
      return {
        message: (
          <InlineError
            message={folder.error.message}
            hint={folder.error.hint}
            onDismiss={() => retry(dir)}
            onRetry={() => retry(dir)}
          />
        ),
      };
    }
    if (folder.listing.entries.length === 0) {
      return { message: <p className={styles.note}>Empty folder</p> };
    }
    return { rows: folder.listing.entries.map((entry) => entryRow(entry, level, false)) };
  };

  const folderBody = (dir: string, level: number): ReactNode => {
    const content = folderContent(dir, level);
    return (
      <ul role="group">
        {"rows" in content ? content.rows : <li role="none">{content.message}</li>}
      </ul>
    );
  };

  const searching = query !== "";
  const shownMatches = matches !== null && matches.query === query ? matches.result : undefined;

  let treeContent: { rows: ReactNode[] } | { message: ReactNode };
  if (!searching) {
    treeContent = folderContent("", 1);
  } else if (shownMatches === undefined) {
    treeContent = {
      message: (
        <p role="status" className={styles.note}>
          Searching…
        </p>
      ),
    };
  } else if ("error" in shownMatches) {
    treeContent = {
      message: (
        <InlineError
          message={shownMatches.error.message}
          hint={shownMatches.error.hint}
          onDismiss={() => setFilter("")}
          onRetry={() => setRetryToken((n) => n + 1)}
        />
      ),
    };
  } else if (shownMatches.listing.entries.length === 0) {
    treeContent = {
      message: (
        <p role="status" className={styles.note}>
          No files match “{query}”.
        </p>
      ),
    };
  } else {
    treeContent = { rows: shownMatches.listing.entries.map((entry) => entryRow(entry, 1, true)) };
  }

  return (
    <div className={styles.browser}>
      <input
        type="search"
        className={styles.filter}
        placeholder="Filter files"
        aria-label="Filter files"
        value={filter}
        onChange={(event) => setFilter(event.target.value)}
      />
      {searching && shownMatches !== undefined && "listing" in shownMatches && shownMatches.listing.truncated && (
        <p role="status" className={styles.note}>
          Showing the first {shownMatches.listing.entries.length} matches. Type more to narrow them down.
        </p>
      )}
      <div className={styles.tree}>
        {"rows" in treeContent ? (
          <ul role="tree" aria-label={`Files at ${short(commitId)}`}>
            {treeContent.rows}
          </ul>
        ) : (
          treeContent.message
        )}
      </div>
      {selected === null ? (
        <p className={styles.note}>Select a file to see it as it is in this commit.</p>
      ) : (
        <CommitFileView
          key={selected.path}
          repoPath={repoPath}
          client={client}
          commitId={commitId}
          entry={selected}
          onSelectRow={onSelectRow}
          onOpenFileHistory={onOpenFileHistory}
          onOpenLineHistory={onOpenLineHistory}
          onOpenFolderHistory={onOpenFolderHistory}
          externalTools={externalTools}
          onRestoreFileVersion={onRestoreFileVersion}
          restoreBlockedReason={restoreBlockedReason}
        />
      )}
    </div>
  );
}

/**
 * One file as it is in a commit, read-only, with its actions: History, Compare with working
 * tree (its diff against the work tree file), Save as… and Restore… (writes it to the work tree,
 * after a confirmation; Undo brings the work tree file back).
 */
export function CommitFileView({
  repoPath,
  client,
  commitId,
  entry,
  onSelectRow,
  onOpenFileHistory,
  onOpenLineHistory,
  onOpenFolderHistory,
  externalTools,
  onRestoreFileVersion,
  restoreBlockedReason = null,
}: {
  repoPath: string;
  client: RepoClient;
  commitId: string;
  entry: TreeEntry;
  onSelectRow: (row: SelectedRow) => void;
  onOpenFileHistory?: (path: string) => void;
  onOpenLineHistory?: (path: string, revision: string, startLine: number, endLine: number) => void;
  onOpenFolderHistory?: (folder: string) => void;
  externalTools?: ExternalToolsApi;
  onRestoreFileVersion?: (commitId: string, path: string) => void | Promise<void>;
  restoreBlockedReason?: string | null;
}) {
  const { path } = entry;
  const contentRef = useRef<HTMLDivElement>(null);
  const submodule = entry.kind === "Submodule";
  const [content, setContent] = useState<FileContent | null>(null);
  const [error, setError] = useState<DescribedError | null>(null);
  const [comparing, setComparing] = useState(false);
  const [saveNote, setSaveNote] = useState<string | null>(null);
  const [confirmRestore, setConfirmRestore] = useState(false);
  const [loadToken, setLoadToken] = useState(0);

  useEffect(() => {
    if (submodule) return;
    let ignore = false;
    client.getFileAtCommit(repoPath, commitId, path).then(
      (next) => {
        if (!ignore) {
          setContent(next);
          setError(null);
        }
      },
      (err: unknown) => {
        if (!ignore) setError(describeError(err));
      },
    );
    return () => {
      ignore = true;
    };
  }, [client, repoPath, commitId, path, submodule, loadToken]);

  const saveAs = () => {
    setSaveNote(null);
    client.saveFileVersion(repoPath, commitId, path).then(
      (saved) => {
        if (saved !== null) setSaveNote(`Saved to ${saved}`);
      },
      (err: unknown) => setError(describeError(err)),
    );
  };

  let body: ReactNode;
  if (submodule) {
    body = <p className={styles.note}>A submodule: its files are in another repository.</p>;
  } else if (comparing) {
    body = (
      <>
      <DiffOptionsToolbar />
      <ul className={diffPaneStyles.fileList}>
        <ReadOnlyFileSection
          repoPath={repoPath}
          client={client}
          path={path}
          diffKey={`working-tree:${commitId}`}
          fetchDiff={(opts) => client.getWorkingTreeCompareDiff(repoPath, commitId, path, null, opts)}
          blameAt={commitId}
          collapsed={false}
          onToggleCollapse={() => setComparing(false)}
          onSelectRow={onSelectRow}
          externalTools={externalTools}
          toolSource={{ kind: "Range", fromId: commitId, toId: null, oldPath: null }}
          previewSource={{ kind: "Range", fromId: commitId, toId: null, oldPath: null }}
        />
      </ul>
      </>
    );
  } else if (error !== null) {
    body = (
      <InlineError
        message={error.message}
        hint={error.hint}
        onDismiss={() => setError(null)}
        onRetry={() => {
          setError(null);
          setLoadToken((n) => n + 1);
        }}
      />
    );
  } else if (content === null) {
    body = (
      <p role="status" className={styles.note}>
        Loading…
      </p>
    );
  } else if (content.kind === "Text") {
    body = <FileText path={path} text={content.text} />;
  } else if (content.kind === "Image") {
    body = (
      <div className={styles.image}>
        <img src={`data:${content.mime};base64,${content.base64}`} alt={`${path} at ${short(commitId)}`} />
      </div>
    );
  } else if (content.kind === "Binary") {
    body = <p className={styles.note}>Binary file ({formatSize(content.size)}), not shown. Use Save as… to open it elsewhere.</p>;
  } else {
    body = <p className={styles.note}>This file is too large to show here ({formatSize(content.size)}). Use Save as… to open it elsewhere.</p>;
  }

  return (
    <section className={styles.viewer} aria-label={`${path} at ${short(commitId)}`}>
      <div className={styles.viewerHeader}>
        <span className={styles.viewerPath}>{path}</span>
        <span className={styles.viewerActions}>
          {onOpenFileHistory !== undefined && (
            <button type="button" onClick={() => onOpenFileHistory(path)}>
              History
            </button>
          )}
          {onOpenLineHistory !== undefined && content?.kind === "Text" && !comparing && (
            <LineHistoryButton container={contentRef} path={path} revision={commitId} onOpen={onOpenLineHistory} />
          )}
          {onOpenFolderHistory !== undefined && <FolderHistoryButton path={path} onOpen={onOpenFolderHistory} />}
          <button type="button" aria-pressed={comparing} disabled={submodule} onClick={() => setComparing((on) => !on)}>
            Compare with working tree
          </button>
          <button type="button" disabled={submodule} onClick={saveAs}>
            Save as…
          </button>
          {onRestoreFileVersion !== undefined && (
            <button
              type="button"
              disabled={submodule || restoreBlockedReason !== null}
              title={restoreBlockedReason ?? undefined}
              onClick={() => setConfirmRestore(true)}
            >
              Restore…
            </button>
          )}
        </span>
      </div>
      {saveNote !== null && (
        <p role="status" className={styles.note}>
          {saveNote}
        </p>
      )}
      <div ref={contentRef}>{body}</div>
      {confirmRestore && onRestoreFileVersion !== undefined && (
        <ConfirmDialog
          ariaLabel="Confirm restore"
          message={
            <>
              <p>
                Replace {path} in the working tree with its version from {short(commitId)}? Undo brings back
                the current file.
              </p>
              {/* #514: the repository can become busy while the confirmation is open. */}
              {restoreBlockedReason !== null && <p>{restoreBlockedReason}</p>}
            </>
          }
          confirmLabel="Restore"
          confirmDisabled={restoreBlockedReason !== null}
          confirmTitle={restoreBlockedReason ?? undefined}
          onConfirm={() => {
            if (restoreBlockedReason !== null) return;
            setConfirmRestore(false);
            void onRestoreFileVersion(commitId, path);
          }}
          onCancel={() => setConfirmRestore(false)}
        />
      )}
    </section>
  );
}

function logHighlightError(error: unknown): void {
  logFailure("Syntax highlighting failed", error);
}

/** A text file's lines with line numbers, syntax highlighted when the setting is on. */
function FileText({ path, text }: { path: string; text: string }) {
  const { settings } = useDiffSettingsContext();
  const allLines = text.endsWith("\n") ? text.slice(0, -1).split("\n") : text.split("\n");
  const lines = allLines.length > MAX_SHOWN_LINES ? allLines.slice(0, MAX_SHOWN_LINES) : allLines;
  const lang = languageForPath(path);
  const highlight = settings.syntaxHighlight && lang !== null && allLines.length <= HIGHLIGHT_LINE_LIMIT;
  const [tokens, setTokens] = useState<{ text: string; lines: Token[][] } | null>(null);

  useEffect(() => {
    if (!highlight || lang === null) return;
    let cancelled = false;
    highlightLines(lang, text.replace(/\r\n/g, "\n")).then(
      (next) => {
        if (!cancelled) setTokens({ text, lines: next });
      },
      (err: unknown) => {
        if (!cancelled) logHighlightError(err);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [highlight, lang, text]);

  const shownTokens = highlight && tokens !== null && tokens.text === text ? tokens.lines : null;
  if (text === "") return <p className={styles.note}>Empty file</p>;
  return (
    <>
      <div className={styles.code} role="region" aria-label={`Content of ${path}`} tabIndex={0}>
        {lines.map((line, i) => (
          <div key={i} className={styles.line} data-line-number={i + 1}>
            <span className={styles.lineNumber} aria-hidden="true">
              {i + 1}
            </span>
            <code className={styles.lineText}>
              {shownTokens?.[i] !== undefined
                ? shownTokens[i].map((token, j) => (
                    <span key={j} style={token.color !== undefined ? { color: token.color } : undefined}>
                      {token.content}
                    </span>
                  ))
                : line.replace(/\r$/, "")}
            </code>
          </div>
        ))}
      </div>
      {allLines.length > MAX_SHOWN_LINES && (
        <p className={styles.note}>
          Showing the first {MAX_SHOWN_LINES} of {allLines.length} lines. Use Save as… to see the whole file.
        </p>
      )}
    </>
  );
}
