### Error kinds

- Fixed: A pull you cancel says "Pull cancelled." again, not "the transfer was cancelled" (#496).
- Fixed: When a repository's worker has stopped, starting a fetch, push or pull says to close
  and reopen the repository, as every other action does, instead of "worker thread stopped"
  (#496).
- Fixed: In the VSCode extension, the errors of a pull, a fetch or push that can't start, an
  unreadable conflict and the repository and workspace pickers no longer start with "Error: "
  (#496).
- Internal: A failed request now answers `{ kind, message }` (`repo_service::wire::ErrorDto`,
  generated to TypeScript) on both transports: the Tauri command's error, and the sidecar's
  JSON-RPC error `data`. `RepoClient` methods reject with `RepoError`, and the frontend switches
  on its `kind` instead of searching the message text. Removed the checks that matched no
  backend text: a direct fetch's "Fetch failed" rejection (gone since fetches report failures
  only through their terminal event), the "Transport failed", "failed to read working diff" and
  "permission denied" hints, and the "missing credential" and "SSH agent failure" rejections
  (#496).
