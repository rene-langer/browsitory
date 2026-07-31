import type { CommitInfo } from '@services/git'

interface CommitDetailsProps {
  commit: CommitInfo
}

export default function CommitDetails({ commit }: CommitDetailsProps) {
  const date = new Date(commit.author.timestamp).toLocaleString()

  return (
    <div className="p-4 border-b border-border">
      <h2 className="text-lg font-semibold text-foreground whitespace-pre-wrap">
        {commit.message}
      </h2>
      <p className="text-sm text-muted-foreground mt-2">
        {commit.author.name} &lt;{commit.author.email}&gt; · {date}
      </p>
      <p className="text-xs text-muted-foreground font-mono mt-1">{commit.oid}</p>
    </div>
  )
}
