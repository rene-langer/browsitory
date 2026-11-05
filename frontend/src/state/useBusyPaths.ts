import { useCallback, useMemo, useState } from "react";

/** The repo tabs whose workspace reports an operation in progress (their close is disabled). */
export function useBusyPaths() {
  const [busyByPath, setBusyByPath] = useState<Record<string, boolean>>({});
  const onBusyChange = useCallback((repoPath: string, busy: boolean) => {
    setBusyByPath((prev) => (prev[repoPath] === busy ? prev : { ...prev, [repoPath]: busy }));
  }, []);
  const busyPaths = useMemo(
    () => new Set(Object.entries(busyByPath).filter(([, busy]) => busy).map(([path]) => path)),
    [busyByPath],
  );
  return { onBusyChange, busyPaths };
}
