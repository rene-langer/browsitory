// Bridges the browser's native File System Access API to the fs.promises-shaped
// object isomorphic-git expects. Implements the subset of Node's fs surface
// isomorphic-git actually calls: readFile, writeFile, unlink, readdir, mkdir,
// rmdir, stat, lstat (aliased to stat — no symlink support), rename, plus
// readlink/symlink stubs (see below — never actually invoked, but must exist).
//
// There is no MIT-licensed off-the-shelf bridge for this (the closest option,
// ZenFS's WebAccess backend, is LGPL-3.0 and was ruled out on license grounds),
// so this is hand-written against the spec.
//
// IMPORTANT: isomorphic-git's internal `bindFs()` unconditionally does
// `fs[command].bind(fs)` for every command in its fixed list — including
// `readlink` and `symlink` — with no existence check. Omitting either of
// those methods here doesn't just disable symlink support, it throws
// "Cannot read properties of undefined (reading 'bind')" at construction
// time for every single isomorphic-git call, against every repo, always.
// This was caught the hard way (real-browser testing against a real repo,
// something this project's test suite couldn't exercise because git.test.ts
// uses Node's real fs — which has these methods — not this adapter). Any
// future trim of this file's surface must keep both, even though `stat`/
// `lstat` never report `isSymbolicLink() === true` so they're never actually
// called in practice.

export interface GitFsStat {
  isFile: () => boolean
  isDirectory: () => boolean
  isSymbolicLink: () => boolean
  size: number
  mtimeMs: number
  // isomorphic-git's normalizeStats() falls back from ctimeSeconds/ctimeMs to
  // ctime (a Date, via .valueOf()) if neither is present — the File System
  // Access API only exposes a single lastModified timestamp per file (no
  // separate change-time), so ctimeMs is set equal to mtimeMs. Omitting this
  // entirely crashes normalizeStats on every git.add() with "Cannot read
  // properties of undefined (reading 'valueOf')" — caught via real-browser
  // testing, not the test suite (see the file-level readlink/symlink note;
  // same root cause pattern: an fs.Stats field isomorphic-git assumes exists
  // that Node's real fs.Stats always has but this hand-rolled one didn't).
  ctimeMs: number
  mode: number
  ino: number
  uid: number
  gid: number
  dev: number
}

export interface GitFs {
  promises: {
    readFile: (
      filepath: string,
      options?: { encoding?: string } | string
    ) => Promise<Uint8Array | string>
    writeFile: (filepath: string, data: Uint8Array | string) => Promise<void>
    unlink: (filepath: string) => Promise<void>
    readdir: (filepath: string) => Promise<string[]>
    mkdir: (filepath: string) => Promise<void>
    rmdir: (filepath: string) => Promise<void>
    stat: (filepath: string) => Promise<GitFsStat>
    lstat: (filepath: string) => Promise<GitFsStat>
    rename: (oldPath: string, newPath: string) => Promise<void>
    readlink: (filepath: string) => Promise<string>
    symlink: (target: string, filepath: string) => Promise<void>
  }
}

function nodeError(code: string, message: string): NodeJS.ErrnoException {
  const err = new Error(message) as NodeJS.ErrnoException
  err.code = code
  return err
}

// isomorphic-git's WORKDIR walker builds paths as `${dir}/${entry._fullpath}`,
// and the root entry's _fullpath is the literal string '.' (its own POSIX-
// relative-path convention for "current directory") — so with dir === '/',
// real calls like `lstat('//.')` happen. `.` segments must be treated as a
// no-op (stay at the current directory), not a literal path component to
// look up — omitting this filter throws ENOENT on lstat('.') for every
// statusMatrix()/walk() call. Caught via real-browser testing.
function splitPath(filepath: string): string[] {
  return filepath.split('/').filter((segment) => segment !== '' && segment !== '.')
}

async function getDirHandle(
  root: FileSystemDirectoryHandle,
  segments: string[],
  create: boolean
): Promise<FileSystemDirectoryHandle> {
  let current = root
  for (const segment of segments) {
    try {
      current = await current.getDirectoryHandle(segment, { create })
    } catch (err) {
      const name = (err as DOMException).name
      if (name === 'NotFoundError') {
        throw nodeError('ENOENT', `ENOENT: no such directory, '${segment}'`)
      }
      if (name === 'TypeMismatchError') {
        throw nodeError('ENOTDIR', `ENOTDIR: not a directory, '${segment}'`)
      }
      throw err
    }
  }
  return current
}

async function resolveParent(
  root: FileSystemDirectoryHandle,
  filepath: string
): Promise<{ parent: FileSystemDirectoryHandle; name: string }> {
  const segments = splitPath(filepath)
  if (segments.length === 0) {
    throw nodeError('EINVAL', 'EINVAL: cannot operate on the repository root as a file')
  }
  const name = segments[segments.length - 1]
  const parent = await getDirHandle(root, segments.slice(0, -1), false)
  return { parent, name }
}

async function resolveHandle(
  root: FileSystemDirectoryHandle,
  filepath: string
): Promise<FileSystemFileHandle | FileSystemDirectoryHandle> {
  const segments = splitPath(filepath)
  if (segments.length === 0) return root
  const { parent, name } = await resolveParent(root, filepath)
  try {
    return await parent.getFileHandle(name)
  } catch (err) {
    const errName = (err as DOMException).name
    if (errName === 'TypeMismatchError') {
      return parent.getDirectoryHandle(name)
    }
    if (errName === 'NotFoundError') {
      throw nodeError('ENOENT', `ENOENT: no such file or directory, '${filepath}'`)
    }
    throw err
  }
}

async function toStat(handle: FileSystemFileHandle | FileSystemDirectoryHandle) {
  const isFile = handle.kind === 'file'
  let size = 0
  let mtimeMs = Date.now()
  if (isFile) {
    const file = await (handle as FileSystemFileHandle).getFile()
    size = file.size
    mtimeMs = file.lastModified
  }
  const stat: GitFsStat = {
    isFile: () => isFile,
    isDirectory: () => !isFile,
    isSymbolicLink: () => false,
    size,
    mtimeMs,
    ctimeMs: mtimeMs,
    mode: isFile ? 0o100644 : 0o40755,
    ino: 0,
    uid: 0,
    gid: 0,
    dev: 0,
  }
  return stat
}

/** Creates all missing directory segments. Idempotent — safe to call on existing paths. */
async function mkdir(root: FileSystemDirectoryHandle, filepath: string): Promise<void> {
  const segments = splitPath(filepath)
  let current = root
  for (const segment of segments) {
    try {
      current = await current.getDirectoryHandle(segment, { create: true })
    } catch (err) {
      if ((err as DOMException).name === 'TypeMismatchError') {
        throw nodeError('EEXIST', `EEXIST: file already exists, '${segment}'`)
      }
      throw err
    }
  }
}

export function createFsaGitFs(root: FileSystemDirectoryHandle): GitFs {
  async function readFile(
    filepath: string,
    options?: { encoding?: string } | string
  ): Promise<Uint8Array | string> {
    const handle = await resolveHandle(root, filepath)
    if (handle.kind !== 'file') {
      throw nodeError('EISDIR', `EISDIR: illegal operation on a directory, '${filepath}'`)
    }
    const file = await (handle as FileSystemFileHandle).getFile()
    const buffer = new Uint8Array(await file.arrayBuffer())
    const encoding = typeof options === 'string' ? options : options?.encoding
    if (encoding === 'utf8') return new TextDecoder().decode(buffer)
    return buffer
  }

  async function writeFile(filepath: string, data: Uint8Array | string): Promise<void> {
    // Defensive: ensure the parent directory exists even if the caller didn't
    // mkdir it first. Cheap and idempotent given mkdir's create:true semantics.
    const dirSegments = splitPath(filepath).slice(0, -1)
    if (dirSegments.length > 0) await mkdir(root, '/' + dirSegments.join('/'))
    const { parent, name } = await resolveParent(root, filepath)
    const handle = await parent.getFileHandle(name, { create: true })
    const writable = await handle.createWritable()
    // FileSystemWriteChunkType is narrower than Uint8Array<ArrayBufferLike> (it
    // excludes SharedArrayBuffer-backed views); isomorphic-git only ever gives us
    // plain ArrayBuffer-backed data here.
    await writable.write(data as FileSystemWriteChunkType)
    await writable.close()
  }

  async function unlink(filepath: string): Promise<void> {
    const { parent, name } = await resolveParent(root, filepath)
    try {
      await parent.removeEntry(name)
    } catch (err) {
      if ((err as DOMException).name === 'NotFoundError') {
        throw nodeError('ENOENT', `ENOENT: no such file or directory, '${filepath}'`)
      }
      throw err
    }
  }

  async function rmdir(filepath: string): Promise<void> {
    const { parent, name } = await resolveParent(root, filepath)
    try {
      await parent.removeEntry(name, { recursive: false })
    } catch (err) {
      if ((err as DOMException).name === 'NotFoundError') {
        throw nodeError('ENOENT', `ENOENT: no such file or directory, '${filepath}'`)
      }
      throw err
    }
  }

  async function readdir(filepath: string): Promise<string[]> {
    const segments = splitPath(filepath)
    const dirHandle = await getDirHandle(root, segments, false)
    const names: string[] = []
    for await (const entry of dirHandle.values()) {
      names.push(entry.name)
    }
    return names
  }

  async function stat(filepath: string): Promise<GitFsStat> {
    const handle = await resolveHandle(root, filepath)
    return toStat(handle)
  }

  async function rename(oldPath: string, newPath: string): Promise<void> {
    // Reads the whole file into memory — fine for typical git object/lock file
    // sizes at MVP scale, but not ideal for very large blobs.
    const data = await readFile(oldPath)
    await writeFile(newPath, data)
    await unlink(oldPath)
  }

  // Never actually invoked in practice — stat()/lstat() never report
  // isSymbolicLink() as true, so isomorphic-git has no reason to call these —
  // but they must exist as functions or isomorphic-git's fs binding step
  // crashes immediately for every operation. See the file-level comment.
  async function readlink(filepath: string): Promise<string> {
    throw nodeError('ENOSYS', `ENOSYS: symlinks are not supported, readlink '${filepath}'`)
  }

  async function symlink(_target: string, filepath: string): Promise<void> {
    throw nodeError('ENOSYS', `ENOSYS: symlinks are not supported, symlink '${filepath}'`)
  }

  return {
    promises: {
      readFile,
      writeFile,
      unlink,
      readdir,
      mkdir: (filepath: string) => mkdir(root, filepath),
      rmdir,
      stat,
      lstat: stat,
      rename,
      readlink,
      symlink,
    },
  }
}

export type PermissionMode = 'read' | 'readwrite'

/**
 * Re-checks (and if needed, re-prompts for) permission on a directory handle
 * restored from IndexedDB — the browser can silently drop write permission
 * across sessions even though the handle itself persists.
 */
export async function verifyPermission(
  handle: FileSystemDirectoryHandle,
  mode: PermissionMode = 'readwrite'
): Promise<boolean> {
  const opts: FileSystemHandlePermissionDescriptor = { mode }
  if ((await handle.queryPermission(opts)) === 'granted') return true
  if ((await handle.requestPermission(opts)) === 'granted') return true
  return false
}
