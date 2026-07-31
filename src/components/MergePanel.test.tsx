import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@services/git', () => ({
  listBranches: vi.fn(async () => ['main', 'feature']),
  getCurrentBranch: vi.fn(async () => 'main'),
  mergeBranch: vi.fn(async () => ({ status: 'merged' })),
  getConflictDiff: vi.fn(async () => ({
    ours: { filepath: 'a.txt', oldContent: 'base', newContent: 'ours' },
    theirs: { filepath: 'a.txt', oldContent: 'base', newContent: 'theirs' },
  })),
  stageFile: vi.fn(async () => undefined),
  createCommit: vi.fn(async () => 'new-oid'),
  abortCurrentMerge: vi.fn(async () => undefined),
}))

vi.mock('./DiffViewer', () => ({
  default: () => <div data-testid="mock-diff" />,
}))

import MergePanel from './MergePanel'
import * as gitService from '@services/git'
import type { OpenRepository } from '@store/repositoryStore'

const repo: OpenRepository = { id: 'r1', name: 'repo', fs: { promises: {} } as never, dir: '/' }

beforeEach(() => {
  vi.clearAllMocks()
  localStorage.clear()
  vi.mocked(gitService.listBranches).mockResolvedValue(['main', 'feature'])
  vi.mocked(gitService.getCurrentBranch).mockResolvedValue('main')
})

describe('MergePanel', () => {
  it('loads branches and defaults the target to a branch other than current', async () => {
    render(<MergePanel repo={repo} onClose={vi.fn()} onMerged={vi.fn()} />)
    await waitFor(() => expect(screen.getByText(/merge into/i)).toBeInTheDocument())
    expect(screen.getByRole('combobox')).toHaveValue('feature')
  })

  it('merges and reports success without conflicts', async () => {
    const onMerged = vi.fn()
    vi.mocked(gitService.mergeBranch).mockResolvedValue({ status: 'merged' })
    render(<MergePanel repo={repo} onClose={vi.fn()} onMerged={onMerged} />)
    await waitFor(() => expect(screen.getByRole('combobox')).toHaveValue('feature'))

    await userEvent.click(screen.getByRole('button', { name: /^merge$/i }))

    expect(await screen.findByText(/merge commit created/i)).toBeInTheDocument()
    expect(onMerged).toHaveBeenCalled()
  })

  it('shows conflicts and requires all to be marked resolved before completing', async () => {
    vi.mocked(gitService.mergeBranch).mockResolvedValue({ status: 'conflict', conflicts: ['a.txt'] })
    render(<MergePanel repo={repo} onClose={vi.fn()} onMerged={vi.fn()} />)
    await waitFor(() => expect(screen.getByRole('combobox')).toHaveValue('feature'))

    await userEvent.click(screen.getByRole('button', { name: /^merge$/i }))

    expect(await screen.findByText(/merge conflict in 1 file/i)).toBeInTheDocument()
    const completeButton = screen.getByRole('button', { name: /complete merge/i })
    expect(completeButton).toBeDisabled()

    await userEvent.click(await screen.findByRole('button', { name: /mark resolved/i }))
    expect(gitService.stageFile).toHaveBeenCalledWith(repo.fs, repo.dir, 'a.txt')
    await waitFor(() => expect(completeButton).toBeEnabled())

    await userEvent.click(completeButton)
    await waitFor(() => expect(gitService.createCommit).toHaveBeenCalled())
  })

  it('aborts the merge on demand', async () => {
    vi.mocked(gitService.mergeBranch).mockResolvedValue({ status: 'conflict', conflicts: ['a.txt'] })
    const onMerged = vi.fn()
    render(<MergePanel repo={repo} onClose={vi.fn()} onMerged={onMerged} />)
    await waitFor(() => expect(screen.getByRole('combobox')).toHaveValue('feature'))
    await userEvent.click(screen.getByRole('button', { name: /^merge$/i }))
    await screen.findByText(/merge conflict/i)

    await userEvent.click(screen.getByRole('button', { name: /abort/i }))
    expect(gitService.abortCurrentMerge).toHaveBeenCalledWith(repo.fs, repo.dir)
    expect(onMerged).toHaveBeenCalled()
  })

  it('calls onClose when Close is clicked', async () => {
    const onClose = vi.fn()
    render(<MergePanel repo={repo} onClose={onClose} onMerged={vi.fn()} />)
    await userEvent.click(screen.getByText('Close'))
    expect(onClose).toHaveBeenCalled()
  })
})
