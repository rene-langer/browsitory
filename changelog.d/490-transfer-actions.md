### Internal: remote transfer actions

- Changed: `useRemoteTransferActions` takes one context object (`client`, `repoPath`, `refresh`,
  `runners`, `setState`, `snapshot`) instead of 15 positional parameters, and reads the
  repository state it needs from one latest-snapshot ref instead of five mirror refs (#490).
- Changed: the transfer lifecycle (requested, active, completing, idle) moved into the React-free
  `lib/transferSession.ts`, unit-tested with progress event sequences; the failure messages are a
  lookup table keyed by error kind with per-operation overrides. No visible change (#490).
