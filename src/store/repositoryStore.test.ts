import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@services/fsaGitFs', () => ({
  createFsaGitFs: vi.fn(() => ({ promises: {} })),
  verifyPermission: vi.fn(async () => true),
}))

vi.mock('@services/git', () => ({
  openRepository: vi.fn(async () => undefined),
}))

vi.mock('@services/repositoryRegistry', () => ({
  listRepositories: vi.fn(async () => []),
  addRepository: vi.fn(async (handle: { name: string }) => ({
    id: 'repo-1',
    name: handle.name,
    handle,
    lastOpened: Date.now(),
  })),
  touchRepository: vi.fn(async () => undefined),
  removeRepository: vi.fn(async () => undefined),
}))

import { useRepositoryStore } from './repositoryStore'
import * as registry from '@services/repositoryRegistry'
import { verifyPermission } from '@services/fsaGitFs'
import { openRepository } from '@services/git'

function makeFakeHandle(name = 'my-repo') {
  return { name } as unknown as FileSystemDirectoryHandle
}

beforeEach(() => {
  useRepositoryStore.setState({
    repositories: [],
    currentRepo: null,
    loading: false,
    error: null,
  })
  vi.clearAllMocks()
  vi.mocked(verifyPermission).mockResolvedValue(true)
  vi.mocked(openRepository).mockResolvedValue(undefined)
  vi.mocked(registry.listRepositories).mockResolvedValue([])
  vi.mocked(registry.addRepository).mockImplementation(async (handle) => ({
    id: 'repo-1',
    name: handle.name,
    handle,
    lastOpened: Date.now(),
  }))
})

describe('openRepositoryPicker', () => {
  it('opens a repo when the picker resolves a handle', async () => {
    const handle = makeFakeHandle('cool-repo')
    Object.assign(window, { showDirectoryPicker: vi.fn(async () => handle) })

    const id = await useRepositoryStore.getState().openRepositoryPicker()

    expect(id).toBe('repo-1')
    expect(useRepositoryStore.getState().currentRepo?.name).toBe('cool-repo')
    expect(useRepositoryStore.getState().error).toBeNull()
  })

  it('returns null without setting an error when the user cancels the picker', async () => {
    Object.assign(window, {
      showDirectoryPicker: vi.fn(async () => {
        throw new DOMException('cancelled', 'AbortError')
      }),
    })

    const id = await useRepositoryStore.getState().openRepositoryPicker()

    expect(id).toBeNull()
    expect(useRepositoryStore.getState().error).toBeNull()
  })

  it('sets an error when permission is denied', async () => {
    const handle = makeFakeHandle()
    Object.assign(window, { showDirectoryPicker: vi.fn(async () => handle) })
    vi.mocked(verifyPermission).mockResolvedValue(false)

    const id = await useRepositoryStore.getState().openRepositoryPicker()

    expect(id).toBeNull()
    expect(useRepositoryStore.getState().error).toMatch(/permission/i)
  })

  it('sets an error when the folder is not a git repository', async () => {
    const handle = makeFakeHandle()
    Object.assign(window, { showDirectoryPicker: vi.fn(async () => handle) })
    vi.mocked(openRepository).mockRejectedValue(new Error('No git repository found'))

    const id = await useRepositoryStore.getState().openRepositoryPicker()

    expect(id).toBeNull()
    expect(useRepositoryStore.getState().error).toMatch(/no git repository/i)
  })
})

describe('removeRepository', () => {
  it('clears currentRepo if the removed repo was the current one', async () => {
    useRepositoryStore.setState({
      currentRepo: { id: 'repo-1', name: 'repo', fs: { promises: {} } as never, dir: '/' },
    })
    vi.mocked(registry.removeRepository).mockResolvedValue(undefined)
    vi.mocked(registry.listRepositories).mockResolvedValue([])

    await useRepositoryStore.getState().removeRepository('repo-1')

    expect(useRepositoryStore.getState().currentRepo).toBeNull()
  })

  it('leaves currentRepo untouched when a different repo is removed', async () => {
    const current = { id: 'repo-1', name: 'repo', fs: { promises: {} } as never, dir: '/' }
    useRepositoryStore.setState({ currentRepo: current })
    vi.mocked(registry.removeRepository).mockResolvedValue(undefined)
    vi.mocked(registry.listRepositories).mockResolvedValue([])

    await useRepositoryStore.getState().removeRepository('some-other-id')

    expect(useRepositoryStore.getState().currentRepo).toBe(current)
  })
})
