import { useEffect, useState } from "react";
import type { ConflictSegment, RepoClient } from "../ipc/RepoClient";

type Resolution = "ours" | "theirs" | "both";

export function ConflictResolutionPane({
  client,
  path,
  onResolve,
}: {
  client: RepoClient;
  path: string;
  onResolve: (path: string, resolvedContent: string) => void;
}) {
  const [segments, setSegments] = useState<ConflictSegment[]>([]);
  const [resolutions, setResolutions] = useState<Resolution[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let ignore = false;
    client
      .getConflictHunks(path)
      .then((next) => {
        if (!ignore) {
          setSegments(next);
          setResolutions(next.map(() => "ours"));
          setError(null);
        }
      })
      .catch((err: unknown) => {
        if (!ignore) {
          setError(String(err));
        }
      });
    return () => {
      ignore = true;
    };
  }, [client, path]);

  if (error !== null) {
    return <p role="alert">{error}</p>;
  }

  const setResolutionAt = (index: number, resolution: Resolution) => {
    setResolutions((prev) => prev.map((r, i) => (i === index ? resolution : r)));
  };

  const save = () => {
    const parts = segments.map((segment, index) => {
      if (segment.kind === "Clean") {
        return segment.content;
      }
      switch (resolutions[index]) {
        case "ours":
          return segment.ours;
        case "theirs":
          return segment.theirs;
        case "both": {
          const separator = segment.ours.endsWith("\n") ? "" : "\n";
          return `${segment.ours}${separator}${segment.theirs}`;
        }
      }
    });
    onResolve(path, parts.join("\n"));
  };

  return (
    <div>
      {segments.map((segment, index) =>
        segment.kind === "Clean" ? (
          <pre key={index}>{segment.content}</pre>
        ) : (
          <div key={index}>
            <pre>Ours: {segment.ours}</pre>
            <pre>Theirs: {segment.theirs}</pre>
            <button onClick={() => setResolutionAt(index, "ours")}>Accept Ours</button>
            <button onClick={() => setResolutionAt(index, "theirs")}>Accept Theirs</button>
            <button onClick={() => setResolutionAt(index, "both")}>Accept Both</button>
          </div>
        ),
      )}
      <button onClick={save}>Save resolution</button>
    </div>
  );
}
