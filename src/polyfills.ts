// isomorphic-git (and its own dependency readable-stream) use Node's global
// `Buffer` extensively for git object/pack-file parsing — e.g. reading a real
// cloned repo's packed objects, not just the loose objects a fresh `git.init`
// produces. Vite doesn't polyfill Node globals automatically (unlike some
// older webpack setups), so without this every such operation throws
// "Buffer is not defined" — only surfaces against a repo with real pack
// files, which is why this was missed until real-browser testing against an
// actual cloned repo. `buffer` (MIT) is already a transitive dependency of
// isomorphic-git via readable-stream; this just wires it up as the global
// Node code expects.
import { Buffer } from 'buffer'

declare global {
  interface Window {
    Buffer: typeof Buffer
  }
}

if (typeof window !== 'undefined' && !window.Buffer) {
  window.Buffer = Buffer
}
