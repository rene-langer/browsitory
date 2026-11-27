import { useCallback, useEffect, useRef } from "react";
import type { CommitDraft } from "../components/CommitBox";
import { readCommitDraft, writeCommitDraft } from "../lib/commitDraftStorage";

/** How long a draft waits after a change before it is stored. */
const STORE_DELAY_MS = 300;

/** `App`'s per-repo commit-message drafts. */
export function useCommitDrafts() {
  // Per-repo commit-message drafts, keyed by repo path. A ref rather than state: nothing renders
  // from it — `CommitBox` reads its entry once on mount and writes through on every change — so
  // keeping it out of state avoids re-rendering the whole app on each keystroke. Entries outlive
  // their tab on purpose (reopening a repo in the same session restores its draft).
  const commitDrafts = useRef<Record<string, CommitDraft>>({});
  // Each draft is also stored in web storage (#1143), shortly after it stops changing, and at
  // once when the page goes away or this unmounts (the window-level Reload), so a quit, a closed
  // VSCode panel or a Reload brings it back. A repository's first read takes the stored one.
  const unstored = useRef(new Set<string>());
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const store = useCallback(() => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
    for (const repoPath of unstored.current) {
      const draft = commitDrafts.current[repoPath];
      if (draft !== undefined) writeCommitDraft(repoPath, draft);
    }
    unstored.current.clear();
  }, []);
  useEffect(() => {
    window.addEventListener("pagehide", store);
    return () => {
      window.removeEventListener("pagehide", store);
      store();
    };
  }, [store]);

  const getCommitDraft = useCallback((repoPath: string) => {
    if (!(repoPath in commitDrafts.current)) {
      const stored = readCommitDraft(repoPath);
      if (stored !== undefined) commitDrafts.current[repoPath] = stored;
    }
    return commitDrafts.current[repoPath];
  }, []);
  const onCommitDraftChange = useCallback(
    (repoPath: string, draft: CommitDraft) => {
      commitDrafts.current[repoPath] = draft;
      unstored.current.add(repoPath);
      if (timer.current !== null) clearTimeout(timer.current);
      timer.current = setTimeout(store, STORE_DELAY_MS);
    },
    [store],
  );
  return { getCommitDraft, onCommitDraftChange };
}
