# Task 1.E.04: `DiffPane` and `CommitBox` components

## Goal

The pane driven by `HistoryList`'s selection. "Uncommitted Changes" selected: file list from
`status`, stage/unstage buttons per file, a diff for whichever file is clicked, and a
`CommitBox` (message + Cmd/Ctrl+Enter to commit, disabled with nothing staged). A commit
selected: that commit's changed files (read-only, no staging controls, no `CommitBox`).

## Depends on

1.D.01 (`RepoClient`), 1.D.02 (`SelectedRow` type and the `stageFile`/`unstageFile`/`commit`
action shapes this component's props mirror), 1.E.02 (`DiffView`, reused by both branches here).

## Interfaces produced

`frontend/src/components/DiffPane.tsx`:
```tsx
export function DiffPane({
  client,
  selectedRow,
  status,
  onStageFile,
  onUnstageFile,
  onCommit,
}: {
  client: RepoClient;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onCommit: (message: string) => void;
}) {
  // ...
}
```
`frontend/src/components/CommitBox.tsx`:
```tsx
export function CommitBox({
  onCommit,
  disabled,
}: {
  onCommit: (message: string) => void;
  disabled: boolean;
}) {
  // ...
}
```
Task 1.F.01 renders `<DiffPane client={...} selectedRow={appState.state.selectedRow}
status={appState.state.status} onStageFile={appState.stageFile}
onUnstageFile={appState.unstageFile} onCommit={appState.commit} />`.

## Implementation notes

`DiffPane` branches on `selectedRow` and delegates to one of two internal (not exported)
components living in the same file — they're tightly coupled to `DiffPane` and never used
independently:

```tsx
import { useEffect, useState } from "react";
import type { DiffHunk, RepoClient, StatusEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { CommitBox } from "./CommitBox";
import { DiffView } from "./DiffView";

export function DiffPane({
  client,
  selectedRow,
  status,
  onStageFile,
  onUnstageFile,
  onCommit,
}: {
  client: RepoClient;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onCommit: (message: string) => void;
}) {
  if (selectedRow === "uncommitted") {
    return (
      <UncommittedDiffPane
        client={client}
        status={status}
        onStageFile={onStageFile}
        onUnstageFile={onUnstageFile}
        onCommit={onCommit}
      />
    );
  }
  return <CommitDiffPane client={client} commitId={selectedRow.commitId} />;
}

function UncommittedDiffPane({
  client,
  status,
  onStageFile,
  onUnstageFile,
  onCommit,
}: {
  client: RepoClient;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onCommit: (message: string) => void;
}) {
  const [selected, setSelected] = useState<{ path: string; staged: boolean } | null>(null);
  const [hunks, setHunks] = useState<DiffHunk[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (selected === null) {
      setHunks([]);
      return;
    }
    client
      .getWorkingDiff(selected.path, selected.staged)
      .then((next) => {
        setHunks(next);
        setError(null);
      })
      .catch((err: unknown) => setError(String(err)));
  }, [client, selected]);

  const stagedCount = status.filter((entry) => entry.staged).length;

  return (
    <div>
      <ul>
        {status.map((entry) => (
          <li key={`${entry.staged}:${entry.path}`}>
            <button onClick={() => setSelected({ path: entry.path, staged: entry.staged })}>
              {entry.path} ({entry.kind})
            </button>
            {entry.staged ? (
              <button onClick={() => onUnstageFile(entry.path)}>Unstage</button>
            ) : (
              <button onClick={() => onStageFile(entry.path)}>Stage</button>
            )}
          </li>
        ))}
      </ul>
      {error !== null ? <p role="alert">{error}</p> : <DiffView hunks={hunks} />}
      <CommitBox onCommit={onCommit} disabled={stagedCount === 0} />
    </div>
  );
}

function CommitDiffPane({ client, commitId }: { client: RepoClient; commitId: string }) {
  const [files, setFiles] = useState<string[]>([]);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [hunks, setHunks] = useState<DiffHunk[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setSelectedPath(null);
    setHunks([]);
    client
      .getCommitFiles(commitId)
      .then((next) => {
        setFiles(next);
        setError(null);
      })
      .catch((err: unknown) => setError(String(err)));
  }, [client, commitId]);

  useEffect(() => {
    if (selectedPath === null) {
      setHunks([]);
      return;
    }
    client
      .getCommitDiff(commitId, selectedPath)
      .then((next) => {
        setHunks(next);
        setError(null);
      })
      .catch((err: unknown) => setError(String(err)));
  }, [client, commitId, selectedPath]);

  return (
    <div>
      <ul>
        {files.map((path) => (
          <li key={path}>
            <button onClick={() => setSelectedPath(path)}>{path}</button>
          </li>
        ))}
      </ul>
      {error !== null ? <p role="alert">{error}</p> : <DiffView hunks={hunks} />}
    </div>
  );
}
```
A path with both a staged and an unstaged change (Phase 0's `status()` returns two separate
`StatusEntry` rows for that case) renders as two separate list items here too — `selected` is
keyed on `{ path, staged }`, not `path` alone, so clicking the staged row diffs HEAD-vs-index
and clicking the unstaged row diffs index-vs-workdir for the same path, independently.

`CommitBox`:
```tsx
import { useState, type KeyboardEvent } from "react";

export function CommitBox({
  onCommit,
  disabled,
}: {
  onCommit: (message: string) => void;
  disabled: boolean;
}) {
  const [message, setMessage] = useState("");

  const commitIfReady = () => {
    if (disabled || message.trim() === "") {
      return;
    }
    onCommit(message);
    setMessage("");
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      commitIfReady();
    }
  };

  return (
    <div>
      <textarea
        value={message}
        onChange={(event) => setMessage(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Commit message"
      />
      <button onClick={commitIfReady} disabled={disabled || message.trim() === ""}>
        Commit
      </button>
    </div>
  );
}
```
`disabled` (nothing staged) and an empty/whitespace-only message both independently disable
committing — either one being true blocks both the button and Cmd/Ctrl+Enter.

## TDD requirement

`frontend/src/components/CommitBox.test.tsx` (new file, test this component in isolation first
since `DiffPane` composes it):

- `Commit button is disabled when disabled=true`: render `disabled={true}`, assert
  `screen.getByText("Commit")` has the `disabled` attribute.
- `Commit button is disabled with an empty message even when disabled=false`: render
  `disabled={false}`, assert the button starts disabled (no text typed yet).
- `typing a message and clicking Commit calls onCommit and clears the textarea`: render
  `disabled={false}`, `fireEvent.change` the textarea to `"my message"`, click "Commit". Assert
  the `onCommit` spy was called with `"my message"`, and the textarea's value is now `""`.
- `Cmd/Ctrl+Enter in the textarea commits`: type a message, `fireEvent.keyDown(textarea, { key:
  "Enter", metaKey: true })`. Assert `onCommit` was called with that message.
- `Cmd/Ctrl+Enter does nothing when disabled`: render `disabled={true}`, type a message anyway
  (the textarea itself isn't disabled, only committing is — matches real editors where you can
  still type while an action is blocked), fire the Cmd+Enter keydown, assert `onCommit` was
  never called.

`frontend/src/components/DiffPane.test.tsx` (new file), fake `RepoClient` implementing only the
methods this component calls (`getWorkingDiff`, `getCommitFiles`, `getCommitDiff` — the other
`RepoClient` methods can throw `new Error("not used in this test")` if called, to catch
accidental extra calls):

- `uncommitted: renders a Stage button for unstaged entries and Unstage for staged ones`:
  `status = [{ path: "a.txt", staged: false, kind: "Modified" }, { path: "b.txt", staged: true,
  kind: "New" }]`. Assert both `screen.getByText("Stage")` (for `a.txt`'s row) and
  `screen.getByText("Unstage")` (for `b.txt`'s row) render.
- `uncommitted: clicking Stage calls onStageFile with that path`: click `a.txt`'s "Stage"
  button, assert the `onStageFile` spy was called with `"a.txt"`.
- `uncommitted: clicking a file fetches and renders its working diff`: fake client's
  `getWorkingDiff` resolves to one hunk with a line whose `content` is `"changed line"`. Click
  `a.txt`'s path button, assert `await screen.findByText(/changed line/)` renders, and that
  `getWorkingDiff` was called with `("a.txt", false)` (matching `a.txt`'s `staged: false`).
- `uncommitted: CommitBox is disabled when nothing is staged`: `status` with only unstaged
  entries. Assert `screen.getByText("Commit")` is disabled.
- `uncommitted: CommitBox is enabled when something is staged`: `status` with one staged entry.
  Assert `screen.getByText("Commit")` is not disabled (still requires a non-empty message to
  actually submit — that's `CommitBox`'s own concern, already covered above).
- `commit: renders the commit's changed files and their diff on click`: fake client's
  `getCommitFiles` resolves to `["src/main.rs"]`, `getCommitDiff` resolves to one hunk with a
  line `"fn main() {}"`. Render with `selectedRow={{ commitId: "abc123" }}`. Assert
  `await screen.findByText("src/main.rs")` renders, click it, assert
  `await screen.findByText(/fn main/)` renders and `getCommitDiff` was called with
  `("abc123", "src/main.rs")`.
- `commit: no CommitBox or stage/unstage buttons render`: same render as above, assert
  `screen.queryByText("Commit")` and `screen.queryByText("Stage")` are both `null`.

Write `CommitBox.test.tsx` and `DiffPane.test.tsx` first (both modules don't exist), confirm they
fail to compile, then implement `CommitBox.tsx` and `DiffPane.tsx` per the code above and re-run
until green.

## Acceptance criteria

- [ ] `pnpm test -- --run` passes (5 `CommitBox` tests + 7 `DiffPane` tests + all existing tests
      still passing).
- [ ] `pnpm build` succeeds.
- [ ] `pnpm lint` clean.
- [ ] Commit: `git add frontend/src/components/DiffPane.tsx frontend/src/components/DiffPane.test.tsx frontend/src/components/CommitBox.tsx frontend/src/components/CommitBox.test.tsx && git commit -m "feat(frontend): add DiffPane and CommitBox components"`.

## Out of scope

Hunk-level or line-level staging (whole-file only, per the design spec). Diffing two arbitrary
selected files side by side. Multi-file selection / bulk stage-all / unstage-all buttons. Syntax
highlighting (inherited from `DiffView`'s own out-of-scope list).
