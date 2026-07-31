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
  listAllBranches: vi.fn(async () => []),
  createBranch: vi.fn(async () => undefined),
  deleteBranchByName: vi.fn(async () => undefined),
  renameBranchTo: vi.fn(async () => undefined),
  switchBranch: vi.fn(async () => undefined),
  listStashes: vi.fn(async () => []),
  createStash: vi.fn(async () => undefined),
  applyStash: vi.fn(async () => undefined),
  popStash: vi.fn(async () => undefined),
  dropStash: vi.fn(async () => undefined),
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
  vi.mocked(gitService.listAllBranches).mockResolvedValue([])
  vi.mocked(gitService.listStashes).mockResolvedValue([])
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

describe('refresh', () => {
  it('also loads branches and stashes', async () => {
    vi.mocked(gitService.listAllBranches).mockResolvedValue([
      { name: 'main', oid: 'abc', isCurrent: true },
      { name: 'feature', oid: 'def', isCurrent: false },
    ])
    vi.mocked(gitService.listStashes).mockResolvedValue([{ index: 0, message: 'WIP on main' }])

    await useGitStore.getState().refresh(repo)

    const state = useGitStore.getState()
    expect(state.branches).toHaveLength(2)
    expect(state.stashes).toEqual([{ index: 0, message: 'WIP on main' }])
  })
})

describe('branch actions', () => {
  it('switchBranch switches then refreshes, clearing the selected diff', async () => {
    useGitStore.setState({
      selectedCommitOid: 'old',
      selectedDiff: [{ filepath: 'a', oldContent: '', newContent: '' }],
    })

    await useGitStore.getState().switchBranch(repo, 'feature')

    expect(gitService.switchBranch).toHaveBeenCalledWith(repo.fs, repo.dir, 'feature')
    expect(gitService.getLog).toHaveBeenCalled() // refresh happened
    const state = useGitStore.getState()
    expect(state.selectedCommitOid).toBeNull()
    expect(state.selectedDiff).toEqual([])
    expect(state.error).toBeNull()
  })

  it('switchBranch sets an error and does not crash when it refuses due to local changes', async () => {
    vi.mocked(gitService.switchBranch).mockRejectedValue(
      new Error('Cannot switch to "feature": local changes would be overwritten.')
    )

    await useGitStore.getState().switchBranch(repo, 'feature')

    const state = useGitStore.getState()
    expect(state.error).toMatch(/local changes/i)
    expect(state.loading).toBe(false)
  })

  it('createBranch creates then refreshes', async () => {
    await useGitStore.getState().createBranch(repo, 'feature', 'abc123')
    expect(gitService.createBranch).toHaveBeenCalledWith(repo.fs, repo.dir, 'feature', 'abc123')
    expect(gitService.getLog).toHaveBeenCalled()
  })

  it('deleteBranch surfaces the guard error from the service layer', async () => {
    vi.mocked(gitService.deleteBranchByName).mockRejectedValue(
      new Error('Cannot delete branch "main" because it is currently checked out.')
    )

    await useGitStore.getState().deleteBranch(repo, 'main')

    expect(useGitStore.getState().error).toMatch(/currently checked out/i)
  })

  it('renameBranch renames then refreshes', async () => {
    await useGitStore.getState().renameBranch(repo, 'old', 'new')
    expect(gitService.renameBranchTo).toHaveBeenCalledWith(repo.fs, repo.dir, 'old', 'new')
    expect(gitService.getLog).toHaveBeenCalled()
  })
})

describe('stash actions', () => {
  it('createStash stashes then refreshes', async () => {
    await useGitStore.getState().createStash(repo, 'my wip')
    expect(gitService.createStash).toHaveBeenCalledWith(repo.fs, repo.dir, 'my wip')
    expect(gitService.getLog).toHaveBeenCalled()
  })

  it('applyStash applies then refreshes', async () => {
    await useGitStore.getState().applyStash(repo, 1)
    expect(gitService.applyStash).toHaveBeenCalledWith(repo.fs, repo.dir, 1)
    expect(gitService.getLog).toHaveBeenCalled()
  })

  it('popStash pops then refreshes', async () => {
    await useGitStore.getState().popStash(repo, 0)
    expect(gitService.popStash).toHaveBeenCalledWith(repo.fs, repo.dir, 0)
    expect(gitService.getLog).toHaveBeenCalled()
  })

  it('dropStash drops then refreshes', async () => {
    await useGitStore.getState().dropStash(repo, 0)
    expect(gitService.dropStash).toHaveBeenCalledWith(repo.fs, repo.dir, 0)
    expect(gitService.getLog).toHaveBeenCalled()
  })
})
