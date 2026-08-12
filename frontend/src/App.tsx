import { tauriRepoClient } from "./ipc/tauriRepoClient";
import { StatusView } from "./components/StatusView";

export default function App() {
  return (
    <main>
      <h1>Browsitory</h1>
      <StatusView client={tauriRepoClient} />
    </main>
  );
}
