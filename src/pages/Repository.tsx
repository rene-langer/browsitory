import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useRepositoryStore } from '@store/repositoryStore'
import { useGitStore } from '@store/gitStore'
import CommitList from '@components/CommitList'
import CommitDetails from '@components/CommitDetails'
import StagingPanel from '@components/StagingPanel'
import CommitForm from '@components/CommitForm'
import DiffViewer from '@components/DiffViewer'
import BlameViewer from '@components/BlameViewer'
import GraphView from '@components/GraphView'

export default function Repository() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const currentRepo = useRepositoryStore((s) => s.currentRepo)
  const repoError = useRepositoryStore((s) => s.error)
  const openRepositoryById = useRepositoryStore((s) => s.openRepositoryById)
  const [view, setView] = useState<'history' | 'graph'>('history')

  const {
    commits,
    branch,
    status,
    selectedCommitOid,
    selectedDiff,
    blameFilepath,
    blame,
    graphCommits,
    loading,
    error,
    refresh,
    selectCommit,
    stage,
    unstage,
    loadUnstagedDiff,
    loadStagedDiff,
    loadBlame,
    loadGraph,
    commit,
    reset,
  } = useGitStore()

  useEffect(() => {
    if (id && currentRepo?.id !== id) {
      openRepositoryById(id)
    }
  }, [id, currentRepo?.id, openRepositoryById])

  useEffect(() => {
    if (currentRepo && currentRepo.id === id) {
      refresh(currentRepo)
    }
    return () => reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentRepo])

  useEffect(() => {
    if (view === 'graph' && currentRepo && currentRepo.id === id) {
      loadGraph(currentRepo)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view, currentRepo])

  if (repoError) {
    return (
      <div className="p-8">
        <p className="text-destructive mb-4">{repoError}</p>
        <button type="button" onClick={() => navigate('/')} className="text-primary hover:underline">
          Back to Dashboard
        </button>
      </div>
    )
  }

  if (!currentRepo || currentRepo.id !== id) {
    return <div className="p-8 text-muted-foreground">Opening repository…</div>
  }

  const selectedCommit = commits.find((c) => c.oid === selectedCommitOid)

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between border-b border-border px-4 py-2 shrink-0">
        <div>
          <h1 className="text-xl font-bold text-foreground truncate">{currentRepo.name}</h1>
          {branch && <p className="text-sm text-muted-foreground">on {branch}</p>}
        </div>
        <div className="flex gap-1">
          <button
            type="button"
            onClick={() => setView('history')}
            className={
              view === 'history'
                ? 'text-xs px-3 py-1.5 rounded bg-accent text-accent-foreground'
                : 'text-xs px-3 py-1.5 rounded text-muted-foreground hover:bg-muted'
            }
          >
            History
          </button>
          <button
            type="button"
            onClick={() => setView('graph')}
            className={
              view === 'graph'
                ? 'text-xs px-3 py-1.5 rounded bg-accent text-accent-foreground'
                : 'text-xs px-3 py-1.5 rounded text-muted-foreground hover:bg-muted'
            }
          >
            Graph
          </button>
        </div>
      </div>

      {view === 'graph' ? (
        <div className="flex-1 overflow-auto">
          {error && <p className="p-4 text-destructive text-sm">{error}</p>}
          {loading && <p className="p-4 text-muted-foreground text-sm">Loading…</p>}
          <GraphView
            commits={graphCommits}
            selectedOid={selectedCommitOid}
            onSelect={(oid) => selectCommit(currentRepo, oid)}
          />
        </div>
      ) : (
        <div className="flex flex-1 overflow-hidden">
          <div className="w-96 border-r border-border flex flex-col overflow-hidden">
            <StagingPanel
              status={status}
              onStage={(f) => stage(currentRepo, f)}
              onUnstage={(f) => unstage(currentRepo, f)}
              onSelectUnstaged={(f) => loadUnstagedDiff(currentRepo, f)}
              onSelectStaged={(f) => loadStagedDiff(currentRepo, f)}
              onBlame={(f) => loadBlame(currentRepo, f)}
            />

            <CommitForm
              disabled={status.staged.length === 0}
              onCommit={(message, author) => commit(currentRepo, message, author)}
            />

            <div className="flex-1 overflow-auto border-t border-border">
              <CommitList
                commits={commits}
                selectedOid={selectedCommitOid}
                onSelect={(oid) => selectCommit(currentRepo, oid)}
              />
            </div>
          </div>

          <div className="flex-1 overflow-auto">
            {error && <p className="p-4 text-destructive text-sm">{error}</p>}
            {loading && <p className="p-4 text-muted-foreground text-sm">Loading…</p>}
            {blameFilepath ? (
              <BlameViewer filepath={blameFilepath} lines={blame} />
            ) : (
              <>
                {selectedCommit && <CommitDetails commit={selectedCommit} />}
                <DiffViewer diffs={selectedDiff} />
              </>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
