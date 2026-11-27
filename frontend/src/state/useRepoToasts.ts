import { useCallback, useEffect, useMemo, useRef } from "react";
import { toastActions, type PushToast, type Toast, type ToastAction } from "./useToasts";

export interface RepoToastsOptions {
  /** The app's toast stack (`useToasts`). */
  toasts: Toast[];
  push: PushToast;
  dismiss: (id: number) => void;
  /** The tab shown. */
  activePath: string | null;
  /** The tabs whose workspace is mounted (`useMountedTabs`). */
  mountedTabs: readonly string[];
  /** What the tab strip calls a repository: its alias or its folder's name. */
  tabName: (repoPath: string) => string;
  switchTo: (repoPath: string) => void;
}

/**
 * One toast stack for every mounted tab (#1116): each workspace pushes through
 * `pushToastFor(repoPath)`, so its toasts carry their repository. While that tab isn't the one
 * shown, `toasts` names it ("widget: Fetched from origin.") and a button first shows the tab,
 * then runs once it is shown (an Undo checks that its tab is). The buttons of a workspace that
 * unmounted (its tab closed, or unloaded, #642) would act on nothing, so its toasts that have
 * buttons go.
 */
export function useRepoToasts({ toasts, push, dismiss, activePath, mountedTabs, tabName, switchTo }: RepoToastsOptions) {
  const latest = useRef({ push, activePath, switchTo });
  useEffect(() => {
    latest.current = { push, activePath, switchTo };
  }, [push, activePath, switchTo]);

  // A button of a hidden tab's toast, run once its tab is shown: this effect runs after the
  // workspace's own layout effects have marked it shown.
  const pending = useRef<{ repoPath: string; run: () => void } | null>(null);
  useEffect(() => {
    const next = pending.current;
    if (next === null || next.repoPath !== activePath) return;
    pending.current = null;
    next.run();
  }, [activePath]);

  // One stable pusher per repository: the outcome toasts' effects list it, and a new identity
  // would show their toast again. Its buttons check at click time whether their tab is shown.
  const pushers = useRef(new Map<string, PushToast>());
  const pushToastFor = useCallback((repoPath: string): PushToast => {
    let pusher = pushers.current.get(repoPath);
    if (pusher === undefined) {
      const inTab = (action: ToastAction): ToastAction => ({
        ...action,
        run: () => {
          if (latest.current.activePath === repoPath) {
            action.run();
            return;
          }
          pending.current = { repoPath, run: action.run };
          latest.current.switchTo(repoPath);
        },
      });
      pusher = (message, options = {}) =>
        latest.current.push(message, {
          ...options,
          action: options.action === undefined ? undefined : inTab(options.action),
          actions: options.actions?.map(inTab),
          repoPath,
        });
      pushers.current.set(repoPath, pusher);
    }
    return pusher;
  }, []);

  useEffect(() => {
    for (const toast of toasts) {
      if (toast.repoPath !== undefined && !mountedTabs.includes(toast.repoPath) && toastActions(toast).length > 0) {
        dismiss(toast.id);
      }
    }
  }, [toasts, mountedTabs, dismiss]);

  const shown = useMemo(
    () =>
      toasts.map((toast): Toast => {
        const { repoPath } = toast;
        if (repoPath === undefined || repoPath === activePath) return toast;
        return { ...toast, message: `${tabName(repoPath)}: ${toast.message}` };
      }),
    [toasts, activePath, tabName],
  );

  return { toasts: shown, pushToastFor };
}
