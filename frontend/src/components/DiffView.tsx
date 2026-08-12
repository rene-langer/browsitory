import type { DiffHunk, DiffLineOrigin } from "../ipc/RepoClient";

export function DiffView({ hunks }: { hunks: DiffHunk[] }) {
  if (hunks.length === 0) {
    return <p>No differences</p>;
  }

  return (
    <div>
      {hunks.map((hunk, hunkIndex) => (
        <div key={hunkIndex}>
          <div className="diff-hunk-header">
            @@ -{hunk.oldStart},{hunk.oldLines} +{hunk.newStart},{hunk.newLines} @@
          </div>
          <pre>
            {hunk.lines.map((line, lineIndex) => (
              <div
                key={lineIndex}
                className={`diff-line diff-line-${line.origin.toLowerCase()}`}
              >
                <span aria-hidden="true">{originPrefix(line.origin)}</span>
                {line.content}
              </div>
            ))}
          </pre>
        </div>
      ))}
    </div>
  );
}

function originPrefix(origin: DiffLineOrigin): string {
  switch (origin) {
    case "Add":
      return "+";
    case "Remove":
      return "-";
    case "Context":
      return " ";
  }
}
