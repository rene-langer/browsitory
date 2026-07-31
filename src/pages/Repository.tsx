import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useRepositoryStore } from '@store/repositoryStore'
import { useGitStore } from '@store/gitStore'
import { useRebaseStore } from '@store/rebaseStore'
import CommitList from '@components/CommitList'
import CommitDetails from '@components/CommitDetails'
import StagingPanel from '@components/StagingPanel'
import CommitForm from '@components/CommitForm'
import DiffViewer from '@components/DiffViewer'
import MergePanel from '@components/MergePanel'
import RebasePlanner from '@components/RebasePlanner'
import RebaseProgress from '@components/RebaseProgress'

const AUTHOR_NAME_KEY = 'browsitory:author-name'
const AUTHOR_EMAIL_KEY = 'browsitory:author-email'

type SidePanel = 'none' | 'merge' | 'rebase-plan'

export default function Repository() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const currentRepo = useRepositoryStore((s) => s.currentRepo)
  const repoError = useRepositoryStore((s) => s.error)
  const openRepositoryById = useRepositoryStore((s) => s.openRepositoryById)

  const {
    commits,
    branch,
    status,
    selectedCommitOid,
    selectedDiff,
    loading,
    error,
    refresh,
    selectCommit,
    stage,
    unstage,
    loadUnstagedDiff,
    loadStagedDiff,
    commit,
    reset,
  } = useGitStore()

  const {
    state: rebaseState,
    planDraft,
    loading: rebaseLoading,
    error: rebaseError,
    checkForRebase,
    loadPlanDraft,
    movePlanEntry,
    toggleDrop,
    clearPlanDraft,
    start: startRebase,
    continue: continueRebase,
    abort: abortRebase,
    reset: resetRebase,
  } = useRebaseStore()

  const [sidePanel, setSidePanel] = useState<SidePanel>('none')
  const [ontoInput, setOntoInput] = useState('')

  useEffect(() => {
    if (id && currentRepo?.id !== id) {
      openRepositoryById(id)
    }
  }, [id, currentRepo?.id, openRepositoryById])

  useEffect(() => {
    if (currentRepo && currentRepo.id === id) {
      refresh(currentRepo)
      checkForRebase(currentRepo)
    }
    return () => {
      reset()
      resetRebase()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentRepo])

  const closeSidePanel = () => {
    setSidePanel('none')
    setOntoInput('')
    clearPlanDraft()
  }

  const handleMergedOrRebased = () => {
    closeSidePanel()
    if (currentRepo) refresh(currentRepo)
  }

  const committer = {
    name: localStorage.getItem(AUTHOR_NAME_KEY) ?? '',
    email: localStorage.getItem(AUTHOR_EMAIL_KEY) ?? '',
  }

  const handleStartRebase = async () => {
    if (!currentRepo) return
    const result = await startRebase(currentRepo, committer)
    if (result?.status === 'done') {
      handleMergedOrRebased()
    } else {
      setSidePanel('none')
    }
  }

  const handleContinueRebase = async () => {
    if (!currentRepo) return
    const result = await continueRebase(currentRepo)
    if (result?.status === 'done') {
      refresh(currentRepo)
    }
  }

  const handleAbortRebase = async () => {
    if (!currentRepo) return
    await abortRebase(currentRepo)
    refresh(currentRepo)
  }

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
    <div className="flex h-full">
      <div className="w-96 border-r border-border flex flex-col overflow-hidden">
        <div className="p-4 border-b border-border">
          <h1 className="text-xl font-bold text-foreground truncate">{currentRepo.name}</h1>
          {branch && <p className="text-sm text-muted-foreground">on {branch}</p>}
          {!rebaseState && (
            <div className="flex gap-2 mt-2">
              <button
                type="button"
                onClick={() => setSidePanel('merge')}
                className="text-xs px-2 py-1 rounded bg-accent text-accent-foreground hover:opacity-90"
              >
                Merge…
              </button>
              <button
                type="button"
                onClick={() => setSidePanel('rebase-plan')}
                className="text-xs px-2 py-1 rounded bg-accent text-accent-foreground hover:opacity-90"
              >
                Rebase…
              </button>
            </div>
          )}
        </div>

        <StagingPanel
          status={status}
          onStage={(f) => stage(currentRepo, f)}
          onUnstage={(f) => unstage(currentRepo, f)}
          onSelectUnstaged={(f) => loadUnstagedDiff(currentRepo, f)}
          onSelectStaged={(f) => loadStagedDiff(currentRepo, f)}
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
        {rebaseState ? (
          <RebaseProgress
            repo={currentRepo}
            state={rebaseState}
            onContinue={handleContinueRebase}
            onAbort={handleAbortRebase}
            loading={rebaseLoading}
            error={rebaseError}
          />
        ) : sidePanel === 'merge' ? (
          <MergePanel repo={currentRepo} onClose={closeSidePanel} onMerged={handleMergedOrRebased} />
        ) : sidePanel === 'rebase-plan' ? (
          <RebasePlanner
            ontoInput={ontoInput}
            onOntoInputChange={setOntoInput}
            onLoadPlan={() => currentRepo && loadPlanDraft(currentRepo, ontoInput.trim())}
            plan={planDraft}
            onMove={movePlanEntry}
            onToggleDrop={toggleDrop}
            onStart={handleStartRebase}
            onCancel={closeSidePanel}
            loading={rebaseLoading}
            error={rebaseError}
          />
        ) : (
          <>
            {error && <p className="p-4 text-destructive text-sm">{error}</p>}
            {loading && <p className="p-4 text-muted-foreground text-sm">Loading…</p>}
            {selectedCommit && <CommitDetails commit={selectedCommit} />}
            <DiffViewer diffs={selectedDiff} />
          </>
        )}
      </div>
    </div>
  )
}
