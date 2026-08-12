import { useEffect } from "react";
import { DiffPane } from "./components/DiffPane";
import { HistoryList } from "./components/HistoryList";
import { RepoPicker } from "./components/RepoPicker";
import { tauriRepoClient } from "./ipc/tauriRepoClient";
import { useAppState } from "./state/useAppState";

export default function App() {
  const appState = useAppState(tauriRepoClient);

  // E2E-only auto-open: `RepoPicker`'s native folder dialog can't be driven through WebDriver,
  // so the E2E build points at a fixture repo via this Vite env var instead. Statically absent
  // from a normal production build unless VITE_E2E_REPO_PATH is set at build time.
  useEffect(() => {
    const autoOpenPath = import.meta.env.VITE_E2E_REPO_PATH;
    if (typeof autoOpenPath === "string" && autoOpenPath.length > 0) {
      appState.openRepo(autoOpenPath);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (appState.state.repoPath === null) {
    return (
      <main>
        <h1>Browsitory</h1>
        {/* `RepoPicker` only surfaces errors from its own `pickRepoFolder`/`listRecentRepos`
            calls; an `onOpenRepo` rejection (bad path, a stale recent-repo entry, permissions)
            lands in `useAppState`'s `state.error`, which is otherwise only rendered in the
            post-open branch below — leaving a failed open looking like nothing happened. */}
        {appState.state.error !== null && <p role="alert">{appState.state.error}</p>}
        <RepoPicker client={tauriRepoClient} onOpenRepo={appState.openRepo} />
      </main>
    );
  }

  return (
    <main>
      <h1>Browsitory</h1>
      {appState.state.error !== null && <p role="alert">{appState.state.error}</p>}
      <div className="app-layout">
        <HistoryList
          status={appState.state.status}
          log={appState.state.log}
          selectedRow={appState.state.selectedRow}
          onSelectRow={appState.selectRow}
        />
        <DiffPane
          client={tauriRepoClient}
          selectedRow={appState.state.selectedRow}
          status={appState.state.status}
          onStageFile={appState.stageFile}
          onUnstageFile={appState.unstageFile}
          onCommit={appState.commit}
        />
      </div>
    </main>
  );
}
