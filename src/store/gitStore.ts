import { create } from 'zustand'
import * as gitService from '@services/git'
import type { CommitInfo, FileDiff, GraphCommit, StatusResult } from '@services/git'
import * as blameService from '@services/blame'
import type { BlameLine } from '@services/blame'
import type { OpenRepository } from './repositoryStore'

interface GitState {
  commits: CommitInfo[]
  branch: string | undefined
  status: StatusResult
  selectedCommitOid: string | null
  selectedDiff: FileDiff[]
  blameFilepath: string | null
  blame: BlameLine[]
  graphCommits: GraphCommit[]
  loading: boolean
  error: string | null

  refresh: (repo: OpenRepository) => Promise<void>
  selectCommit: (repo: OpenRepository, oid: string) => Promise<void>
  stage: (repo: OpenRepository, filepath: string) => Promise<void>
  unstage: (repo: OpenRepository, filepath: string) => Promise<void>
  loadUnstagedDiff: (repo: OpenRepository, filepath: string) => Promise<void>
  loadStagedDiff: (repo: OpenRepository, filepath: string) => Promise<void>
  loadBlame: (repo: OpenRepository, filepath: string) => Promise<void>
  loadGraph: (repo: OpenRepository) => Promise<void>
  commit: (
    repo: OpenRepository,
    message: string,
    author: { name: string; email: string }
  ) => Promise<void>
  reset: () => void
}

const emptyStatus: StatusResult = { staged: [], unstaged: [], untracked: [] }

function describeError(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}

export const useGitStore = create<GitState>((set, get) => ({
  commits: [],
  branch: undefined,
  status: emptyStatus,
  selectedCommitOid: null,
  selectedDiff: [],
  blameFilepath: null,
  blame: [],
  graphCommits: [],
  loading: false,
  error: null,

  refresh: async (repo) => {
    set({ loading: true, error: null })
    try {
      const [commits, branch, status] = await Promise.all([
        gitService.getLog(repo.fs, repo.dir),
        gitService.getCurrentBranch(repo.fs, repo.dir),
        gitService.getStatus(repo.fs, repo.dir),
      ])
      set({ commits, branch, status, loading: false })
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  selectCommit: async (repo, oid) => {
    set({ selectedCommitOid: oid, blameFilepath: null, blame: [], loading: true, error: null })
    try {
      const selectedDiff = await gitService.getCommitDiff(repo.fs, repo.dir, oid)
      set({ selectedDiff, loading: false })
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  stage: async (repo, filepath) => {
    await gitService.stageFile(repo.fs, repo.dir, filepath)
    await get().refresh(repo)
  },

  unstage: async (repo, filepath) => {
    await gitService.unstageFile(repo.fs, repo.dir, filepath)
    await get().refresh(repo)
  },

  loadUnstagedDiff: async (repo, filepath) => {
    set({ selectedCommitOid: null, blameFilepath: null, blame: [], loading: true, error: null })
    try {
      const selectedDiff = await gitService.getUnstagedDiff(repo.fs, repo.dir, filepath)
      set({ selectedDiff, loading: false })
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  loadStagedDiff: async (repo, filepath) => {
    set({ selectedCommitOid: null, blameFilepath: null, blame: [], loading: true, error: null })
    try {
      const selectedDiff = await gitService.getStagedDiff(repo.fs, repo.dir, filepath)
      set({ selectedDiff, loading: false })
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  loadBlame: async (repo, filepath) => {
    set({
      blameFilepath: filepath,
      blame: [],
      selectedCommitOid: null,
      selectedDiff: [],
      loading: true,
      error: null,
    })
    try {
      const blame = await blameService.getBlame(repo.fs, repo.dir, filepath)
      set({ blame, loading: false })
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  loadGraph: async (repo) => {
    set({ loading: true, error: null })
    try {
      const graphCommits = await gitService.getGraphLog(repo.fs, repo.dir)
      set({ graphCommits, loading: false })
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  commit: async (repo, message, author) => {
    await gitService.createCommit(repo.fs, repo.dir, { message, author })
    set({ selectedCommitOid: null, selectedDiff: [], blameFilepath: null, blame: [] })
    await get().refresh(repo)
  },

  reset: () =>
    set({
      commits: [],
      branch: undefined,
      status: emptyStatus,
      selectedCommitOid: null,
      selectedDiff: [],
      blameFilepath: null,
      blame: [],
      graphCommits: [],
      loading: false,
      error: null,
    }),
}))
