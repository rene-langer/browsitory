import type { BlameLine } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";

export function BlameView({
  lines,
  onSelectRow,
}: {
  lines: BlameLine[];
  onSelectRow: (row: SelectedRow) => void;
}) {
  return (
    <table>
      <tbody>
        {lines.map((line) => (
          <tr key={line.lineNumber} onClick={() => onSelectRow({ commitId: line.commitId })}>
            <td>{line.lineNumber}</td>
            <td>{line.shortId}</td>
            <td>{line.authorName}</td>
            <td>{line.content}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
