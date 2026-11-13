import {
  Fragment,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type FocusEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { errorKind, type ConflictSegment, type ConflictSide, type FileConflictChoice, type RepoClient } from "../ipc/RepoClient";
import { conflictSideNames, type ConflictSideNames } from "../lib/conflictSides";
import {
  assembleResult,
  blockResolved,
  blockResult,
  hasConflictMarkers,
  takeSide,
  type BlockChoice,
  type BlockState,
  type ConflictBlock,
} from "../lib/conflictMerge";
import { useConflictTokens } from "../lib/conflictHighlight";
import { blockWordMarks } from "../lib/conflictWordDiff";
import type { Token } from "../lib/highlight";
import {
  ariaKeyShortcuts,
  chordsFor,
  formatChords,
  matchMergeEditorShortcut,
  matchShortcut,
  mergeEditorAriaKeyShortcuts,
  mergeEditorShortcutLabel,
  withShortcutLabel,
  type MergeEditorShortcutId,
} from "../lib/keybindings";
import { useKeymap } from "../lib/keymap";
import { languageForPath } from "../lib/languageForPath";
import { logFailure } from "../lib/logger";
import { historyOf, record, redo, undo, type History } from "../lib/mergeHistory";
import type { Segment } from "../lib/wordDiff";
import { useRegisterMergeEditor } from "../state/useCurrentMergeEditor";
import { useMergeEditorView } from "../state/useMergeEditorView";
import { useNarrowLayout } from "../state/useNarrowLayout";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { MarkedLine } from "./MarkedLine";
import { MergePanes } from "./MergePanes";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./ConflictResolutionPane.module.css";

// Each choice's key (#764) is its shortcut's: 1 … 5 while the block has the focus.
const CHOICES: { choice: BlockChoice; label: string; title: string; shortcut: MergeEditorShortcutId }[] = [
  { choice: "ours", label: "Accept ours", title: "Take our lines for this block.", shortcut: "conflict-ours" },
  { choice: "theirs", label: "Accept theirs", title: "Take their lines for this block.", shortcut: "conflict-theirs" },
  {
    choice: "oursThenTheirs",
    label: "Ours then theirs",
    title: "Keep both: our lines first.",
    shortcut: "conflict-ours-then-theirs",
  },
  {
    choice: "theirsThenOurs",
    label: "Theirs then ours",
    title: "Keep both: their lines first.",
    shortcut: "conflict-theirs-then-ours",
  },
  { choice: "base", label: "Accept base", title: "Go back to the common ancestor's lines.", shortcut: "conflict-base" },
];

/** A text field types the digits itself; the choices' keys leave it alone (#764). */
function inTextField(target: EventTarget): boolean {
  return target instanceof HTMLElement && target.closest("textarea, input, select, [contenteditable='true']") !== null;
}

const STATE_LABELS: Record<BlockChoice, string> = {
  ours: "Ours",
  theirs: "Theirs",
  oursThenTheirs: "Ours then theirs",
  theirsThenOurs: "Theirs then ours",
  base: "Base",
};

/** Lines of an unchanged run shown next to a conflict; a longer run is collapsed in between. */
const CONTEXT_LINES = 3;

/** Below this width the three panes (#875) give way to the block view (a VS Code side panel). */
const MERGE_PANES_MIN_WIDTH = 680;

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

/**
 * `text` as it is, its lines coloured with `tokens` when given (#763) and the words that differ
 * from the other side in `<mark>` (#879).
 */
function Code({
  text,
  tokens,
  marks,
}: {
  text: string;
  tokens: readonly Token[][] | undefined;
  marks?: readonly (Segment[] | undefined)[];
}) {
  if (tokens === undefined && marks === undefined) return <>{text}</>;
  return (
    <>
      {text.split(/(?<=\n)/).map((line, i) => {
        const lineTokens = tokens?.[i];
        const lineMarks = marks?.[i];
        if (lineTokens === undefined && lineMarks === undefined) return <Fragment key={i}>{line}</Fragment>;
        const ending = /\r?\n$/.exec(line)?.[0] ?? "";
        return (
          <Fragment key={i}>
            <MarkedLine text={line.slice(0, line.length - ending.length)} tokens={lineTokens} marks={lineMarks} />
            {ending}
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
  // Its keys and their tooltips follow the user's keymap (#898).
  useKeymap();
  const [segments, setSegments] = useState<ConflictSegment[]>([]);
  // Every block starts unchosen, so Save never takes a side the user didn't pick (#654). An
  // unchanged run typed over (#878) is `edited` too. Every change is a step in the undo history.
  const [history, setHistory] = useState<History<BlockState[]>>(() => historyOf([]));
  const states = history.present;
  const [names, setNames] = useState<ConflictSideNames>(NO_NAMES);
  const [showBase, setShowBase] = useState(false);
  // Unchanged runs the user expanded, by segment index.
  const [expanded, setExpanded] = useState<ReadonlySet<number>>(new Set());
  const [wholeEdit, setWholeEdit] = useState<WholeEdit | null>(null);
  const [confirmLeave, setConfirmLeave] = useState(false);
  const blockRefs = useRef<(HTMLElement | null)[]>([]);
  const [currentConflict, setCurrentConflict] = useState(-1);
  // The unchanged run (by segment index) whose result had the focus last, after the current
  // conflict: F7 and Shift+F7 go on from there (#879). Null once a block has the focus.
  const [runCursor, setRunCursor] = useState<number | null>(null);
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
          setHistory(historyOf(next.map((): BlockState => ({ kind: "unchosen" }))));
          setExpanded(new Set());
          setWholeEdit(null);
          setConfirmLeave(false);
          setCurrentConflict(-1);
          setRunCursor(null);
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

  const setStateAt = (index: number, state: BlockState) => {
    setHistory((prev) => record(prev, prev.present.map((s, i) => (i === index ? state : s)), null, Date.now()));
  };
  // Text typed into a segment's result; keystrokes in one field make one undo step (#878).
  // Stable, so the panes' unchanged rows don't re-render while typing.
  const editAt = useCallback((index: number, text: string) => {
    const now = Date.now();
    setHistory((prev) =>
      record(
        prev,
        prev.present.map((s, i): BlockState => (i === index ? { kind: "edited", text } : s)),
        `edit-${index}`,
        now,
      ),
    );
  }, []);
  const undoEdit = () => setHistory(undo);
  const redoEdit = () => setHistory(redo);

  const conflictIndices = segments.flatMap((segment, index) => (segment.kind === "Conflict" ? [index] : []));

  // From an unchanged run, the conflicts after and before it; else the current one's neighbours.
  let previousPosition = currentConflict - 1;
  let nextPosition = currentConflict + 1;
  if (runCursor !== null) {
    nextPosition = conflictIndices.findIndex((index) => index > runCursor);
    if (nextPosition < 0) nextPosition = conflictIndices.length;
    previousPosition = nextPosition - 1;
  }
  const canGoPrevious = previousPosition >= 0;
  const canGoNext = nextPosition < conflictIndices.length;

  const goToConflict = (position: number) => {
    setCurrentConflict(position);
    setRunCursor(null);
    const block = blockRefs.current[conflictIndices[position]];
    block?.focus();
    block?.scrollIntoView?.({ block: "nearest" });
  };
  const goToPrevious = () => {
    if (canGoPrevious) goToConflict(previousPosition);
  };
  const goToNext = () => {
    if (canGoNext) goToConflict(nextPosition);
  };

  // The palette's merge editor commands (#764) act on the current conflict.
  const blocksShown = loaded && error === null && wholeEdit === null && conflictIndices.length > 0;
  useRegisterMergeEditor(
    blocksShown
      ? { path, canGoPrevious, canGoNext }
      : null,
    {
      previous: goToPrevious,
      next: goToNext,
      choose: (choice) => {
        if (conflictIndices.length === 0) return;
        const position = currentConflict >= 0 ? currentConflict : 0;
        setStateAt(conflictIndices[position], { kind: "chosen", choice });
        goToConflict(position);
      },
    },
  );

  // The read-only parts are highlighted like the diff viewer (#763); the results stay plain.
  const { settings } = useDiffSettingsContext();
  const tokens = useConflictTokens(segments, languageForPath(path), settings.syntaxHighlight, logHighlightError);
  // The words that differ between each block's sides (#879), worked out once per file.
  const wordMarks = useMemo(
    () => segments.map((segment) => (segment.kind === "Conflict" ? blockWordMarks(segment.ours, segment.theirs) : null)),
    [segments],
  );

  // The whole file in panes (#875) unless the user chose the blocks or the pane is too narrow.
  const [view, setView] = useMergeEditorView();
  const [editorElement, setEditorElement] = useState<HTMLDivElement | null>(null);
  const narrow = useNarrowLayout(editorElement, MERGE_PANES_MIN_WIDTH);
  const panes = view === "panes" && !narrow;

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

  const stateAt = (index: number): BlockState => states[index] ?? { kind: "unchosen" };
  const resolvedCount = conflictIndices.filter((index) => {
    const segment = segments[index];
    return segment.kind === "Conflict" && blockResolved(segment, stateAt(index));
  }).length;
  const unchosenLeft = conflictIndices.some((index) => stateAt(index).kind === "unchosen");
  // A conflict's start or end marker left anywhere, typed or not, keeps Save disabled (#878).
  const markersLeft = !unchosenLeft && hasConflictMarkers(assembleResult(segments, states));
  const canSave = loaded && !unchosenLeft && !markersLeft;

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
    if (!canSave) return;
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

  const blockTitle = (index: number) => `Conflict ${conflictIndices.indexOf(index) + 1} of ${conflictIndices.length}`;

  /** A block's focusable section (#764): its title, then `children`; it takes the choices' keys. */
  const blockSection = (index: number, className: string, children: ReactNode) => {
    const position = conflictIndices.indexOf(index);
    const title = blockTitle(index);
    return (
      <section
        key={index}
        ref={(el) => {
          blockRefs.current[index] = el;
        }}
        tabIndex={-1}
        aria-label={title}
        className={className}
        onFocus={() => {
          setCurrentConflict(position);
          setRunCursor(null);
        }}
        onKeyDown={(event) => {
          if (inTextField(event.target)) {
            // Esc leaves the result field for its block, where the choices' keys work (#764).
            if (event.key === "Escape" && event.target instanceof HTMLTextAreaElement) {
              event.preventDefault();
              event.stopPropagation();
              event.currentTarget.focus();
            }
            return;
          }
          const option = CHOICES.find((candidate) => candidate.shortcut === matchMergeEditorShortcut(event)?.id);
          if (option === undefined) return;
          event.preventDefault();
          setStateAt(index, { kind: "chosen", choice: option.choice });
        }}
      >
        <h4 className={styles.blockTitle}>
          {title}
          <span className={styles.blockState}>{stateLabel(stateAt(index))}</span>
        </h4>
        {children}
      </section>
    );
  };

  /** A block's choices and Reset. */
  const choiceToolbar = (index: number) => {
    const state = stateAt(index);
    const choice = state.kind === "chosen" ? state.choice : null;
    return (
      <Toolbar aria-label={`${blockTitle(index)}: choice`}>
        {CHOICES.map((option) => (
          <button
            key={option.choice}
            aria-pressed={choice === option.choice}
            aria-keyshortcuts={mergeEditorAriaKeyShortcuts(option.shortcut) || undefined}
            title={withShortcutLabel(option.title, mergeEditorShortcutLabel(option.shortcut))}
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
    );
  };

  const renderBlock = (segment: ConflictBlock, index: number) => {
    const state = stateAt(index);
    const result = blockResult(segment, state);
    const choice = state.kind === "chosen" ? state.choice : null;
    const bothChosen = choice === "oursThenTheirs" || choice === "theirsThenOurs";
    const resultId = `${idPrefix}-result-${index}`;
    return blockSection(
      index,
      styles.block,
      <>
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
              <Code text={segment.ours} tokens={tokens?.ours[index]} marks={wordMarks[index]?.ours} />
            </pre>
          </div>
          <div className={`${styles.side} ${choice === "theirs" || bothChosen ? styles.chosen : ""}`}>
            <div className={styles.sideLabel}>{sideLabel("Theirs", names.theirs)}</div>
            <pre className={styles.segment}>
              <Code text={segment.theirs} tokens={tokens?.theirs[index]} marks={wordMarks[index]?.theirs} />
            </pre>
          </div>
        </div>
        {choiceToolbar(index)}
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
          onChange={(event) => editAt(index, event.target.value)}
        />
        {result === "" && <p className={styles.hint}>Empty: the block&apos;s lines are removed.</p>}
      </>,
    );
  };

  // F7 / Shift+F7 (#764) from anywhere in the blocks, the result fields too (they type no F7).
  // Undo and Redo (#878) undo the merge editor's own changes, in a result field too, and don't
  // reach the app's Git undo while the focus is in the editor.
  const onEditorKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const global = matchShortcut(event)?.id;
    if (global === "undo" || global === "redo") {
      event.preventDefault();
      event.stopPropagation();
      if (global === "undo") undoEdit();
      else redoEdit();
      return;
    }
    const id = matchMergeEditorShortcut(event)?.id;
    if (id !== "next-conflict" && id !== "previous-conflict") return;
    event.preventDefault();
    if (id === "next-conflict") goToNext();
    else goToPrevious();
  };

  // An unchanged run's result taking the focus in the panes is where F7 goes on from (#879).
  const onEditorFocus = (event: FocusEvent<HTMLDivElement>) => {
    const run = event.target instanceof Element ? event.target.closest("[data-run]") : null;
    if (run !== null) setRunCursor(Number(run.getAttribute("data-run")));
  };

  return (
    <Panel title="Resolve conflict">
      {header}
      <div ref={setEditorElement} onKeyDown={onEditorKeyDown} onFocus={onEditorFocus}>
        {conflictIndices.length > 0 && (
          <Toolbar aria-label="Conflicts">
            <span role="status">
              {resolvedCount} of {conflictIndices.length} resolved
            </span>
            <button
              disabled={!canGoPrevious}
              aria-keyshortcuts={mergeEditorAriaKeyShortcuts("previous-conflict") || undefined}
              title={withShortcutLabel("Previous conflict", mergeEditorShortcutLabel("previous-conflict"))}
              onClick={goToPrevious}
            >
              Previous conflict
            </button>
            <button
              disabled={!canGoNext}
              aria-keyshortcuts={mergeEditorAriaKeyShortcuts("next-conflict") || undefined}
              title={withShortcutLabel("Next conflict", mergeEditorShortcutLabel("next-conflict"))}
              onClick={goToNext}
            >
              Next conflict
            </button>
            <button aria-pressed={showBase} onClick={() => setShowBase(!showBase)}>
              Show base
            </button>
            <button
              disabled={history.past.length === 0}
              aria-keyshortcuts={ariaKeyShortcuts("undo") || undefined}
              title={withShortcutLabel("Undo the last change in the merge editor", formatChords(chordsFor("undo")))}
              onClick={undoEdit}
            >
              Undo
            </button>
            <button
              disabled={history.future.length === 0}
              aria-keyshortcuts={ariaKeyShortcuts("redo") || undefined}
              title={withShortcutLabel("Redo the change undone in the merge editor", formatChords(chordsFor("redo")))}
              onClick={redoEdit}
            >
              Redo
            </button>
            <button onClick={startWholeEdit}>Edit whole result</button>
            <span className={styles.segmented} role="group" aria-label="Merge editor layout">
              <button
                aria-pressed={panes}
                disabled={narrow}
                title={
                  narrow
                    ? "The pane is too narrow for three panes; widen it to use them."
                    : "The whole file in three panes: ours, the result and theirs."
                }
                onClick={() => setView("panes")}
              >
                Three panes
              </button>
              <button
                aria-pressed={!panes}
                title="Each conflict on its own, with both sides and the result."
                onClick={() => setView("blocks")}
              >
                Blocks
              </button>
            </span>
          </Toolbar>
        )}
        {panes ? (
          <MergePanes
            segments={segments}
            stateAt={stateAt}
            labels={{ ours: sideLabel("Ours", names.ours), result: "Result", theirs: sideLabel("Theirs", names.theirs) }}
            tokens={tokens}
            showBase={showBase}
            expanded={expanded}
            onExpand={(index) => setExpanded((prev) => new Set(prev).add(index))}
            contextLines={CONTEXT_LINES}
            renderBlock={(_segment, index, className, children) =>
              blockSection(
                index,
                className,
                <>
                  {choiceToolbar(index)}
                  {children}
                </>,
              )
            }
            onEdit={editAt}
            onTake={(index, side) => setStateAt(index, takeSide(stateAt(index), side))}
            blockTitle={blockTitle}
            wordMarks={wordMarks}
          />
        ) : (
          segments.map((segment, index) =>
            segment.kind === "Clean" ? renderClean(segment.content, index) : renderBlock(segment, index),
          )
        )}
      </div>
      {loaded && unchosenLeft && <p className={styles.hint}>Choose or type a result for every conflict to save.</p>}
      {loaded && markersLeft && (
        <p className={styles.hint}>
          Remove the conflict markers (lines starting with &lt;&lt;&lt;&lt;&lt;&lt;&lt; or
          &gt;&gt;&gt;&gt;&gt;&gt;&gt;) to save.
        </p>
      )}
      <button onClick={save} disabled={!canSave}>
        Save resolution
      </button>
    </Panel>
  );
}
