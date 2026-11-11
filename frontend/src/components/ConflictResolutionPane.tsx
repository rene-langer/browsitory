import { Fragment, useEffect, useId, useRef, useState } from "react";
import { errorKind, type ConflictSegment, type ConflictSide, type FileConflictChoice, type RepoClient } from "../ipc/RepoClient";
import { conflictSideNames, type ConflictSideNames } from "../lib/conflictSides";
import {
  assembleResult,
  blockResult,
  hasConflictMarkers,
  type BlockChoice,
  type BlockState,
  type ConflictBlock,
} from "../lib/conflictMerge";
import { useConflictTokens } from "../lib/conflictHighlight";
import type { Token } from "../lib/highlight";
import { languageForPath } from "../lib/languageForPath";
import { logFailure } from "../lib/logger";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./ConflictResolutionPane.module.css";

const CHOICES: { choice: BlockChoice; label: string; title: string }[] = [
  { choice: "ours", label: "Accept ours", title: "Take our lines for this block." },
  { choice: "theirs", label: "Accept theirs", title: "Take their lines for this block." },
  { choice: "oursThenTheirs", label: "Ours then theirs", title: "Keep both: our lines first." },
  { choice: "theirsThenOurs", label: "Theirs then ours", title: "Keep both: their lines first." },
  { choice: "base", label: "Accept base", title: "Go back to the common ancestor's lines." },
];

const STATE_LABELS: Record<BlockChoice, string> = {
  ours: "Ours",
  theirs: "Theirs",
  oursThenTheirs: "Ours then theirs",
  theirsThenOurs: "Theirs then ours",
  base: "Base",
};

/** Lines of an unchanged run shown next to a conflict; a longer run is collapsed in between. */
const CONTEXT_LINES = 3;

function sideLabel(side: string, name: string | null): string {
  return name === null ? side : `${side} (${name})`;
}

function stateLabel(state: BlockState): string {
  switch (state.kind) {
    case "unchosen":
      return "Not chosen";
    case "chosen":
      return STATE_LABELS[state.choice];
    case "edited":
      return "Edited";
  }
}

function lineCount(text: string): number {
  return text.split("\n").length;
}

const NO_NAMES: ConflictSideNames = { ours: null, theirs: null };

function logHighlightError(error: unknown): void {
  logFailure("Syntax highlighting failed", error);
}

/** `text` as it is, its lines coloured with `tokens` when given (#763). */
function Code({ text, tokens }: { text: string; tokens: readonly Token[][] | undefined }) {
  if (tokens === undefined) return <>{text}</>;
  return (
    <>
      {text.split(/(?<=\n)/).map((line, i) => {
        const lineTokens = tokens[i];
        if (lineTokens === undefined) return <Fragment key={i}>{line}</Fragment>;
        return (
          <Fragment key={i}>
            {lineTokens.map((token, j) => (
              <span key={j} style={token.color !== undefined ? { color: token.color } : undefined}>
                {token.content}
              </span>
            ))}
            {/\r?\n$/.exec(line)?.[0] ?? ""}
          </Fragment>
        );
      })}
    </>
  );
}

/** Whole-result editing (#720): the assembled file as one text, and that text as it started. */
interface WholeEdit {
  text: string;
  initial: string;
}

export function ConflictResolutionPane({
  repoPath,
  client,
  path,
  onResolve,
  onResolveAddDelete,
  onResolveWholeFile,
  onMarkResolved,
}: {
  repoPath: string;
  client: RepoClient;
  path: string;
  onResolve: (path: string, resolvedContent: string) => void;
  onResolveAddDelete: (path: string, choice: FileConflictChoice) => void;
  /** Takes one side's whole file; works for every conflict shape, binary included. */
  onResolveWholeFile?: (path: string, side: ConflictSide) => void;
  /** Stages the file as it is on disk (the caller asks first if conflict markers remain). */
  onMarkResolved?: () => void;
}) {
  const [segments, setSegments] = useState<ConflictSegment[]>([]);
  // Every block starts unchosen, so Save never takes a side the user didn't pick (#654).
  const [states, setStates] = useState<BlockState[]>([]);
  const [names, setNames] = useState<ConflictSideNames>(NO_NAMES);
  const [showBase, setShowBase] = useState(false);
  // Unchanged runs the user expanded, by segment index.
  const [expanded, setExpanded] = useState<ReadonlySet<number>>(new Set());
  const [wholeEdit, setWholeEdit] = useState<WholeEdit | null>(null);
  const [confirmLeave, setConfirmLeave] = useState(false);
  const blockRefs = useRef<(HTMLElement | null)[]>([]);
  const [currentConflict, setCurrentConflict] = useState(-1);
  const idPrefix = useId();
  const [error, setError] = useState<string | null>(null);
  const [isAddDeleteConflict, setIsAddDeleteConflict] = useState(false);
  // Guards "Save resolution" against a click landing before the fetch below completes — without
  // this, an early click sees `segments = []`, reconstructs an empty string, and silently
  // overwrites the file via `onResolve(path, "")`. Tracked by path rather than a plain boolean
  // so it self-resets on every `path` change (re-arming the guard for that path's own fetch)
  // without a synchronous `setState` call in the effect body, which the lint rule
  // `react-hooks/set-state-in-effect` forbids.
  const [loadedPath, setLoadedPath] = useState<string | null>(null);
  const loaded = loadedPath === path;

  useEffect(() => {
    let ignore = false;
    client
      .getConflictHunks(repoPath, path)
      .then((next) => {
        if (!ignore) {
          setSegments(next);
          setStates(next.map(() => ({ kind: "unchosen" })));
          setExpanded(new Set());
          setWholeEdit(null);
          setConfirmLeave(false);
          setCurrentConflict(-1);
          setError(null);
          setLoadedPath(path);
        }
      })
      .catch((err: unknown) => {
        if (!ignore) {
          const message = String(err);
          setError(message);
          setIsAddDeleteConflict(errorKind(err) === "AddDeleteConflict");
          setLoadedPath(path);
        }
      });
    return () => {
      ignore = true;
    };
  }, [repoPath, client, path]);

  // The branch names are only labels: when they can't be read, the sides stay "Ours"/"Theirs".
  useEffect(() => {
    let ignore = false;
    const load = async () =>
      conflictSideNames(
        await client.getHeadInfo(repoPath),
        await client.listBranches(repoPath),
        await client.getMergeMessage(repoPath),
      );
    load()
      .then((next) => {
        if (!ignore) setNames(next);
      })
      .catch(() => {});
    return () => {
      ignore = true;
    };
  }, [repoPath, client]);

  // The read-only parts are highlighted like the diff viewer (#763); the results stay plain.
  const { settings } = useDiffSettingsContext();
  const tokens = useConflictTokens(segments, languageForPath(path), settings.syntaxHighlight, logHighlightError);

  // "Ours" is stage 2 (HEAD); "theirs" stage 3, the commit being applied. During a rebase that
  // makes "ours" the branch being rebased onto, which is why the titles name what they replace.
  const header =
    onResolveWholeFile === undefined && onMarkResolved === undefined ? null : (
      <Toolbar aria-label="Whole file">
        {onResolveWholeFile !== undefined && (
          <>
            <button
              disabled={!loaded}
              title="Replace the file with the current branch's version (HEAD)."
              onClick={() => onResolveWholeFile(path, "Ours")}
            >
              Use ours (whole file)
            </button>
            <button
              disabled={!loaded}
              title="Replace the file with the incoming version."
              onClick={() => onResolveWholeFile(path, "Theirs")}
            >
              Use theirs (whole file)
            </button>
          </>
        )}
        {onMarkResolved !== undefined && (
          <button disabled={!loaded} onClick={onMarkResolved}>
            Mark resolved
          </button>
        )}
      </Toolbar>
    );

  if (error !== null) {
    if (isAddDeleteConflict) {
      return (
        <Panel title="Resolve conflict">
          {header}
          <p>{error}</p>
          <Toolbar>
            <button disabled={!loaded} onClick={() => onResolveAddDelete(path, "Ours")}>
              Keep our version
            </button>
            <button disabled={!loaded} onClick={() => onResolveAddDelete(path, "Theirs")}>
              Keep their version
            </button>
            <button disabled={!loaded} onClick={() => onResolveAddDelete(path, "Delete")}>
              Delete file
            </button>
          </Toolbar>
        </Panel>
      );
    }
    if (header === null) return <p role="alert">{error}</p>;
    return (
      <Panel title="Resolve conflict">
        {header}
        <p role="alert">{error}</p>
      </Panel>
    );
  }

  const setStateAt = (index: number, state: BlockState) => {
    setStates((prev) => prev.map((s, i) => (i === index ? state : s)));
  };

  const conflictIndices = segments.flatMap((segment, index) => (segment.kind === "Conflict" ? [index] : []));
  const stateAt = (index: number): BlockState => states[index] ?? { kind: "unchosen" };
  const resolvedCount = conflictIndices.filter((index) => stateAt(index).kind !== "unchosen").length;
  const allResolved = resolvedCount === conflictIndices.length;

  const goToConflict = (position: number) => {
    setCurrentConflict(position);
    const block = blockRefs.current[conflictIndices[position]];
    block?.focus();
    block?.scrollIntoView?.({ block: "nearest" });
  };

  const startWholeEdit = () => {
    const text = assembleResult(segments, states);
    setWholeEdit({ text, initial: text });
  };

  const leaveWholeEdit = () => {
    setConfirmLeave(false);
    setWholeEdit(null);
  };

  if (wholeEdit !== null) {
    const markers = hasConflictMarkers(wholeEdit.text);
    return (
      <Panel title="Resolve conflict">
        {header}
        <Toolbar aria-label="Editing the whole result">
          <button
            onClick={() => (wholeEdit.text === wholeEdit.initial ? leaveWholeEdit() : setConfirmLeave(true))}
          >
            Back to blocks
          </button>
        </Toolbar>
        <p className={styles.hint}>
          The whole file as it will be saved. Blocks without a choice are in it with their conflict
          markers; resolve them here.
        </p>
        <textarea
          aria-label="Whole result"
          className={`${styles.segment} ${styles.result}`}
          spellCheck={false}
          rows={Math.min(40, Math.max(10, lineCount(wholeEdit.text)))}
          value={wholeEdit.text}
          onChange={(event) => setWholeEdit({ ...wholeEdit, text: event.target.value })}
        />
        {markers && (
          <p className={styles.hint}>
            Remove the conflict markers (lines starting with &lt;&lt;&lt;&lt;&lt;&lt;&lt; or
            &gt;&gt;&gt;&gt;&gt;&gt;&gt;) to save.
          </p>
        )}
        <button disabled={markers} onClick={() => onResolve(path, wholeEdit.text)}>
          Save resolution
        </button>
        {confirmLeave && (
          <ConfirmDialog
            ariaLabel="Drop the edits"
            message="Going back to the blocks drops what you changed in the whole result."
            confirmLabel="Drop edits"
            cancelLabel="Keep editing"
            onConfirm={leaveWholeEdit}
            onCancel={() => setConfirmLeave(false)}
          />
        )}
      </Panel>
    );
  }

  const save = () => {
    if (!allResolved) return;
    onResolve(path, assembleResult(segments, states));
  };

  const renderClean = (content: string, index: number) => {
    const lines = content.split(/(?<=\n)/);
    const head = index > 0 ? CONTEXT_LINES : 0;
    const tail = index < segments.length - 1 ? CONTEXT_LINES : 0;
    const hidden = lines.length - head - tail;
    const cleanTokens = tokens?.ours[index];
    if (expanded.has(index) || hidden <= 2) {
      return (
        <pre key={index} className={styles.segment}>
          <Code text={content} tokens={cleanTokens} />
        </pre>
      );
    }
    return (
      <div key={index}>
        {head > 0 && (
          <pre className={styles.segment}>
            <Code text={lines.slice(0, head).join("")} tokens={cleanTokens?.slice(0, head)} />
          </pre>
        )}
        <button className={styles.expand} onClick={() => setExpanded((prev) => new Set(prev).add(index))}>
          Show {hidden} unchanged lines
        </button>
        {tail > 0 && (
          <pre className={styles.segment}>
            <Code text={lines.slice(lines.length - tail).join("")} tokens={cleanTokens?.slice(lines.length - tail)} />
          </pre>
        )}
      </div>
    );
  };

  const renderBlock = (segment: ConflictBlock, index: number) => {
    const position = conflictIndices.indexOf(index);
    const state = stateAt(index);
    const title = `Conflict ${position + 1} of ${conflictIndices.length}`;
    const result = blockResult(segment, state);
    const choice = state.kind === "chosen" ? state.choice : null;
    const bothChosen = choice === "oursThenTheirs" || choice === "theirsThenOurs";
    const resultId = `${idPrefix}-result-${index}`;
    return (
      <section
        key={index}
        ref={(el) => {
          blockRefs.current[index] = el;
        }}
        tabIndex={-1}
        aria-label={title}
        className={styles.block}
        onFocus={() => setCurrentConflict(position)}
      >
        <h4 className={styles.blockTitle}>
          {title}
          <span className={styles.blockState}>{stateLabel(state)}</span>
        </h4>
        {showBase && (
          <div className={`${styles.side} ${choice === "base" ? styles.chosen : ""}`}>
            <div className={styles.sideLabel}>Base</div>
            <pre className={`${styles.segment} ${segment.base === "" ? styles.placeholder : ""}`}>
              {segment.base === "" ? "Empty: no common lines" : <Code text={segment.base} tokens={tokens?.base[index]} />}
            </pre>
          </div>
        )}
        <div className={styles.sides}>
          <div className={`${styles.side} ${choice === "ours" || bothChosen ? styles.chosen : ""}`}>
            <div className={styles.sideLabel}>{sideLabel("Ours", names.ours)}</div>
            <pre className={styles.segment}>
              <Code text={segment.ours} tokens={tokens?.ours[index]} />
            </pre>
          </div>
          <div className={`${styles.side} ${choice === "theirs" || bothChosen ? styles.chosen : ""}`}>
            <div className={styles.sideLabel}>{sideLabel("Theirs", names.theirs)}</div>
            <pre className={styles.segment}>
              <Code text={segment.theirs} tokens={tokens?.theirs[index]} />
            </pre>
          </div>
        </div>
        <Toolbar aria-label={`${title}: choice`}>
          {CHOICES.map((option) => (
            <button
              key={option.choice}
              aria-pressed={choice === option.choice}
              title={option.title}
              onClick={() => setStateAt(index, { kind: "chosen", choice: option.choice })}
            >
              {option.label}
            </button>
          ))}
          <button
            disabled={state.kind === "unchosen"}
            title="Clear this block's choice and edits."
            onClick={() => setStateAt(index, { kind: "unchosen" })}
          >
            Reset
          </button>
        </Toolbar>
        <label className={styles.sideLabel} htmlFor={resultId}>
          Result
        </label>
        <textarea
          id={resultId}
          className={`${styles.segment} ${styles.result}`}
          spellCheck={false}
          placeholder="Not chosen yet: take a side above, or type the result."
          rows={Math.min(20, Math.max(2, lineCount(result ?? "")))}
          value={result ?? ""}
          onChange={(event) => setStateAt(index, { kind: "edited", text: event.target.value })}
        />
        {result === "" && <p className={styles.hint}>Empty: the block&apos;s lines are removed.</p>}
      </section>
    );
  };

  return (
    <Panel title="Resolve conflict">
      {header}
      {conflictIndices.length > 0 && (
        <Toolbar aria-label="Conflicts">
          <span role="status">
            {resolvedCount} of {conflictIndices.length} resolved
          </span>
          <button disabled={currentConflict <= 0} onClick={() => goToConflict(currentConflict - 1)}>
            Previous conflict
          </button>
          <button
            disabled={currentConflict >= conflictIndices.length - 1}
            onClick={() => goToConflict(currentConflict + 1)}
          >
            Next conflict
          </button>
          <button aria-pressed={showBase} onClick={() => setShowBase(!showBase)}>
            Show base
          </button>
          <button onClick={startWholeEdit}>Edit whole result</button>
        </Toolbar>
      )}
      {segments.map((segment, index) =>
        segment.kind === "Clean" ? renderClean(segment.content, index) : renderBlock(segment, index),
      )}
      {loaded && !allResolved && (
        <p className={styles.hint}>Choose or type a result for every conflict to save.</p>
      )}
      <button onClick={save} disabled={!loaded || !allResolved}>
        Save resolution
      </button>
    </Panel>
  );
}
