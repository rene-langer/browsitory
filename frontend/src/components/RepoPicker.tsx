import { useEffect, useState } from "react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import { WorkspaceEditor } from "./WorkspaceEditor";
import styles from "./RepoPicker.module.css";

function isApplePlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  return /mac|iphone|ipad/i.test(`${navigator.platform ?? ""} ${navigator.userAgent ?? ""}`);
}

function repoNameOf(path: string): string {
  const parts = path.split(/[\\/]/).filter((part) => part !== "");
  return parts[parts.length - 1] ?? path;
}

export function RepoPicker({
  client,
  onOpenRepo,
  onOpenWorkspace,
  workspaces,
  workspacesLoading,
  workspacesError,
  onDismissWorkspacesError,
  onCreateWorkspace,
  onEditWorkspace,
  onDeleteWorkspace,
}: {
  client: RepoClient;
  onOpenRepo: (path: string) => void;
  onOpenWorkspace: (workspace: Workspace) => void;
  workspaces: Workspace[];
  workspacesLoading: boolean;
  workspacesError: string | null;
  onDismissWorkspacesError: () => void;
  onCreateWorkspace: (name: string, root: string, members: string[]) => Promise<string>;
  onEditWorkspace: (id: string, name: string, members: string[]) => Promise<void>;
  onDeleteWorkspace: (id: string) => Promise<void>;
}) {
  const [recentRepos, setRecentRepos] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [creatingWorkspace, setCreatingWorkspace] = useState(false);
  const [editingWorkspace, setEditingWorkspace] = useState<Workspace | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] = useState<Workspace | null>(null);

  useEffect(() => {
    client
      .listRecentRepos()
      .then(setRecentRepos)
      .catch((err: unknown) => setError(String(err)));
  }, [client]);

  const handleOpenFolder = () => {
    client
      .pickRepoFolder()
      .then((path) => {
        if (path !== null) {
          onOpenRepo(path);
        }
      })
      .catch((err: unknown) => setError(String(err)));
  };

  if (creatingWorkspace) {
    return (
      <WorkspaceEditor
        client={client}
        onSave={(name, root, members) =>
          onCreateWorkspace(name, root, members).then((id) => {
            onOpenWorkspace({ id, name, rootPath: root, memberPaths: members });
            setCreatingWorkspace(false);
          })
        }
        onCancel={() => setCreatingWorkspace(false)}
      />
    );
  }

  if (editingWorkspace !== null) {
    return (
      <WorkspaceEditor
        client={client}
        existing={editingWorkspace}
        onSave={(name, _root, members) =>
          onEditWorkspace(editingWorkspace.id, name, members).then(() => setEditingWorkspace(null))
        }
        onCancel={() => setEditingWorkspace(null)}
      />
    );
  }

  return (
    <div className={styles.picker}>
      <Panel title="Open a repository">
        <Toolbar>
          <button onClick={handleOpenFolder}>Open folder</button>
          <button onClick={() => setCreatingWorkspace(true)}>Open workspace root</button>
        </Toolbar>
        {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
        <p className={styles.hint}>
          Tip: press <kbd>{isApplePlatform() ? "⌘K" : "Ctrl+K"}</kbd> once a repository is open to search every command.
        </p>
        {recentRepos.length === 0 ? (
          <>
            <p className={styles.empty}>No recent repositories</p>
            <p className={styles.hint}>Open a folder that contains a Git repository to get started.</p>
          </>
        ) : (
          <ul className={styles.list}>
            {recentRepos.map((path) => (
              <ListRow key={path} className={styles.repoRow} onClick={() => onOpenRepo(path)}>
                <span className={styles.repoName}>{repoNameOf(path)}</span>
                <span className={styles.repoPath}>{path}</span>
              </ListRow>
            ))}
          </ul>
        )}
        <section className={styles.workspaces} aria-label="Workspaces">
          <h3 className={styles.workspacesTitle}>Workspaces</h3>
          <p className={styles.hint}>A workspace is a saved group of repositories you can open together.</p>
          <div>
            {workspacesError !== null && (
              <InlineError message={workspacesError} onDismiss={onDismissWorkspacesError} />
            )}
            {!workspacesLoading && workspaces.length === 0 ? (
              <p className={styles.empty}>No saved workspaces</p>
            ) : (
              <ul className={styles.list}>
                {workspaces.map((workspace) => (
                  <ListRow key={workspace.id}>
                    <span title={workspace.rootPath}>{workspace.name}</span>
                    <Toolbar>
                      <button
                        onClick={(event) => {
                          event.stopPropagation();
                          onOpenWorkspace(workspace);
                        }}
                      >
                        Open All
                      </button>
                      <button
                        onClick={(event) => {
                          event.stopPropagation();
                          setEditingWorkspace(workspace);
                        }}
                      >
                        Edit
                      </button>
                      <button
                        onClick={(event) => {
                          event.stopPropagation();
                          setDeleteConfirmation(workspace);
                        }}
                      >
                        Delete {workspace.name}
                      </button>
                    </Toolbar>
                  </ListRow>
                ))}
              </ul>
            )}
          </div>
        </section>
        {deleteConfirmation !== null && (
          <dialog open aria-label={`Delete workspace ${deleteConfirmation.name}`}>
            <p>
              Delete workspace {deleteConfirmation.name}? Its member repos stay open if currently open; only the saved
              workspace is removed.
            </p>
            <button
              type="button"
              onClick={() =>
                void onDeleteWorkspace(deleteConfirmation.id).then(() => setDeleteConfirmation(null))
              }
            >
              Delete workspace
            </button>
            <button type="button" onClick={() => setDeleteConfirmation(null)}>
              Cancel
            </button>
          </dialog>
        )}
      </Panel>
    </div>
  );
}
