import { useEffect, useId, useRef, useState } from "react";
import { errorKind, type ConflictSegment, type ConflictSide, type FileConflictChoice, type RepoClient } from "../ipc/RepoClient";
import { conflictSideNames, type ConflictSideNames } from "../lib/conflictSides";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./ConflictResolutionPane.module.css";

type Resolution = "ours" | "theirs" | "both";

const CHOICES: { resolution: Resolution; label: string }[] = [
  { resolution: "ours", label: "Accept ours" },
  { resolution: "theirs", label: "Accept theirs" },
  { resolution: "both", label: "Accept both" },
];

function resolvedText(segment: Extract<ConflictSegment, { kind: "Conflict" }>, resolution: Resolution): string {
  switch (resolution) {
    case "ours":
      return segment.ours;
    case "theirs":
      return segment.theirs;
    case "both":
      return segment.ours + segment.theirs;
  }
}

function sideLabel(side: string, name: string | null): string {
  return name === null ? side : `${side} (${name})`;
}

const NO_NAMES: ConflictSideNames = { ours: null, theirs: null };

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
  // `null`: not chosen yet. Every block starts that way, so Save never takes a side the user
  // didn't pick (#654).
  const [resolutions, setResolutions] = useState<(Resolution | null)[]>([]);
  const [names, setNames] = useState<ConflictSideNames>(NO_NAMES);
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
          setResolutions(next.map(() => null));
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

  const setResolutionAt = (index: number, resolution: Resolution) => {
    setResolutions((prev) => prev.map((r, i) => (i === index ? resolution : r)));
  };

  const conflictIndices = segments.flatMap((segment, index) => (segment.kind === "Conflict" ? [index] : []));
  const resolvedCount = conflictIndices.filter((index) => resolutions[index] !== null).length;
  const allResolved = resolvedCount === conflictIndices.length;

  const goToConflict = (position: number) => {
    setCurrentConflict(position);
    const block = blockRefs.current[conflictIndices[position]];
    block?.focus();
    block?.scrollIntoView?.({ block: "nearest" });
  };

  const save = () => {
    if (!allResolved) return;
    // Each segment already carries its own embedded line terminators (see
    // `parse_conflict_markers` in `crates/git-core/src/merge.rs`), so segments are joined with
    // "" — no synthetic separator needed, and none of the old empty-side blank-line guards are
    // needed either: concatenating "" with anything is a no-op.
    const parts = segments.map((segment, index) => {
      if (segment.kind === "Clean") {
        return segment.content;
      }
      const resolution = resolutions[index];
      return resolution === null ? "" : resolvedText(segment, resolution);
    });
    onResolve(path, parts.join(""));
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
        </Toolbar>
      )}
      {segments.map((segment, index) => {
        if (segment.kind === "Clean") {
          return (
            <pre key={index} className={styles.segment}>
              {segment.content}
            </pre>
          );
        }
        const position = conflictIndices.indexOf(index);
        const resolution = resolutions[index] ?? null;
        const title = `Conflict ${position + 1} of ${conflictIndices.length}`;
        const result = resolution === null ? null : resolvedText(segment, resolution);
        const oursChosen = resolution === "ours" || resolution === "both";
        const theirsChosen = resolution === "theirs" || resolution === "both";
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
            <h4 className={styles.blockTitle}>{title}</h4>
            <div className={`${styles.side} ${oursChosen ? styles.chosen : ""}`}>
              <div className={styles.sideLabel}>{sideLabel("Ours", names.ours)}</div>
              <pre className={styles.segment}>{segment.ours}</pre>
            </div>
            <div className={`${styles.side} ${theirsChosen ? styles.chosen : ""}`}>
              <div className={styles.sideLabel}>{sideLabel("Theirs", names.theirs)}</div>
              <pre className={styles.segment}>{segment.theirs}</pre>
            </div>
            <Toolbar aria-label={`${title}: choice`}>
              {CHOICES.map((choice) => (
                <button
                  key={choice.resolution}
                  aria-pressed={resolution === choice.resolution}
                  onClick={() => setResolutionAt(index, choice.resolution)}
                >
                  {choice.label}
                </button>
              ))}
            </Toolbar>
            <div className={styles.sideLabel} id={`${idPrefix}-result-${index}`}>
              Result
            </div>
            <pre
              className={`${styles.segment} ${result === null || result === "" ? styles.placeholder : ""}`}
              aria-labelledby={`${idPrefix}-result-${index}`}
            >
              {result === null ? "Not chosen yet" : result === "" ? "Empty: the block's lines are removed" : result}
            </pre>
          </section>
        );
      })}
      {loaded && !allResolved && <p className={styles.hint}>Choose a side for every conflict to save.</p>}
      <button onClick={save} disabled={!loaded || !allResolved}>
        Save resolution
      </button>
    </Panel>
  );
}
