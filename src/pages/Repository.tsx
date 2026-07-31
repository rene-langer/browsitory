import { useEffect } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useRepositoryStore } from '@store/repositoryStore'
import { useGitStore } from '@store/gitStore'
import CommitList from '@components/CommitList'
import CommitDetails from '@components/CommitDetails'
import StagingPanel from '@components/StagingPanel'
import CommitForm from '@components/CommitForm'
import DiffViewer from '@components/DiffViewer'
import BranchSwitcher from '@components/BranchSwitcher'
import StashPanel from '@components/StashPanel'

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
    branches,
    stashes,
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
    switchBranch,
    createBranch,
    deleteBranch,
    renameBranch,
    createStash,
    applyStash,
    popStash,
    dropStash,
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
          <BranchSwitcher
            branches={branches}
            currentBranch={branch}
            onSwitch={(name) => switchBranch(currentRepo, name)}
            onCreate={(name, startPoint) => createBranch(currentRepo, name, startPoint)}
            onDelete={(name) => deleteBranch(currentRepo, name)}
            onRename={(oldName, newName) => renameBranch(currentRepo, oldName, newName)}
          />
        </div>

        <StagingPanel
          status={status}
          onStage={(f) => stage(currentRepo, f)}
          onUnstage={(f) => unstage(currentRepo, f)}
          onSelectUnstaged={(f) => loadUnstagedDiff(currentRepo, f)}
          onSelectStaged={(f) => loadStagedDiff(currentRepo, f)}
        />

        <StashPanel
          stashes={stashes}
          onCreate={(message) => createStash(currentRepo, message)}
          onApply={(refIdx) => applyStash(currentRepo, refIdx)}
          onPop={(refIdx) => popStash(currentRepo, refIdx)}
          onDrop={(refIdx) => dropStash(currentRepo, refIdx)}
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
        {selectedCommit && <CommitDetails commit={selectedCommit} />}
        <DiffViewer diffs={selectedDiff} />
      </div>
    </div>
  )
}
