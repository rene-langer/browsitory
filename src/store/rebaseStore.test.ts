import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@services/git', () => ({
  resolveRef: vi.fn(async () => 'onto-oid'),
}))

vi.mock('@services/rebase', () => ({
  getRebaseState: vi.fn(async () => null),
  planRebase: vi.fn(async () => []),
  beginRebase: vi.fn(async () => ({ status: 'done' })),
  continueRebase: vi.fn(async () => ({ status: 'done' })),
  abortRebase: vi.fn(async () => undefined),
}))

import { useRebaseStore } from './rebaseStore'
import * as gitService from '@services/git'
import * as rebaseService from '@services/rebase'
import type { OpenRepository } from './repositoryStore'
import type { RebaseCommitPlan } from '@services/rebase'

const repo: OpenRepository = { id: 'r1', name: 'repo', fs: { promises: {} } as never, dir: '/' }
const COMMITTER = { name: 'C', email: 'c@x.com' }

const plan: RebaseCommitPlan[] = [
  { oid: 'a', message: 'one', author: { name: 'A', email: 'a@x.com', timestamp: 0 }, action: 'pick' },
  { oid: 'b', message: 'two', author: { name: 'A', email: 'a@x.com', timestamp: 0 }, action: 'pick' },
]

beforeEach(() => {
  useRebaseStore.getState().reset()
  vi.clearAllMocks()
  vi.mocked(rebaseService.getRebaseState).mockResolvedValue(null)
})

describe('checkForRebase', () => {
  it('loads the persisted state, if any', async () => {
    vi.mocked(rebaseService.getRebaseState).mockResolvedValue({
      branch: 'main',
      originalTip: 'x',
      ontoOid: 'y',
      committer: COMMITTER,
      plan,
      cursor: 0,
    })
    await useRebaseStore.getState().checkForRebase(repo)
    expect(useRebaseStore.getState().state?.branch).toBe('main')
  })
})

describe('loadPlanDraft', () => {
  it('resolves the onto ref and loads a plan', async () => {
    vi.mocked(gitService.resolveRef).mockResolvedValue('resolved-oid')
    vi.mocked(rebaseService.planRebase).mockResolvedValue(plan)

    await useRebaseStore.getState().loadPlanDraft(repo, 'main~2')

    expect(gitService.resolveRef).toHaveBeenCalledWith(repo.fs, repo.dir, 'main~2')
    expect(rebaseService.planRebase).toHaveBeenCalledWith(repo.fs, repo.dir, 'resolved-oid')
    const state = useRebaseStore.getState()
    expect(state.planDraft).toEqual(plan)
    expect(state.ontoOid).toBe('resolved-oid')
    expect(state.error).toBeNull()
  })

  it('sets an error when the ref cannot be resolved', async () => {
    vi.mocked(gitService.resolveRef).mockRejectedValue(new Error('unknown ref'))
    await useRebaseStore.getState().loadPlanDraft(repo, 'bogus')
    expect(useRebaseStore.getState().error).toBe('unknown ref')
  })
})

describe('movePlanEntry / toggleDrop', () => {
  it('swaps two entries', () => {
    useRebaseStore.setState({ planDraft: plan })
    useRebaseStore.getState().movePlanEntry(0, 1)
    expect(useRebaseStore.getState().planDraft.map((e) => e.oid)).toEqual(['b', 'a'])
  })

  it('is a no-op past the array bounds', () => {
    useRebaseStore.setState({ planDraft: plan })
    useRebaseStore.getState().movePlanEntry(0, -1)
    expect(useRebaseStore.getState().planDraft.map((e) => e.oid)).toEqual(['a', 'b'])
  })

  it('toggles an entry between pick and drop', () => {
    useRebaseStore.setState({ planDraft: plan })
    useRebaseStore.getState().toggleDrop(0)
    expect(useRebaseStore.getState().planDraft[0].action).toBe('drop')
    useRebaseStore.getState().toggleDrop(0)
    expect(useRebaseStore.getState().planDraft[0].action).toBe('pick')
  })
})

describe('start', () => {
  it('requires a loaded plan (ontoOid) before starting', async () => {
    const result = await useRebaseStore.getState().start(repo, COMMITTER)
    expect(result).toBeUndefined()
    expect(useRebaseStore.getState().error).toMatch(/load a rebase plan/i)
    expect(rebaseService.beginRebase).not.toHaveBeenCalled()
  })

  it('begins the rebase and refreshes state on success', async () => {
    useRebaseStore.setState({ planDraft: plan, ontoOid: 'onto-oid' })
    vi.mocked(rebaseService.beginRebase).mockResolvedValue({ status: 'done' })
    vi.mocked(rebaseService.getRebaseState).mockResolvedValue(null)

    const result = await useRebaseStore.getState().start(repo, COMMITTER)

    expect(rebaseService.beginRebase).toHaveBeenCalledWith(repo.fs, repo.dir, plan, 'onto-oid', COMMITTER)
    expect(result).toEqual({ status: 'done' })
    expect(useRebaseStore.getState().planDraft).toEqual([])
    expect(useRebaseStore.getState().ontoOid).toBeNull()
  })
})

describe('continue / abort', () => {
  it('continue resumes and refreshes state', async () => {
    vi.mocked(rebaseService.continueRebase).mockResolvedValue({ status: 'done' })
    const result = await useRebaseStore.getState().continue(repo)
    expect(rebaseService.continueRebase).toHaveBeenCalledWith(repo.fs, repo.dir)
    expect(result).toEqual({ status: 'done' })
  })

  it('abort clears state', async () => {
    useRebaseStore.setState({
      state: { branch: 'main', originalTip: 'x', ontoOid: 'y', committer: COMMITTER, plan, cursor: 0 },
    })
    await useRebaseStore.getState().abort(repo)
    expect(rebaseService.abortRebase).toHaveBeenCalledWith(repo.fs, repo.dir)
    expect(useRebaseStore.getState().state).toBeNull()
  })
})
