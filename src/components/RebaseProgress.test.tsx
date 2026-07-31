import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@services/git', () => ({
  getConflictDiff: vi.fn(async () => ({
    ours: { filepath: 'a.txt', oldContent: 'base', newContent: 'ours' },
    theirs: { filepath: 'a.txt', oldContent: 'base', newContent: 'theirs' },
  })),
  stageFile: vi.fn(async () => undefined),
}))

vi.mock('./DiffViewer', () => ({
  default: () => <div data-testid="mock-diff" />,
}))

import RebaseProgress from './RebaseProgress'
import * as gitService from '@services/git'
import type { OpenRepository } from '@store/repositoryStore'
import type { PersistedRebaseState } from '@services/rebase'

const repo: OpenRepository = { id: 'r1', name: 'repo', fs: { promises: {} } as never, dir: '/' }

const baseState: PersistedRebaseState = {
  branch: 'main',
  originalTip: 'aaa',
  ontoOid: 'bbb',
  committer: { name: 'C', email: 'c@x.com' },
  plan: [
    { oid: 'ccccccccffff', message: 'commit one', author: { name: 'A', email: 'a@x.com', timestamp: 0 }, action: 'pick' },
    { oid: 'ddddddddffff', message: 'commit two', author: { name: 'A', email: 'a@x.com', timestamp: 0 }, action: 'pick' },
  ],
  cursor: 0,
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('RebaseProgress', () => {
  it('shows step progress without conflicts', () => {
    render(
      <RebaseProgress
        repo={repo}
        state={baseState}
        onContinue={vi.fn()}
        onAbort={vi.fn()}
        loading={false}
        error={null}
      />
    )
    expect(screen.getByText(/step 1 of 2/i)).toBeInTheDocument()
    expect(screen.getByText(/commit one/)).toBeInTheDocument()
    expect(screen.queryByText(/continue/i)).not.toBeInTheDocument()
  })

  it('shows conflicts and lets the user mark resolved then continue', async () => {
    const onContinue = vi.fn()
    const conflicted: PersistedRebaseState = { ...baseState, conflictFilepaths: ['a.txt'] }
    render(
      <RebaseProgress
        repo={repo}
        state={conflicted}
        onContinue={onContinue}
        onAbort={vi.fn()}
        loading={false}
        error={null}
      />
    )
    expect(await screen.findByText(/conflict in 1 file/i)).toBeInTheDocument()

    await userEvent.click(screen.getByRole('button', { name: /mark resolved/i }))
    expect(gitService.stageFile).toHaveBeenCalledWith(repo.fs, repo.dir, 'a.txt')
    await waitFor(() => expect(screen.getByRole('button', { name: /resolved/i })).toBeDisabled())

    await userEvent.click(screen.getByRole('button', { name: /^continue$/i }))
    expect(onContinue).toHaveBeenCalled()
  })

  it('calls onAbort', async () => {
    const onAbort = vi.fn()
    render(
      <RebaseProgress
        repo={repo}
        state={baseState}
        onContinue={vi.fn()}
        onAbort={onAbort}
        loading={false}
        error={null}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: /abort rebase/i }))
    expect(onAbort).toHaveBeenCalled()
  })

  it('renders an error message when present', () => {
    render(
      <RebaseProgress
        repo={repo}
        state={baseState}
        onContinue={vi.fn()}
        onAbort={vi.fn()}
        loading={false}
        error="something went wrong"
      />
    )
    expect(screen.getByText('something went wrong')).toBeInTheDocument()
  })
})
