import { useEffect, useRef } from "react";
import type { UseOpenReposResult } from "./useOpenRepos";

/**
 * E2E builds only (`VITE_E2E_REPO_PATH`): starts the session with exactly the fixture repo open.
 */
export function useE2eAutoOpen(openRepos: UseOpenReposResult, openRepoTab: (path: string) => Promise<void>) {
  // E2E-only auto-open: `RepoPicker`'s native folder dialog can't be driven through WebDriver,
  // so the E2E build points at a fixture repo via this Vite env var instead — opened as this
  // session's first tab. Statically absent from a normal production build unless
  // VITE_E2E_REPO_PATH is set at build time.
  // Guards the E2E reconciliation effect below so it only forces a deterministic starting repo
  // once, at launch — not on every later `openRepos.openRepos` change, which would otherwise
  // also fire for (and undo) a tab the test itself opens afterward.
  // The workspace E2E uses a one-shot local-storage marker before restarting the real app so
  // that one launch can prove persisted workspace grouping is restored. Normal E2E launches
  // still reconcile to the single fixture repo, preserving cross-test isolation.
  const e2eReconciledRef = useRef(false);
  useEffect(() => {
    if (
      typeof import.meta.env.VITE_E2E_REPO_PATH === "string" &&
      window.localStorage.getItem("browsitory-e2e-restore-open-repos-once") === "true"
    ) {
      e2eReconciledRef.current = true;
      window.localStorage.removeItem("browsitory-e2e-restore-open-repos-once");
    }
  }, []);
  useEffect(() => {
    // Wait for the persisted-tab restore to settle first: on the mount pass `openRepos` is still
    // the empty initial value, so without this the fixture would be opened concurrently with
    // `listOpenRepos`, and whichever promise resolved last would clobber the other's tab list.
    if (openRepos.loading || e2eReconciledRef.current) return;
    const autoOpenPath = import.meta.env.VITE_E2E_REPO_PATH;
    if (typeof autoOpenPath !== "string" || autoOpenPath.length === 0) {
      e2eReconciledRef.current = true;
      return;
    }
    // E2E fixtures need a deterministic single starting repo, not whatever tabs happen to be
    // persisted from a prior run — e.g. a previous spec that crashed after opening a second tab
    // but before closing it would otherwise leak a two-tab restore into the next spec's launch.
    // So this bypasses the persisted-tab restore entirely rather than only firing when it
    // happened to already be empty: close anything that isn't the fixture first (the effect
    // re-runs once that settles, since `openRepos.openRepos` is a dependency), then open the
    // fixture if it isn't already the sole tab. Once exactly the fixture is open, mark
    // reconciliation done so this stops touching tabs — the spec itself opens more afterward.
    const stale = openRepos.openRepos.filter((repo) => repo.path !== autoOpenPath);
    if (stale.length > 0) {
      stale.forEach((repo) => openRepos.closeRepo(repo.path));
      return;
    }
    if (openRepos.openRepos.length === 0) {
      void openRepoTab(autoOpenPath);
      return;
    }
    e2eReconciledRef.current = true;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openRepos.loading, openRepos.openRepos]);
}
