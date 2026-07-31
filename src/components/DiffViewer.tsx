import ReactDiffViewer, { DiffMethod } from 'react-diff-viewer-continued'
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
          <ReactDiffViewer
            oldValue={diff.oldContent}
            newValue={diff.newContent}
            splitView={false}
            // The library defaults to character-level diffing (DiffMethod.CHARS),
            // which highlights individual changed characters mid-word — on prose
            // or code with multiple small edits per line this reads as noisy,
            // fragmented highlighting (confirmed against a real diff: word
            // boundaries visually disappear, e.g. "Requires the" rendering with
            // no gap between the words). Word-level diffing, keeping whitespace
            // attached to each word token, matches how GitHub/GitLab render
            // diffs and is far more readable for anything beyond single-token
            // changes.
            compareMethod={DiffMethod.WORDS_WITH_SPACE}
          />
        </div>
      ))}
    </div>
  )
}
