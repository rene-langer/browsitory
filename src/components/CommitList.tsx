import type { CommitInfo } from '@services/git'
import CommitListItem from './CommitListItem'

interface CommitListProps {
  commits: CommitInfo[]
  selectedOid?: string | null
  onSelect: (oid: string) => void
}

export default function CommitList({ commits, selectedOid, onSelect }: CommitListProps) {
  if (commits.length === 0) {
    return <p className="p-4 text-sm text-muted-foreground">No commits yet.</p>
  }

  return (
    <div role="list">
      {commits.map((commit) => (
        <CommitListItem
          key={commit.oid}
          commit={commit}
          selected={commit.oid === selectedOid}
          onSelect={onSelect}
        />
      ))}
    </div>
  )
}
