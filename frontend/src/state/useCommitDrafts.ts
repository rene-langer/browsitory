import { useCallback, useRef } from "react";
import type { CommitDraft } from "../components/CommitBox";

/** `App`'s per-repo commit-message drafts. */
export function useCommitDrafts() {
  // Per-repo commit-message drafts, keyed by repo path. A ref rather than state: nothing renders
  // from it — `CommitBox` reads its entry once on mount and writes through on every change — so
  // keeping it out of state avoids re-rendering the whole app on each keystroke. Entries outlive
  // their tab on purpose (reopening a repo in the same session restores its draft), and cost one
  // short string per repo ever opened.
  const commitDrafts = useRef<Record<string, CommitDraft>>({});
  const getCommitDraft = useCallback((repoPath: string) => commitDrafts.current[repoPath], []);
  const onCommitDraftChange = useCallback((repoPath: string, draft: CommitDraft) => {
    commitDrafts.current[repoPath] = draft;
  }, []);
  return { getCommitDraft, onCommitDraftChange };
}
