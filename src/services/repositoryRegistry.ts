// Persists the list of previously opened repositories — including their
// FileSystemDirectoryHandle, which IndexedDB can store directly via structured
// clone — so they survive a page reload without re-prompting the folder picker
// (though the browser may still require a permission re-grant; see
// fsaGitFs.verifyPermission).
import { get, set } from 'idb-keyval'

const STORAGE_KEY = 'browsitory:repositories'

export interface StoredRepository {
  id: string
  name: string
  handle: FileSystemDirectoryHandle
  lastOpened: number
}

function generateId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}

async function readAll(): Promise<StoredRepository[]> {
  return (await get<StoredRepository[]>(STORAGE_KEY)) ?? []
}

async function writeAll(repos: StoredRepository[]): Promise<void> {
  await set(STORAGE_KEY, repos)
}

export async function listRepositories(): Promise<StoredRepository[]> {
  const repos = await readAll()
  return [...repos].sort((a, b) => b.lastOpened - a.lastOpened)
}

export async function getRepository(id: string): Promise<StoredRepository | undefined> {
  const repos = await readAll()
  return repos.find((r) => r.id === id)
}

export async function addRepository(handle: FileSystemDirectoryHandle): Promise<StoredRepository> {
  const repos = await readAll()

  for (const repo of repos) {
    if (await handle.isSameEntry(repo.handle)) {
      const updated: StoredRepository = { ...repo, handle, lastOpened: Date.now() }
      await writeAll(repos.map((r) => (r.id === repo.id ? updated : r)))
      return updated
    }
  }

  const created: StoredRepository = {
    id: generateId(),
    name: handle.name,
    handle,
    lastOpened: Date.now(),
  }
  await writeAll([...repos, created])
  return created
}

export async function touchRepository(id: string): Promise<void> {
  const repos = await readAll()
  await writeAll(repos.map((r) => (r.id === id ? { ...r, lastOpened: Date.now() } : r)))
}

export async function removeRepository(id: string): Promise<void> {
  const repos = await readAll()
  await writeAll(repos.filter((r) => r.id !== id))
}
