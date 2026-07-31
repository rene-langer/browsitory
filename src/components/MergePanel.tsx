import { useEffect, useState } from 'react'
import * as gitService from '@services/git'
import type { FileDiff, MergeResult } from '@services/git'
import type { OpenRepository } from '@store/repositoryStore'
import DiffViewer from './DiffViewer'

// Reuses the same author-identity storage keys CommitForm.tsx uses — there is
// only one identity mechanism in this app (localStorage), not a separate one
// per feature.
const NAME_KEY = 'browsitory:author-name'
const EMAIL_KEY = 'browsitory:author-email'

interface MergePanelProps {
  repo: OpenRepository
  onClose: () => void
  onMerged: () => void
}

export default function MergePanel({ repo, onClose, onMerged }: MergePanelProps) {
  const [branches, setBranches] = useState<string[]>([])
  const [currentBranch, setCurrentBranch] = useState<string | undefined>()
  const [target, setTarget] = useState('')
  const [result, setResult] = useState<MergeResult | null>(null)
  const [conflictDiffs, setConflictDiffs] = useState<
    Record<string, { ours: FileDiff; theirs: FileDiff }>
  >({})
  const [resolved, setResolved] = useState<Set<string>>(new Set())
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Deliberately calling isomorphic-git's listBranches/currentBranch (via
  // git.ts's thin wrappers) directly here rather than depending on any
  // sibling BranchSwitcher work-in-progress elsewhere in the app.
  useEffect(() => {
    let cancelled = false
    async function load() {
      const [b, c] = await Promise.all([
        gitService.listBranches(repo.fs, repo.dir),
        gitService.getCurrentBranch(repo.fs, repo.dir),
      ])
      if (cancelled) return
      setBranches(b)
      setCurrentBranch(c)
      setTarget(b.find((name) => name !== c) ?? '')
    }
    load()
    return () => {
      cancelled = true
    }
  }, [repo])

  useEffect(() => {
    if (result?.status !== 'conflict' || !result.conflicts) {
      setConflictDiffs({})
      return
    }
    let cancelled = false
    async function loadDiffs() {
      const conflicts = result?.status === 'conflict' ? (result.conflicts ?? []) : []
      const entries = await Promise.all(
        conflicts.map(
          async (fp) => [fp, await gitService.getConflictDiff(repo.fs, repo.dir, fp)] as const
        )
      )
      if (!cancelled) setConflictDiffs(Object.fromEntries(entries))
    }
    loadDiffs()
    return () => {
      cancelled = true
    }
  }, [result, repo])

  const author = {
    name: localStorage.getItem(NAME_KEY) ?? '',
    email: localStorage.getItem(EMAIL_KEY) ?? '',
  }

  const handleMerge = async () => {
    if (!target) return
    setLoading(true)
    setError(null)
    try {
      const r = await gitService.mergeBranch(repo.fs, repo.dir, target, author)
      setResult(r)
      setResolved(new Set())
      if (r.status !== 'conflict') onMerged()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  const handleMarkResolved = async (filepath: string) => {
    try {
      await gitService.stageFile(repo.fs, repo.dir, filepath)
      setResolved((prev) => new Set(prev).add(filepath))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const handleComplete = async () => {
    setLoading(true)
    setError(null)
    try {
      await gitService.createCommit(repo.fs, repo.dir, {
        message: `Merge branch '${target}' into ${currentBranch ?? 'HEAD'}`,
        author,
      })
      setResult(null)
      onMerged()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  const handleAbort = async () => {
    setLoading(true)
    setError(null)
    try {
      await gitService.abortCurrentMerge(repo.fs, repo.dir)
      setResult(null)
      onMerged()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  const conflicts = result?.status === 'conflict' ? (result.conflicts ?? []) : []

  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-foreground">Merge</h2>
        <button type="button" onClick={onClose} className="text-sm text-muted-foreground hover:underline">
          Close
        </button>
      </div>

      {!result && (
        <div className="space-y-2">
          <p className="text-sm text-muted-foreground">
            Merge into <span className="font-mono">{currentBranch ?? '…'}</span>
          </p>
          <select
            value={target}
            onChange={(e) => setTarget(e.target.value)}
            className="w-full rounded-md border border-border bg-background text-foreground p-2 text-sm"
          >
            {branches
              .filter((b) => b !== currentBranch)
              .map((b) => (
                <option key={b} value={b}>
                  {b}
                </option>
              ))}
          </select>
          <button
            type="button"
            onClick={handleMerge}
            disabled={!target || loading}
            className="w-full px-4 py-2 bg-primary text-primary-foreground rounded-md hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Merge
          </button>
        </div>
      )}

      {result && result.status !== 'conflict' && (
        <p className="text-sm text-foreground">
          {result.status === 'already-up-to-date' && 'Already up to date.'}
          {result.status === 'fast-forward' && 'Fast-forwarded successfully.'}
          {result.status === 'merged' && 'Merge commit created successfully.'}
        </p>
      )}

      {result?.status === 'conflict' && (
        <div className="space-y-4">
          <p className="text-sm text-destructive">
            Merge conflict in {conflicts.length} file{conflicts.length === 1 ? '' : 's'}.
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
                    <p className="text-xs text-muted-foreground mb-1">Ours</p>
                    <DiffViewer diffs={[conflictDiffs[fp].ours]} />
                  </div>
                  <div>
                    <p className="text-xs text-muted-foreground mb-1">Theirs</p>
                    <DiffViewer diffs={[conflictDiffs[fp].theirs]} />
                  </div>
                </div>
              )}
            </div>
          ))}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={handleComplete}
              disabled={resolved.size < conflicts.length || loading}
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed"
            >
              Complete merge
            </button>
            <button
              type="button"
              onClick={handleAbort}
              disabled={loading}
              className="px-4 py-2 border border-border rounded-md hover:bg-muted transition disabled:opacity-50"
            >
              Abort
            </button>
          </div>
        </div>
      )}

      {error && <p className="text-sm text-destructive">{error}</p>}
    </div>
  )
}
