import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@services/git', () => ({
  getLog: vi.fn(async () => []),
  getCurrentBranch: vi.fn(async () => 'main'),
  getStatus: vi.fn(async () => ({ staged: [], unstaged: [], untracked: [] })),
  getCommitDiff: vi.fn(async () => []),
  getUnstagedDiff: vi.fn(async () => []),
  getStagedDiff: vi.fn(async () => []),
  stageFile: vi.fn(async () => undefined),
  unstageFile: vi.fn(async () => undefined),
  createCommit: vi.fn(async () => 'new-oid'),
}))

import { useGitStore } from './gitStore'
import * as gitService from '@services/git'
import type { OpenRepository } from './repositoryStore'

const repo: OpenRepository = { id: 'r1', name: 'repo', fs: { promises: {} } as never, dir: '/' }

beforeEach(() => {
  useGitStore.getState().reset()
  vi.clearAllMocks()
  vi.mocked(gitService.getLog).mockResolvedValue([])
  vi.mocked(gitService.getCurrentBranch).mockResolvedValue('main')
  vi.mocked(gitService.getStatus).mockResolvedValue({ staged: [], unstaged: [], untracked: [] })
})

describe('refresh', () => {
  it('loads commits, branch, and status', async () => {
    vi.mocked(gitService.getLog).mockResolvedValue([
      { oid: 'abc', message: 'msg', author: { name: 'a', email: 'a@b.c', timestamp: 0 }, parents: [] },
    ])
    vi.mocked(gitService.getStatus).mockResolvedValue({
      staged: ['a.txt'],
      unstaged: [],
      untracked: [],
    })

    await useGitStore.getState().refresh(repo)

    const state = useGitStore.getState()
    expect(state.commits).toHaveLength(1)
    expect(state.branch).toBe('main')
    expect(state.status.staged).toEqual(['a.txt'])
    expect(state.loading).toBe(false)
    expect(state.error).toBeNull()
  })

  it('sets an error if loading fails', async () => {
    vi.mocked(gitService.getLog).mockRejectedValue(new Error('boom'))

    await useGitStore.getState().refresh(repo)

    expect(useGitStore.getState().error).toBe('boom')
    expect(useGitStore.getState().loading).toBe(false)
  })
})

describe('stage/unstage', () => {
  it('stages a file and refreshes status', async () => {
    vi.mocked(gitService.getStatus).mockResolvedValue({
      staged: ['a.txt'],
      unstaged: [],
      untracked: [],
    })

    await useGitStore.getState().stage(repo, 'a.txt')

    expect(gitService.stageFile).toHaveBeenCalledWith(repo.fs, repo.dir, 'a.txt')
    expect(useGitStore.getState().status.staged).toEqual(['a.txt'])
  })

  it('unstages a file and refreshes status', async () => {
    vi.mocked(gitService.getStatus).mockResolvedValue({
      staged: [],
      unstaged: ['a.txt'],
      untracked: [],
    })

    await useGitStore.getState().unstage(repo, 'a.txt')

    expect(gitService.unstageFile).toHaveBeenCalledWith(repo.fs, repo.dir, 'a.txt')
    expect(useGitStore.getState().status.unstaged).toEqual(['a.txt'])
  })
})

describe('commit', () => {
  it('creates a commit, clears the selected diff, and refreshes', async () => {
    useGitStore.setState({
      selectedCommitOid: 'old',
      selectedDiff: [{ filepath: 'a', oldContent: '', newContent: '' }],
    })

    await useGitStore.getState().commit(repo, 'message', { name: 'a', email: 'a@b.c' })

    expect(gitService.createCommit).toHaveBeenCalledWith(repo.fs, repo.dir, {
      message: 'message',
      author: { name: 'a', email: 'a@b.c' },
    })
    expect(useGitStore.getState().selectedCommitOid).toBeNull()
  })
})

describe('selectCommit', () => {
  it('loads the diff for a commit', async () => {
    vi.mocked(gitService.getCommitDiff).mockResolvedValue([
      { filepath: 'a.txt', oldContent: 'x', newContent: 'y' },
    ])

    await useGitStore.getState().selectCommit(repo, 'abc123')

    const state = useGitStore.getState()
    expect(state.selectedCommitOid).toBe('abc123')
    expect(state.selectedDiff).toHaveLength(1)
  })
})
