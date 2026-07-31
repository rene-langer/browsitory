import { create } from 'zustand'
import { createFsaGitFs, verifyPermission, type GitFs } from '@services/fsaGitFs'
import { openRepository } from '@services/git'
import * as registry from '@services/repositoryRegistry'
import type { StoredRepository } from '@services/repositoryRegistry'

export interface OpenRepository {
  id: string
  name: string
  fs: GitFs
  dir: string
}

interface RepositoryState {
  repositories: StoredRepository[]
  currentRepo: OpenRepository | null
  loading: boolean
  error: string | null

  loadRepositories: () => Promise<void>
  /** Opens the native folder picker. Returns the new repository's id, or null if cancelled/failed. */
  openRepositoryPicker: () => Promise<string | null>
  openRepositoryById: (id: string) => Promise<void>
  removeRepository: (id: string) => Promise<void>
  clearError: () => void
}

async function openHandle(handle: FileSystemDirectoryHandle): Promise<GitFs> {
  const granted = await verifyPermission(handle, 'readwrite')
  if (!granted) throw new Error('Permission to access the folder was denied.')
  const fs = createFsaGitFs(handle)
  await openRepository(fs, '/')
  return fs
}

export const useRepositoryStore = create<RepositoryState>((set) => ({
  repositories: [],
  currentRepo: null,
  loading: false,
  error: null,

  loadRepositories: async () => {
    const repositories = await registry.listRepositories()
    set({ repositories })
  },

  openRepositoryPicker: async () => {
    if (typeof window === 'undefined' || !('showDirectoryPicker' in window)) {
      set({
        error:
          'This browser does not support the File System Access API. Use Chrome, Edge, or another Chromium-based browser.',
      })
      return null
    }
    set({ loading: true, error: null })
    try {
      const handle = await window.showDirectoryPicker({ mode: 'readwrite' })
      const fs = await openHandle(handle)
      const stored = await registry.addRepository(handle)
      const repositories = await registry.listRepositories()
      set({
        repositories,
        currentRepo: { id: stored.id, name: stored.name, fs, dir: '/' },
        loading: false,
      })
      return stored.id
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') {
        set({ loading: false }) // user cancelled the picker — not an error
        return null
      }
      set({ loading: false, error: err instanceof Error ? err.message : String(err) })
      return null
    }
  },

  openRepositoryById: async (id) => {
    set({ loading: true, error: null })
    try {
      const stored = await registry.listRepositories().then((repos) => repos.find((r) => r.id === id))
      if (!stored) throw new Error('Repository not found. It may have been removed.')

      const fs = await openHandle(stored.handle)
      await registry.touchRepository(id)

      set({ currentRepo: { id: stored.id, name: stored.name, fs, dir: '/' }, loading: false })
    } catch (err) {
      set({ loading: false, error: err instanceof Error ? err.message : String(err) })
    }
  },

  removeRepository: async (id) => {
    await registry.removeRepository(id)
    const repositories = await registry.listRepositories()
    set((state) => ({
      repositories,
      currentRepo: state.currentRepo?.id === id ? null : state.currentRepo,
    }))
  },

  clearError: () => set({ error: null }),
}))
