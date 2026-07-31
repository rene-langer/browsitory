import { create } from 'zustand'
import * as gitService from '@services/git'
import type {
  BranchInfo,
  CommitInfo,
  FileDiff,
  GraphCommit,
  StashEntry,
  StatusResult,
} from '@services/git'
import * as blameService from '@services/blame'
import type { BlameLine } from '@services/blame'
import type { OpenRepository } from './repositoryStore'

interface GitState {
  commits: CommitInfo[]
  branch: string | undefined
  status: StatusResult
  branches: BranchInfo[]
  stashes: StashEntry[]
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

  // Branches — mutations set loading/error themselves (unlike stage/unstage/
  // commit above) because, unlike those, they can fail for reasons the user
  // needs to see: switching branches refuses when it would clobber
  // uncommitted changes, and deleting refuses on the current branch.
  switchBranch: (repo: OpenRepository, name: string) => Promise<void>
  createBranch: (repo: OpenRepository, name: string, startPoint?: string) => Promise<void>
  deleteBranch: (repo: OpenRepository, name: string) => Promise<void>
  renameBranch: (repo: OpenRepository, oldName: string, newName: string) => Promise<void>

  // Stash
  createStash: (repo: OpenRepository, message?: string) => Promise<void>
  applyStash: (repo: OpenRepository, refIdx?: number) => Promise<void>
  popStash: (repo: OpenRepository, refIdx?: number) => Promise<void>
  dropStash: (repo: OpenRepository, refIdx?: number) => Promise<void>

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
  branches: [],
  stashes: [],
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
      const [commits, branch, status, branches, stashes] = await Promise.all([
        gitService.getLog(repo.fs, repo.dir),
        gitService.getCurrentBranch(repo.fs, repo.dir),
        gitService.getStatus(repo.fs, repo.dir),
        gitService.listAllBranches(repo.fs, repo.dir),
        gitService.listStashes(repo.fs, repo.dir),
      ])
      set({ commits, branch, status, branches, stashes, loading: false })
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

  switchBranch: async (repo, name) => {
    set({ loading: true, error: null })
    try {
      await gitService.switchBranch(repo.fs, repo.dir, name)
      set({ selectedCommitOid: null, selectedDiff: [] })
      await get().refresh(repo)
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  createBranch: async (repo, name, startPoint) => {
    set({ loading: true, error: null })
    try {
      await gitService.createBranch(repo.fs, repo.dir, name, startPoint)
      await get().refresh(repo)
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  deleteBranch: async (repo, name) => {
    set({ loading: true, error: null })
    try {
      await gitService.deleteBranchByName(repo.fs, repo.dir, name)
      await get().refresh(repo)
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  renameBranch: async (repo, oldName, newName) => {
    set({ loading: true, error: null })
    try {
      await gitService.renameBranchTo(repo.fs, repo.dir, oldName, newName)
      await get().refresh(repo)
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  createStash: async (repo, message) => {
    set({ loading: true, error: null })
    try {
      await gitService.createStash(repo.fs, repo.dir, message)
      await get().refresh(repo)
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  applyStash: async (repo, refIdx) => {
    set({ loading: true, error: null })
    try {
      await gitService.applyStash(repo.fs, repo.dir, refIdx)
      await get().refresh(repo)
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  popStash: async (repo, refIdx) => {
    set({ loading: true, error: null })
    try {
      await gitService.popStash(repo.fs, repo.dir, refIdx)
      await get().refresh(repo)
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  dropStash: async (repo, refIdx) => {
    set({ loading: true, error: null })
    try {
      await gitService.dropStash(repo.fs, repo.dir, refIdx)
      await get().refresh(repo)
    } catch (err) {
      set({ loading: false, error: describeError(err) })
    }
  },

  reset: () =>
    set({
      commits: [],
      branch: undefined,
      status: emptyStatus,
      branches: [],
      stashes: [],
      selectedCommitOid: null,
      selectedDiff: [],
      blameFilepath: null,
      blame: [],
      graphCommits: [],
      loading: false,
      error: null,
    }),
}))
