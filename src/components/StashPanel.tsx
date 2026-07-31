import { useState, type FormEvent } from 'react'
import type { StashEntry } from '@services/git'

interface StashPanelProps {
  stashes: StashEntry[]
  onCreate: (message?: string) => void
  onApply: (refIdx: number) => void
  onPop: (refIdx: number) => void
  onDrop: (refIdx: number) => void
}

export default function StashPanel({ stashes, onCreate, onApply, onPop, onDrop }: StashPanelProps) {
  const [message, setMessage] = useState('')

  const handleCreate = (e: FormEvent) => {
    e.preventDefault()
    const trimmed = message.trim()
    onCreate(trimmed.length > 0 ? trimmed : undefined)
    setMessage('')
  }

  return (
    <div className="p-4 space-y-2 border-t border-border">
      <h3 className="text-sm font-semibold text-foreground mb-1">Stashes ({stashes.length})</h3>

      <form onSubmit={handleCreate} className="flex gap-1">
        <input
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder="Stash message (optional)"
          aria-label="Stash message"
          className="flex-1 min-w-0 text-sm rounded border border-border bg-background px-2 py-1"
        />
        <button
          type="submit"
          className="text-xs px-2 py-1 rounded bg-accent text-accent-foreground hover:opacity-90"
        >
          Stash
        </button>
      </form>

      {stashes.length === 0 ? (
        <p className="text-xs text-muted-foreground px-1">No stashes.</p>
      ) : (
        stashes.map((s) => (
          <div
            key={s.index}
            className="flex items-center justify-between gap-2 px-1 py-1.5 rounded hover:bg-muted"
          >
            <span className="text-sm text-foreground truncate flex-1" title={s.message}>
              stash@{'{'}
              {s.index}
              {'}'}: {s.message}
            </span>
            <div className="flex gap-1 shrink-0">
              <button
                type="button"
                onClick={() => onApply(s.index)}
                className="text-xs px-2 py-1 rounded bg-accent text-accent-foreground hover:opacity-90"
              >
                Apply
              </button>
              <button
                type="button"
                onClick={() => onPop(s.index)}
                className="text-xs px-2 py-1 rounded bg-accent text-accent-foreground hover:opacity-90"
              >
                Pop
              </button>
              <button
                type="button"
                onClick={() => onDrop(s.index)}
                className="text-xs px-2 py-1 rounded bg-destructive text-destructive-foreground hover:opacity-90"
              >
                Drop
              </button>
            </div>
          </div>
        ))
      )}
    </div>
  )
}
