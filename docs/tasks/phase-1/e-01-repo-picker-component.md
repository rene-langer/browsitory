# Task 1.E.01: `RepoPicker` component

## Goal

Show a recent-repos list plus an "Open Folder" button when no repo is open. This is what a user
sees on first launch, instead of Phase 0's always-empty status view.

## Depends on

1.D.01 (`RepoClient.pickRepoFolder`/`listRecentRepos`), 1.D.02 (only for the shape of the
`onOpenRepo` callback this component receives — it doesn't call `useAppState` itself).

## Interfaces produced

`frontend/src/components/RepoPicker.tsx`:
```tsx
export function RepoPicker({
  client,
  onOpenRepo,
}: {
  client: RepoClient;
  onOpenRepo: (path: string) => void;
}) {
  // ...
}
```
Same prop shape as the existing `StatusView` (`client` + callback/data props, no direct
`useAppState` dependency — keeps this component independently testable with a fake client, and
reusable if App.tsx's wiring changes later). Task 1.F.01 renders `<RepoPicker client={...}
onOpenRepo={appState.openRepo} />` when `appState.state.repoPath === null`.

## Implementation notes

```tsx
import { useEffect, useState } from "react";
import type { RepoClient } from "../ipc/RepoClient";

export function RepoPicker({
  client,
  onOpenRepo,
}: {
  client: RepoClient;
  onOpenRepo: (path: string) => void;
}) {
  const [recentRepos, setRecentRepos] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    client
      .listRecentRepos()
      .then(setRecentRepos)
      .catch((err: unknown) => setError(String(err)));
  }, [client]);

  const handleOpenFolder = () => {
    client
      .pickRepoFolder()
      .then((path) => {
        if (path !== null) {
          onOpenRepo(path);
        }
      })
      .catch((err: unknown) => setError(String(err)));
  };

  return (
    <div>
      <button onClick={handleOpenFolder}>Open Folder</button>
      {error !== null && <p role="alert">{error}</p>}
      {recentRepos.length === 0 ? (
        <p>No recent repositories</p>
      ) : (
        <ul>
          {recentRepos.map((path) => (
            <li key={path}>
              <button onClick={() => onOpenRepo(path)}>{path}</button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
```
`pickRepoFolder` resolving to `null` means the user cancelled the native dialog — silently do
nothing (not an error). Clicking a recent-repo entry calls `onOpenRepo` directly with that
already-known path — no need to go through `pickRepoFolder` again.

## TDD requirement

`frontend/src/components/RepoPicker.test.tsx` (new file), same `fakeClient`-object-literal
pattern as `StatusView.test.tsx`:

- `renders each recent repo and opens it on click`: fake client whose `listRecentRepos` resolves
  to `["/repo/a", "/repo/b"]`. Render with an `onOpenRepo` spy (`vi.fn()`). Assert
  `await screen.findByText("/repo/a")` and `"/repo/b"` both render. Click `/repo/a`'s button
  (`screen.getByText("/repo/a")`), assert the spy was called with `"/repo/a"` exactly once.
- `shows a message when there are no recent repos`: fake client's `listRecentRepos` resolves to
  `[]`. Assert `await screen.findByText("No recent repositories")` renders.
- `Open Folder button opens the picked path`: fake client's `pickRepoFolder` resolves to
  `"/picked/repo"`. Click the "Open Folder" button (`screen.getByText("Open Folder")`), assert
  (via `await waitFor` or by awaiting a microtask) the `onOpenRepo` spy was called with
  `"/picked/repo"`.
- `Open Folder button does nothing when the dialog is cancelled`: fake client's `pickRepoFolder`
  resolves to `null`. Click "Open Folder", assert the `onOpenRepo` spy was never called (after
  awaiting a microtask so the promise has settled).
- `renders the error instead of the recent list when listRecentRepos rejects`: fake client's
  `listRecentRepos` rejects with `new Error("config unreadable")`. Assert
  `(await screen.findByRole("alert")).toHaveTextContent("config unreadable")`.

Write these five tests first (module doesn't exist), confirm they fail, then implement
`RepoPicker.tsx` per the code above and re-run until green.

## Acceptance criteria

- [ ] `pnpm test -- --run` passes (5 new tests + all existing tests still passing).
- [ ] `pnpm build` succeeds.
- [ ] `pnpm lint` clean (confirms `RepoPicker.tsx` doesn't accidentally import `@tauri-apps/api`
      — it shouldn't, since it only touches `client`/props, but the `no-restricted-imports` rule
      catching a slip here is exactly what it's for).
- [ ] Commit: `git add frontend/src/components/RepoPicker.tsx frontend/src/components/RepoPicker.test.tsx && git commit -m "feat(frontend): add RepoPicker component"`.

## Out of scope

Removing entries from the recent list from this UI. Drag-and-drop folder opening. Validating a
recent path still exists before showing it (a stale entry just fails normally when clicked, via
`onOpenRepo` → `useAppState.openRepo` → `RepoClient.openRepo` rejecting, surfaced the same way
any other open failure is — Task 1.F.01's concern, not this component's).
