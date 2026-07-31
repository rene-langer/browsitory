import { useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { useRepositoryStore } from '@store/repositoryStore'

export default function Dashboard() {
  const navigate = useNavigate()
  const {
    repositories,
    loading,
    error,
    loadRepositories,
    openRepositoryPicker,
    openRepositoryById,
    removeRepository,
    clearError,
  } = useRepositoryStore()

  useEffect(() => {
    loadRepositories()
  }, [loadRepositories])

  const handleOpen = async () => {
    const id = await openRepositoryPicker()
    if (id) navigate(`/repo/${id}`)
  }

  const handleOpenExisting = async (id: string) => {
    await openRepositoryById(id)
    if (!useRepositoryStore.getState().error) navigate(`/repo/${id}`)
  }

  const supportsFileSystemAccess = typeof window !== 'undefined' && 'showDirectoryPicker' in window

  return (
    <div className="p-8">
      <div className="max-w-2xl">
        <h1 className="text-3xl font-bold text-foreground mb-4">Dashboard</h1>
        <p className="text-muted-foreground mb-6">
          Welcome to Browsitory, your Git repository manager. Open a local repository to get
          started.
        </p>

        {error && (
          <div className="mb-4 p-3 rounded-md bg-destructive/10 text-destructive text-sm flex items-start justify-between gap-4">
            <span>{error}</span>
            <button type="button" onClick={clearError} className="underline shrink-0">
              Dismiss
            </button>
          </div>
        )}

        {!supportsFileSystemAccess && (
          <div className="mb-4 p-3 rounded-md bg-muted text-sm text-muted-foreground">
            Your browser doesn&apos;t support the File System Access API required to open local
            repositories. Please use Chrome, Edge, or another Chromium-based browser.
          </div>
        )}

        <div className="space-y-4">
          <button
            type="button"
            onClick={handleOpen}
            disabled={loading}
            className="px-6 py-2 bg-primary text-primary-foreground rounded-md hover:opacity-90 transition disabled:opacity-50"
          >
            {loading ? 'Opening…' : 'Open Repository'}
          </button>
        </div>

        {repositories.length > 0 && (
          <div className="mt-8">
            <h2 className="text-lg font-semibold text-foreground mb-3">Recent Repositories</h2>
            <div className="space-y-2">
              {repositories.map((repo) => (
                <div
                  key={repo.id}
                  className="flex items-center justify-between p-3 bg-card border border-border rounded-lg"
                >
                  <button
                    type="button"
                    onClick={() => handleOpenExisting(repo.id)}
                    className="text-left flex-1 text-foreground hover:underline"
                  >
                    {repo.name}
                  </button>
                  <button
                    type="button"
                    onClick={() => removeRepository(repo.id)}
                    className="text-xs text-muted-foreground hover:text-destructive"
                  >
                    Remove
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="mt-12 p-6 bg-card border border-border rounded-lg">
          <h2 className="text-xl font-semibold text-foreground mb-4">Features</h2>
          <ul className="space-y-2 text-muted-foreground">
            <li>✓ View commit history</li>
            <li>✓ Visual diff viewer</li>
            <li>✓ Stage/unstage changes</li>
            <li>✓ Create commits</li>
            <li>○ Branch management (coming soon)</li>
          </ul>
        </div>
      </div>
    </div>
  )
}
