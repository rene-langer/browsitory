import type { RebaseCommitPlan } from '@services/rebase'

interface RebasePlannerProps {
  ontoInput: string
  onOntoInputChange: (value: string) => void
  onLoadPlan: () => void
  plan: RebaseCommitPlan[]
  onMove: (index: number, direction: -1 | 1) => void
  onToggleDrop: (index: number) => void
  onStart: () => void
  onCancel: () => void
  loading: boolean
  error: string | null
}

export default function RebasePlanner({
  ontoInput,
  onOntoInputChange,
  onLoadPlan,
  plan,
  onMove,
  onToggleDrop,
  onStart,
  onCancel,
  loading,
  error,
}: RebasePlannerProps) {
  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-foreground">Interactive Rebase</h2>
        <button type="button" onClick={onCancel} className="text-sm text-muted-foreground hover:underline">
          Close
        </button>
      </div>

      <div className="flex gap-2">
        <input
          value={ontoInput}
          onChange={(e) => onOntoInputChange(e.target.value)}
          placeholder="Onto branch, tag, or commit oid"
          className="flex-1 rounded-md border border-border bg-background text-foreground p-2 text-sm font-mono"
        />
        <button
          type="button"
          onClick={onLoadPlan}
          disabled={!ontoInput.trim() || loading}
          className="px-3 py-2 rounded-md bg-accent text-accent-foreground hover:opacity-90 transition disabled:opacity-50"
        >
          Load plan
        </button>
      </div>

      {plan.length > 0 && (
        <div className="space-y-1">
          {plan.map((entry, index) => (
            <div
              key={entry.oid}
              className={`flex items-center gap-2 px-2 py-1 rounded border border-border ${
                entry.action === 'drop' ? 'opacity-50' : ''
              }`}
            >
              <span className="text-xs font-mono text-muted-foreground">{entry.oid.slice(0, 7)}</span>
              <span
                className={`text-sm flex-1 truncate text-foreground ${
                  entry.action === 'drop' ? 'line-through' : ''
                }`}
              >
                {entry.message.trim()}
              </span>
              <button
                type="button"
                onClick={() => onMove(index, -1)}
                disabled={index === 0}
                aria-label="Move up"
                className="text-xs px-1 disabled:opacity-30"
              >
                ↑
              </button>
              <button
                type="button"
                onClick={() => onMove(index, 1)}
                disabled={index === plan.length - 1}
                aria-label="Move down"
                className="text-xs px-1 disabled:opacity-30"
              >
                ↓
              </button>
              <button
                type="button"
                onClick={() => onToggleDrop(index)}
                className="text-xs px-2 py-0.5 rounded bg-accent text-accent-foreground hover:opacity-90"
              >
                {entry.action === 'drop' ? 'Pick' : 'Drop'}
              </button>
            </div>
          ))}

          <button
            type="button"
            onClick={onStart}
            disabled={loading}
            className="mt-2 w-full px-4 py-2 bg-primary text-primary-foreground rounded-md hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Start rebase
          </button>
        </div>
      )}

      {plan.length === 0 && !loading && (
        <p className="text-sm text-muted-foreground">
          Enter a target commit and load a plan to begin.
        </p>
      )}

      {error && <p className="text-sm text-destructive">{error}</p>}
    </div>
  )
}
