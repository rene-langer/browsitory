import { useState } from "react";
import { Package } from "lucide-react";
import type { SubmoduleInfo } from "../ipc/RepoClient";
import { AccordionSection } from "./primitives/AccordionSection";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./SubmodulePanel.module.css";

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
    <AccordionSection title="Submodules" storageKey="sidebar-submodules" icon={Package} count={submodules.length}>
      <label>
        <input
          type="checkbox"
          checked={recursive}
          disabled={operationDisabled}
          onChange={(event) => setRecursive(event.target.checked)}
        />
        Update recursively
      </label>
      <ul className={styles.list}>
        {submodules.map((submodule) => {
          const rowDisabled = operationDisabled || pendingPath === submodule.path;

          return (
            <li key={submodule.path}>
              <Package size={14} aria-hidden="true" className={styles.rowIcon} />
              <strong>{submodule.path}</strong>
              <span>{submodule.url ?? "No URL configured"}</span>
              <span>{submodule.gitlinkId ?? "No recorded gitlink"}</span>
              <span>{submodule.initialized ? "Initialized" : "Not initialized"}</span>
              {submodule.headId !== null && <span>{submodule.headId}</span>}
              <Toolbar>
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
              </Toolbar>
            </li>
          );
        })}
      </ul>
    </AccordionSection>
  );
}
