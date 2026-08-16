import { useState } from "react";
import type { SubmoduleInfo } from "../ipc/RepoClient";

export function SubmodulePanel({
  submodules,
  onInit,
  onUpdate,
  operationDisabled,
}: {
  submodules: SubmoduleInfo[];
  onInit: (path: string) => Promise<void>;
  onUpdate: (path: string, recursive: boolean) => Promise<void>;
  operationDisabled: boolean;
}) {
  const [recursive, setRecursive] = useState(false);
  const [pendingPath, setPendingPath] = useState<string | null>(null);

  const runMutation = async (path: string, mutation: () => Promise<void>) => {
    setPendingPath(path);
    try {
      await mutation();
    } finally {
      setPendingPath(null);
    }
  };

  return (
    <section className="submodule-panel" aria-labelledby="submodule-panel-heading">
      <h2 id="submodule-panel-heading">Submodules</h2>
      <label>
        <input
          type="checkbox"
          checked={recursive}
          disabled={operationDisabled}
          onChange={(event) => setRecursive(event.target.checked)}
        />
        Update recursively
      </label>
      <ul className="submodule-list">
        {submodules.map((submodule) => {
          const rowDisabled = operationDisabled || pendingPath === submodule.path;

          return (
            <li key={submodule.path}>
              <strong>{submodule.path}</strong>
              <span>{submodule.url ?? "No URL configured"}</span>
              <span>{submodule.gitlinkId ?? "No recorded gitlink"}</span>
              <span>{submodule.initialized ? "Initialized" : "Not initialized"}</span>
              {submodule.headId !== null && <span>{submodule.headId}</span>}
              <button
                type="button"
                disabled={rowDisabled}
                onClick={() => void runMutation(submodule.path, () => onInit(submodule.path))}
              >
                Initialize {submodule.path}
              </button>
              <button
                type="button"
                disabled={rowDisabled}
                onClick={() => void runMutation(submodule.path, () => onUpdate(submodule.path, recursive))}
              >
                Update {submodule.path}
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
