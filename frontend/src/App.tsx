import { DiffPane } from "./components/DiffPane";
import { HistoryList } from "./components/HistoryList";
import { RepoPicker } from "./components/RepoPicker";
import { tauriRepoClient } from "./ipc/tauriRepoClient";
import { useAppState } from "./state/useAppState";

export default function App() {
  const appState = useAppState(tauriRepoClient);

  if (appState.state.repoPath === null) {
    return (
      <main>
        <h1>Browsitory</h1>
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
