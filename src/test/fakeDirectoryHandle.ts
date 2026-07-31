// Minimal in-memory stand-in for the browser's FileSystemDirectoryHandle /
// FileSystemFileHandle, used only to unit-test fsaGitFs.ts outside a browser.
// Mirrors the subset of the real API's behavior (including DOMException error
// names) that fsaGitFs.ts depends on.

export class FakeFileHandle {
  readonly kind = 'file' as const
  name: string
  private data: Uint8Array = new Uint8Array(0)

  constructor(name: string) {
    this.name = name
  }

  async getFile() {
    const data = this.data
    return {
      size: data.byteLength,
      lastModified: Date.now(),
      arrayBuffer: async () =>
        data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength),
      text: async () => new TextDecoder().decode(data),
    }
  }

  async createWritable() {
    const chunks: Uint8Array[] = []
    return {
      write: async (chunk: Uint8Array | string) => {
        chunks.push(typeof chunk === 'string' ? new TextEncoder().encode(chunk) : chunk)
      },
      close: async () => {
        const total = chunks.reduce((n, c) => n + c.length, 0)
        const merged = new Uint8Array(total)
        let offset = 0
        for (const chunk of chunks) {
          merged.set(chunk, offset)
          offset += chunk.length
        }
        this.data = merged
      },
    }
  }
}

export class FakeDirectoryHandle {
  readonly kind = 'directory' as const
  name: string
  children: Map<string, FakeFileHandle | FakeDirectoryHandle> = new Map()

  constructor(name: string) {
    this.name = name
  }

  async getDirectoryHandle(
    name: string,
    opts?: { create?: boolean }
  ): Promise<FakeDirectoryHandle> {
    const existing = this.children.get(name)
    if (existing) {
      if (existing.kind !== 'directory') {
        throw new DOMException(`${name} is not a directory`, 'TypeMismatchError')
      }
      return existing
    }
    if (!opts?.create) {
      throw new DOMException(`${name} not found`, 'NotFoundError')
    }
    const dir = new FakeDirectoryHandle(name)
    this.children.set(name, dir)
    return dir
  }

  async getFileHandle(name: string, opts?: { create?: boolean }): Promise<FakeFileHandle> {
    const existing = this.children.get(name)
    if (existing) {
      if (existing.kind !== 'file') {
        throw new DOMException(`${name} is not a file`, 'TypeMismatchError')
      }
      return existing
    }
    if (!opts?.create) {
      throw new DOMException(`${name} not found`, 'NotFoundError')
    }
    const file = new FakeFileHandle(name)
    this.children.set(name, file)
    return file
  }

  async removeEntry(name: string): Promise<void> {
    if (!this.children.has(name)) {
      throw new DOMException(`${name} not found`, 'NotFoundError')
    }
    this.children.delete(name)
  }

  async *values(): AsyncIterableIterator<FakeFileHandle | FakeDirectoryHandle> {
    for (const child of this.children.values()) yield child
  }

  async *entries(): AsyncIterableIterator<[string, FakeFileHandle | FakeDirectoryHandle]> {
    for (const entry of this.children.entries()) yield entry
  }
}

export function createFakeRoot(): FakeDirectoryHandle {
  return new FakeDirectoryHandle('root')
}
