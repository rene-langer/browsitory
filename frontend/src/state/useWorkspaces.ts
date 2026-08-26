import { useCallback, useEffect, useState } from "react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";

export interface UseWorkspacesResult {
  workspaces: Workspace[];
  loading: boolean;
  error: string | null;
  // Clears `error` without waiting for the next successful refresh — its dismiss control
  // (issue #30/UX-002).
  dismissError(): void;
  createWorkspace(name: string, root: string, members: string[]): Promise<string>;
  editWorkspace(id: string, name: string, members: string[]): Promise<void>;
  deleteWorkspace(id: string): Promise<void>;
}

export function useWorkspaces(client: RepoClient): UseWorkspacesResult {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await client.listWorkspaces();
      setWorkspaces(list);
      setError(null);
    } catch (err: unknown) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [client]);

  useEffect(() => {
    void Promise.resolve().then(refresh);
  }, [refresh]);

  const createWorkspace = useCallback(
    async (name: string, root: string, members: string[]) => {
      const id = await client.saveWorkspace(name, root, members);
      await refresh();
      return id;
    },
    [client, refresh],
  );

  const editWorkspace = useCallback(
    async (id: string, name: string, members: string[]) => {
      await client.updateWorkspace(id, name, members);
      await refresh();
    },
    [client, refresh],
  );

  const deleteWorkspace = useCallback(
    async (id: string) => {
      await client.deleteWorkspace(id);
      await refresh();
    },
    [client, refresh],
  );

  const dismissError = useCallback(() => {
    setError(null);
  }, []);

  return { workspaces, loading, error, dismissError, createWorkspace, editWorkspace, deleteWorkspace };
}
