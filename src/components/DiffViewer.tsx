import ReactDiffViewer from 'react-diff-viewer-continued'
import type { FileDiff } from '@services/git'

interface DiffViewerProps {
  diffs: FileDiff[]
}

export default function DiffViewer({ diffs }: DiffViewerProps) {
  if (diffs.length === 0) {
    return <p className="p-4 text-sm text-muted-foreground">No changes.</p>
  }

  return (
    <div className="space-y-6 p-4">
      {diffs.map((diff) => (
        <div key={diff.filepath} className="border border-border rounded-md overflow-hidden">
          <div className="px-3 py-2 bg-muted text-sm font-mono text-foreground">
            {diff.filepath}
          </div>
          <ReactDiffViewer oldValue={diff.oldContent} newValue={diff.newContent} splitView={false} />
        </div>
      ))}
    </div>
  )
}
