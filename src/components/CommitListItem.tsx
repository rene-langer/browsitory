import clsx from 'clsx'
import type { CommitInfo } from '@services/git'

interface CommitListItemProps {
  commit: CommitInfo
  selected?: boolean
  onSelect: (oid: string) => void
}

export default function CommitListItem({ commit, selected, onSelect }: CommitListItemProps) {
  const date = new Date(commit.author.timestamp).toLocaleString()
  const shortOid = commit.oid.slice(0, 7)
  const firstLine = commit.message.split('\n')[0]

  return (
    <button
      type="button"
      onClick={() => onSelect(commit.oid)}
      className={clsx(
        'w-full text-left px-4 py-2 border-b border-border hover:bg-muted transition',
        selected && 'bg-muted'
      )}
    >
      <div className="font-medium text-foreground truncate">{firstLine}</div>
      <div className="text-xs text-muted-foreground mt-1">
        <span className="font-mono">{shortOid}</span> · {commit.author.name} · {date}
      </div>
    </button>
  )
}
