import { useMemo } from 'react'
import type { BlameLine } from '@services/blame'

interface BlameViewerProps {
  filepath: string
  lines: BlameLine[]
}

interface BlameRun {
  key: string
  commit: BlameLine['commit']
  lines: BlameLine[]
}

// Collapses consecutive lines attributed to the same commit into a single
// commit-info block, like real `git blame` output does, instead of repeating
// the commit's hash/author/date on every line.
function groupRuns(lines: BlameLine[]): BlameRun[] {
  const runs: BlameRun[] = []
  for (const line of lines) {
    const lastRun = runs[runs.length - 1]
    if (lastRun && lastRun.commit.oid === line.commit.oid) {
      lastRun.lines.push(line)
    } else {
      runs.push({ key: `${line.commit.oid}-${line.lineNumber}`, commit: line.commit, lines: [line] })
    }
  }
  return runs
}

export default function BlameViewer({ filepath, lines }: BlameViewerProps) {
  const runs = useMemo(() => groupRuns(lines), [lines])

  if (lines.length === 0) {
    return <p className="p-4 text-sm text-muted-foreground">No blame information available.</p>
  }

  return (
    <div className="p-4">
      <div className="px-3 py-2 bg-muted text-sm font-mono text-foreground rounded-t-md border border-border border-b-0">
        {filepath}
      </div>
      <div className="border border-border rounded-b-md overflow-hidden font-mono text-xs">
        {runs.map((run) => {
          const date = new Date(run.commit.author.timestamp).toLocaleDateString()
          const firstLine = run.commit.message.split('\n')[0]
          return (
            <div key={run.key} className="flex border-b border-border last:border-b-0">
              <div className="w-64 shrink-0 px-3 py-1.5 bg-muted text-muted-foreground border-r border-border">
                <div className="truncate text-foreground" title={run.commit.message}>
                  {firstLine}
                </div>
                <div className="truncate">
                  <span className="text-foreground">{run.commit.oid.slice(0, 7)}</span> ·{' '}
                  {run.commit.author.name} · {date}
                </div>
              </div>
              <div className="flex-1 min-w-0">
                {run.lines.map((line) => (
                  <div key={line.lineNumber} className="flex hover:bg-muted">
                    <span className="w-10 shrink-0 px-2 text-right text-muted-foreground select-none">
                      {line.lineNumber}
                    </span>
                    <span className="px-2 whitespace-pre text-foreground">{line.content}</span>
                  </div>
                ))}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}
