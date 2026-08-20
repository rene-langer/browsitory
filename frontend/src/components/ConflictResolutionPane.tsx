import { useEffect, useState } from "react";
import type { ConflictSegment, FileConflictChoice, RepoClient } from "../ipc/RepoClient";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./ConflictResolutionPane.module.css";

type Resolution = "ours" | "theirs" | "both";

export function ConflictResolutionPane({
  repoPath,
  client,
  path,
  onResolve,
  onResolveAddDelete,
}: {
  repoPath: string;
  client: RepoClient;
  path: string;
  onResolve: (path: string, resolvedContent: string) => void;
  onResolveAddDelete: (path: string, choice: FileConflictChoice) => void;
}) {
  const [segments, setSegments] = useState<ConflictSegment[]>([]);
  const [resolutions, setResolutions] = useState<Resolution[]>([]);
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
          setResolutions(next.map(() => "ours"));
          setError(null);
          setLoadedPath(path);
        }
      })
      .catch((err: unknown) => {
        if (!ignore) {
          const message = String(err);
          setError(message);
          setIsAddDeleteConflict(message.includes("add/delete conflict"));
          setLoadedPath(path);
        }
      });
    return () => {
      ignore = true;
    };
  }, [repoPath, client, path]);

  if (error !== null) {
    if (isAddDeleteConflict) {
      return (
        <Panel title="Resolve conflict">
          <p>{error}</p>
          <Toolbar>
            <button disabled={!loaded} onClick={() => onResolveAddDelete(path, "Ours")}>
              Keep Our Version
            </button>
            <button disabled={!loaded} onClick={() => onResolveAddDelete(path, "Theirs")}>
              Keep Their Version
            </button>
            <button disabled={!loaded} onClick={() => onResolveAddDelete(path, "Delete")}>
              Delete File
            </button>
          </Toolbar>
        </Panel>
      );
    }
    return <p role="alert">{error}</p>;
  }

  const setResolutionAt = (index: number, resolution: Resolution) => {
    setResolutions((prev) => prev.map((r, i) => (i === index ? resolution : r)));
  };

  const save = () => {
    // Each segment already carries its own embedded line terminators (see
    // `parse_conflict_markers` in `crates/git-core/src/merge.rs`), so segments are joined with
    // "" — no synthetic separator needed, and none of the old empty-side blank-line guards are
    // needed either: concatenating "" with anything is a no-op.
    const parts = segments.map((segment, index) => {
      if (segment.kind === "Clean") {
        return segment.content;
      }
      switch (resolutions[index]) {
        case "ours":
          return segment.ours;
        case "theirs":
          return segment.theirs;
        case "both":
          return segment.ours + segment.theirs;
      }
    });
    onResolve(path, parts.join(""));
  };

  return (
    <Panel title="Resolve conflict">
      {segments.map((segment, index) =>
        segment.kind === "Clean" ? (
          <pre key={index} className={styles.segment}>
            {segment.content}
          </pre>
        ) : (
          <div key={index}>
            <pre className={`${styles.segment} ${styles.segmentOurs}`}>Ours: {segment.ours}</pre>
            <pre className={`${styles.segment} ${styles.segmentTheirs}`}>
              Theirs: {segment.theirs}
            </pre>
            <Toolbar>
              <button onClick={() => setResolutionAt(index, "ours")}>Accept Ours</button>
              <button onClick={() => setResolutionAt(index, "theirs")}>Accept Theirs</button>
              <button onClick={() => setResolutionAt(index, "both")}>Accept Both</button>
            </Toolbar>
          </div>
        ),
      )}
      <button onClick={save} disabled={!loaded}>
        Save resolution
      </button>
    </Panel>
  );
}
