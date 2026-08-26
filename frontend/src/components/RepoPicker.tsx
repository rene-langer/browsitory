import { useEffect, useState } from "react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import { WorkspaceEditor } from "./WorkspaceEditor";
import styles from "./RepoPicker.module.css";

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
    <Panel title="Open a repository">
      <Toolbar>
        <button onClick={handleOpenFolder}>Open Folder</button>
        <button onClick={() => setCreatingWorkspace(true)}>Open Workspace Root</button>
      </Toolbar>
      {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
      {recentRepos.length === 0 ? (
        <p>No recent repositories</p>
      ) : (
        <ul className={styles.list}>
          {recentRepos.map((path) => (
            <ListRow key={path} onClick={() => onOpenRepo(path)}>
              {path}
            </ListRow>
          ))}
        </ul>
      )}
      <Panel title="Workspaces" headingLevel={3}>
        {workspacesError !== null && (
          <InlineError message={workspacesError} onDismiss={onDismissWorkspacesError} />
        )}
        {!workspacesLoading && workspaces.length === 0 ? (
          <p>No saved workspaces</p>
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
      </Panel>
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
  );
}
