import { create } from 'zustand'
import * as gitService from '@services/git'
import type { CommitAuthor } from '@services/git'
import * as rebaseService from '@services/rebase'
import type { PersistedRebaseState, RebaseCommitPlan, RebaseResult } from '@services/rebase'
import type { OpenRepository } from './repositoryStore'

interface RebaseState {
  /** The persisted, in-progress rebase (if any) — mirrors PersistedRebaseState. */
  state: PersistedRebaseState | null
  /** UI-only pre-start plan editor, not yet persisted or started. */
  planDraft: RebaseCommitPlan[]
  ontoOid: string | null
  loading: boolean
  error: string | null

  /** Checks for a paused rebase on repo load (getRebaseState is non-throwing). */
  checkForRebase: (repo: OpenRepository) => Promise<void>
  /** Resolves `ontoRef` (branch name, tag, or oid) and builds the pre-start plan. */
  loadPlanDraft: (repo: OpenRepository, ontoRef: string) => Promise<void>
  movePlanEntry: (index: number, direction: -1 | 1) => void
  toggleDrop: (index: number) => void
  clearPlanDraft: () => void
  start: (repo: OpenRepository, committer: CommitAuthor) => Promise<RebaseResult | undefined>
  continue: (repo: OpenRepository) => Promise<RebaseResult | undefined>
  abort: (repo: OpenRepository) => Promise<void>
  reset: () => void
}

function describeError(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}

export const useRebaseStore = create<RebaseState>((set, get) => ({
  state: null,
  planDraft: [],
  ontoOid: null,
  loading: false,
  error: null,

  checkForRebase: async (repo) => {
    try {
      const state = await rebaseService.getRebaseState(repo.fs, repo.dir)
      set({ state })
    } catch (err) {
      set({ error: describeError(err) })
    }
  },

  loadPlanDraft: async (repo, ontoRef) => {
    set({ loading: true, error: null })
    try {
      const ontoOid = await gitService.resolveRef(repo.fs, repo.dir, ontoRef)
      const planDraft = await rebaseService.planRebase(repo.fs, repo.dir, ontoOid)
      set({ planDraft, ontoOid, loading: false })
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  movePlanEntry: (index, direction) => {
    const plan = [...get().planDraft]
    const target = index + direction
    if (target < 0 || target >= plan.length) return
    ;[plan[index], plan[target]] = [plan[target], plan[index]]
    set({ planDraft: plan })
  },

  toggleDrop: (index) => {
    const plan = get().planDraft.map((entry, i) =>
      i === index ? { ...entry, action: entry.action === 'drop' ? ('pick' as const) : ('drop' as const) } : entry
    )
    set({ planDraft: plan })
  },

  clearPlanDraft: () => set({ planDraft: [], ontoOid: null }),

  start: async (repo, committer) => {
    const { planDraft, ontoOid } = get()
    if (!ontoOid) {
      set({ error: 'Load a rebase plan before starting.' })
      return undefined
    }
    set({ loading: true, error: null })
    try {
      const result = await rebaseService.beginRebase(repo.fs, repo.dir, planDraft, ontoOid, committer)
      const state = await rebaseService.getRebaseState(repo.fs, repo.dir)
      set({ state, planDraft: [], ontoOid: null, loading: false })
      return result
    } catch (err) {
      set({ loading: false, error: describeError(err) })
      return undefined
    }
  },

  continue: async (repo) => {
    set({ loading: true, error: null })
    try {
      const result = await rebaseService.continueRebase(repo.fs, repo.dir)
      const state = await rebaseService.getRebaseState(repo.fs, repo.dir)
      set({ state, loading: false })
      return result
    } catch (err) {
      set({ loading: false, error: describeError(err) })
      return undefined
    }
  },

  abort: async (repo) => {
    set({ loading: true, error: null })
    try {
      await rebaseService.abortRebase(repo.fs, repo.dir)
      set({ state: null, loading: false })
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  reset: () => set({ state: null, planDraft: [], ontoOid: null, loading: false, error: null }),
}))
