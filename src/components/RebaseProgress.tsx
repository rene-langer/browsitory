import { useEffect, useState } from 'react'
import * as gitService from '@services/git'
import type { FileDiff } from '@services/git'
import type { PersistedRebaseState } from '@services/rebase'
import type { OpenRepository } from '@store/repositoryStore'
import DiffViewer from './DiffViewer'

interface RebaseProgressProps {
  repo: OpenRepository
  state: PersistedRebaseState
  onContinue: () => void
  onAbort: () => void
  loading: boolean
  error: string | null
}

export default function RebaseProgress({
  repo,
  state,
  onContinue,
  onAbort,
  loading,
  error,
}: RebaseProgressProps) {
  const [conflictDiffs, setConflictDiffs] = useState<
    Record<string, { ours: FileDiff; theirs: FileDiff }>
  >({})
  const [resolved, setResolved] = useState<Set<string>>(new Set())

  const conflicts = state.conflictFilepaths ?? []
  const conflictsKey = conflicts.join(',')

  // Conflicts here come from a cherry-pick, not a merge, but the underlying
  // index layout is identical (stage 1/2/3 = base/ours/theirs), so
  // getConflictDiff — written against git.ts's merge conflict handling —
  // works unchanged for rebase conflicts too.
  useEffect(() => {
    setResolved(new Set())
    if (conflicts.length === 0) {
      setConflictDiffs({})
      return
    }
    let cancelled = false
    async function load() {
      const entries = await Promise.all(
        conflicts.map(
          async (fp) => [fp, await gitService.getConflictDiff(repo.fs, repo.dir, fp)] as const
        )
      )
      if (!cancelled) setConflictDiffs(Object.fromEntries(entries))
    }
    load()
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.cursor, conflictsKey, repo])

  const handleMarkResolved = async (filepath: string) => {
    await gitService.stageFile(repo.fs, repo.dir, filepath)
    setResolved((prev) => new Set(prev).add(filepath))
  }

  const current = state.plan[state.cursor]

  return (
    <div className="p-4 space-y-4">
      <h2 className="text-lg font-semibold text-foreground">Rebase in progress</h2>
      <p className="text-sm text-muted-foreground">
        Step {Math.min(state.cursor + 1, state.plan.length)} of {state.plan.length}
        {current && (
          <>
            {' — '}
            <span className="font-mono">{current.oid.slice(0, 7)}</span> {current.message.trim()}
          </>
        )}
      </p>

      {conflicts.length > 0 ? (
        <div className="space-y-4">
          <p className="text-sm text-destructive">
            Conflict in {conflicts.length} file{conflicts.length === 1 ? '' : 's'}.
          </p>
          {conflicts.map((fp) => (
            <div key={fp} className="border border-border rounded-md p-2 space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-mono">{fp}</span>
                <button
                  type="button"
                  onClick={() => handleMarkResolved(fp)}
                  disabled={resolved.has(fp)}
                  className="text-xs px-2 py-1 rounded bg-accent text-accent-foreground hover:opacity-90 disabled:opacity-50"
                >
                  {resolved.has(fp) ? 'Resolved' : 'Mark resolved'}
                </button>
              </div>
              {conflictDiffs[fp] && (
                <div className="space-y-2">
                  <div>
                    <p className="text-xs text-muted-foreground mb-1">Ours (HEAD)</p>
                    <DiffViewer diffs={[conflictDiffs[fp].ours]} />
                  </div>
                  <div>
                    <p className="text-xs text-muted-foreground mb-1">Theirs (picked commit)</p>
                    <DiffViewer diffs={[conflictDiffs[fp].theirs]} />
                  </div>
                </div>
              )}
            </div>
          ))}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={onContinue}
              disabled={loading}
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed"
            >
              Continue
            </button>
            <button
              type="button"
              onClick={onAbort}
              disabled={loading}
              className="px-4 py-2 border border-border rounded-md hover:bg-muted transition disabled:opacity-50"
            >
              Abort rebase
            </button>
          </div>
        </div>
      ) : (
        <button
          type="button"
          onClick={onAbort}
          disabled={loading}
          className="px-4 py-2 border border-border rounded-md hover:bg-muted transition disabled:opacity-50"
        >
          Abort rebase
        </button>
      )}

      {error && <p className="text-sm text-destructive">{error}</p>}
    </div>
  )
}
