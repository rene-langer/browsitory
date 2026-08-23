import { useEffect, useState } from "react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";
import { Panel } from "./primitives/Panel";
import styles from "./WorkspaceEditor.module.css";

function basename(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const segments = trimmed.split(/[\\/]/);
  return segments[segments.length - 1] || trimmed;
}

export function WorkspaceEditor({
  client,
  existing,
  onSave,
  onCancel,
}: {
  client: RepoClient;
  existing?: Workspace;
  onSave: (name: string, root: string, members: string[]) => Promise<void>;
  onCancel: () => void;
}) {
  const [root, setRoot] = useState<string | null>(existing?.rootPath ?? null);
  const [candidates, setCandidates] = useState<string[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set(existing?.memberPaths ?? []));
  const [name, setName] = useState(existing?.name ?? "");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (root === null) return;
    client
      .scanReposInRoot(root)
      .then((found) => {
        setCandidates(found);
        if (existing === undefined) {
          setSelected(new Set(found));
        } else {
          const savedMembers = new Set(existing.memberPaths);
          setSelected(new Set(found.filter((path) => savedMembers.has(path))));
        }
      })
      .catch((err: unknown) => setError(String(err)));
    // Only re-scan when `root` itself changes (picked once in create mode, fixed in edit mode)
    // — `existing`/`client` are stable identities for the component's lifetime.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root]);

  const handleChooseRoot = () => {
    client
      .pickRepoFolder()
      .then((picked) => {
        if (picked === null) return;
        setRoot(picked);
        setName(basename(picked));
      })
      .catch((err: unknown) => setError(String(err)));
  };

  const toggle = (path: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const handleSave = () => {
    if (root === null) return;
    onSave(name, root, Array.from(selected)).catch((err: unknown) => setError(String(err)));
  };

  return (
    <Panel title={existing === undefined ? "New Workspace" : "Edit Workspace"}>
      {error !== null && <p role="alert">{error}</p>}
      {root === null ? (
        <div className={styles.actions}>
          <button type="button" onClick={handleChooseRoot}>
            Choose Root Folder
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      ) : (
        <>
          <p title={root}>{root}</p>
          {candidates !== null && (
            <ul className={styles.list}>
              {candidates.map((path) => (
                <li key={path} className={styles.row}>
                  <input
                    type="checkbox"
                    id={`member-${path}`}
                    aria-label={path}
                    checked={selected.has(path)}
                    onChange={() => toggle(path)}
                  />
                  <label htmlFor={`member-${path}`} title={path}>
                    {basename(path)}
                  </label>
                </li>
              ))}
            </ul>
          )}
          <label>
            Workspace name
            <input
              aria-label="Workspace name"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <div className={styles.actions}>
            <button
              type="button"
              disabled={candidates === null || selected.size === 0 || name.trim() === ""}
              onClick={handleSave}
            >
              Save
            </button>
            <button type="button" onClick={onCancel}>
              Cancel
            </button>
          </div>
        </>
      )}
    </Panel>
  );
}
