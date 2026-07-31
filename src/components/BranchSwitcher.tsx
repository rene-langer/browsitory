import { useState, type FormEvent } from 'react'
import type { BranchInfo } from '@services/git'

interface BranchSwitcherProps {
  branches: BranchInfo[]
  currentBranch?: string
  onSwitch: (name: string) => void
  onCreate: (name: string, startPoint?: string) => void
  onDelete: (name: string) => void
  onRename: (oldName: string, newName: string) => void
}

export default function BranchSwitcher({
  branches,
  currentBranch,
  onSwitch,
  onCreate,
  onDelete,
  onRename,
}: BranchSwitcherProps) {
  const [open, setOpen] = useState(false)
  const [newBranchName, setNewBranchName] = useState('')
  const [renamingBranch, setRenamingBranch] = useState<string | null>(null)
  const [renameValue, setRenameValue] = useState('')

  const startRename = (name: string) => {
    setRenamingBranch(name)
    setRenameValue(name)
  }

  const submitRename = (e: FormEvent, oldName: string) => {
    e.preventDefault()
    const newName = renameValue.trim()
    setRenamingBranch(null)
    if (!newName || newName === oldName) return
    onRename(oldName, newName)
  }

  const submitCreate = (e: FormEvent) => {
    e.preventDefault()
    const name = newBranchName.trim()
    if (!name) return
    onCreate(name)
    setNewBranchName('')
  }

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="text-sm text-muted-foreground hover:text-foreground flex items-center gap-1"
      >
        on {currentBranch ?? '…'}
        <span aria-hidden="true">▾</span>
      </button>

      {open && (
        <div
          role="menu"
          className="absolute z-10 mt-1 w-64 rounded-md border border-border bg-card shadow-md p-2 space-y-1"
        >
          {branches.length === 0 ? (
            <p className="text-xs text-muted-foreground px-2 py-1">No branches yet.</p>
          ) : (
            branches.map((b) => (
              <div
                key={b.name}
                className="flex items-center justify-between gap-1 px-2 py-1 rounded hover:bg-muted"
              >
                {renamingBranch === b.name ? (
                  <form onSubmit={(e) => submitRename(e, b.name)} className="flex-1 flex gap-1">
                    <input
                      value={renameValue}
                      onChange={(e) => setRenameValue(e.target.value)}
                      aria-label={`Rename ${b.name}`}
                      className="flex-1 min-w-0 text-sm rounded border border-border bg-background px-1"
                    />
                    <button type="submit" className="text-xs px-1 text-foreground">
                      Save
                    </button>
                  </form>
                ) : (
                  <>
                    <button
                      type="button"
                      disabled={b.isCurrent}
                      onClick={() => {
                        onSwitch(b.name)
                        setOpen(false)
                      }}
                      title={b.oid}
                      className={`flex-1 text-left text-sm truncate ${
                        b.isCurrent
                          ? 'font-semibold text-foreground cursor-default'
                          : 'text-muted-foreground hover:text-foreground'
                      }`}
                    >
                      {b.isCurrent ? '● ' : ''}
                      {b.name}
                    </button>
                    <button
                      type="button"
                      onClick={() => startRename(b.name)}
                      className="text-xs text-muted-foreground hover:text-foreground px-1"
                    >
                      Rename
                    </button>
                    {!b.isCurrent && (
                      <button
                        type="button"
                        onClick={() => onDelete(b.name)}
                        className="text-xs text-destructive hover:opacity-80 px-1"
                      >
                        Delete
                      </button>
                    )}
                  </>
                )}
              </div>
            ))
          )}

          <form
            onSubmit={submitCreate}
            className="flex gap-1 pt-2 mt-1 border-t border-border"
          >
            <input
              value={newBranchName}
              onChange={(e) => setNewBranchName(e.target.value)}
              placeholder="New branch name"
              aria-label="New branch name"
              className="flex-1 min-w-0 text-sm rounded border border-border bg-background px-2 py-1"
            />
            <button
              type="submit"
              className="text-xs px-2 py-1 rounded bg-accent text-accent-foreground hover:opacity-90"
            >
              Create
            </button>
          </form>
        </div>
      )}
    </div>
  )
}
