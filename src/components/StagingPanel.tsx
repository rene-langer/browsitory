import type { StatusResult } from '@services/git'

interface FileRowProps {
  filepath: string
  actionLabel: string
  onAction: () => void
  onSelect: () => void
  onBlame: () => void
}

function FileRow({ filepath, actionLabel, onAction, onSelect, onBlame }: FileRowProps) {
  return (
    <div className="flex items-center justify-between px-3 py-1.5 hover:bg-muted rounded">
      <button
        type="button"
        onClick={onSelect}
        className="text-sm text-foreground truncate text-left flex-1"
      >
        {filepath}
      </button>
      <button
        type="button"
        onClick={onBlame}
        className="text-xs px-2 py-1 rounded hover:bg-accent hover:text-accent-foreground text-muted-foreground mr-1"
      >
        Blame
      </button>
      <button
        type="button"
        onClick={onAction}
        className="text-xs px-2 py-1 rounded bg-accent text-accent-foreground hover:opacity-90"
      >
        {actionLabel}
      </button>
    </div>
  )
}

interface StagingPanelProps {
  status: StatusResult
  onStage: (filepath: string) => void
  onUnstage: (filepath: string) => void
  onSelectUnstaged: (filepath: string) => void
  onSelectStaged: (filepath: string) => void
  onBlame: (filepath: string) => void
}

export default function StagingPanel({
  status,
  onStage,
  onUnstage,
  onSelectUnstaged,
  onSelectStaged,
  onBlame,
}: StagingPanelProps) {
  const unstagedFiles = [...status.unstaged, ...status.untracked]

  return (
    <div className="space-y-4 p-4">
      <div>
        <h3 className="text-sm font-semibold text-foreground mb-1">
          Staged Changes ({status.staged.length})
        </h3>
        {status.staged.length === 0 ? (
          <p className="text-xs text-muted-foreground px-3">Nothing staged.</p>
        ) : (
          status.staged.map((filepath) => (
            <FileRow
              key={filepath}
              filepath={filepath}
              actionLabel="Unstage"
              onAction={() => onUnstage(filepath)}
              onSelect={() => onSelectStaged(filepath)}
              onBlame={() => onBlame(filepath)}
            />
          ))
        )}
      </div>
      <div>
        <h3 className="text-sm font-semibold text-foreground mb-1">
          Changes ({unstagedFiles.length})
        </h3>
        {unstagedFiles.length === 0 ? (
          <p className="text-xs text-muted-foreground px-3">No changes.</p>
        ) : (
          unstagedFiles.map((filepath) => (
            <FileRow
              key={filepath}
              filepath={filepath}
              actionLabel="Stage"
              onAction={() => onStage(filepath)}
              onSelect={() => onSelectUnstaged(filepath)}
              onBlame={() => onBlame(filepath)}
            />
          ))
        )}
      </div>
    </div>
  )
}
