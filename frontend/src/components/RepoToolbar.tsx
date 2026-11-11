import { ReasonButton } from "./primitives/ReasonButton";
import type { ComponentProps } from "react";
import { FileDiff, GitBranch, PanelLeft, RefreshCw } from "lucide-react";
import type { UpstreamInfo } from "../ipc/RepoClient";
import type { SplitViewLeftState } from "./primitives/SplitView";
import { ariaKeyShortcuts, shortcutLabel } from "../lib/keybindings";
import { NavigationButtons } from "./NavigationButtons";
import { SyncBar } from "./SyncBar";
import { UndoRedoButtons } from "./UndoRedoButtons";
import styles from "./RepoToolbar.module.css";


/**
 * The one repository toolbar above the sidebar, graph and diff (#637): the sidebar toggle (a
 * drawer in a narrow layout, #635), the current branch, which opens the branch switcher (#638),
 * Back/Forward, Fetch/Pull/Push with the ahead/behind counts, what is running (a transfer's
 * progress and Cancel, #659), Stash, Apply patch… (#649), Undo/Redo and Refresh.
 */
export function RepoToolbar({
  sidebar,
  branch,
  navigation,
  sync,
  undo,
  applyPatch,
  onRefresh,
}: {
  /** `onToggle` gets the button, for the drawer to return focus to. */
  sidebar: { state: SplitViewLeftState; controls: string; onToggle: (button: HTMLElement) => void };
  branch: {
    current: string | null;
    detachedAt: string | null;
    upstream: UpstreamInfo | null;
    onOpenSwitcher: () => void;
  };
  navigation: ComponentProps<typeof NavigationButtons>;
  /** Includes `activity`, the running transfer or action (#659, #660). */
  sync: Omit<ComponentProps<typeof SyncBar>, "leading" | "trailing" | "ariaLabel" | "pane">;
  undo: ComponentProps<typeof UndoRedoButtons>;
  /** "Apply patch…" (#649), disabled with `disabledReason` while repository actions can't run. */
  applyPatch: { disabledReason: string | null; onApplyPatch: () => void };
  onRefresh: () => void;
}) {
  const sidebarShown = sidebar.state !== "collapsed";
  return (
    <SyncBar
      {...sync}
      ariaLabel="Repository"
      pane
      leading={
        <>
          <button
            type="button"
            className={styles.iconButton}
            aria-label="Sidebar"
            aria-expanded={sidebarShown}
            aria-controls={sidebar.controls}
            title={`${sidebarShown ? "Hide" : "Show"} the sidebar (${shortcutLabel("toggle-sidebar")})`}
            aria-keyshortcuts={ariaKeyShortcuts("toggle-sidebar")}
            onClick={(event) => sidebar.onToggle(event.currentTarget)}
          >
            <PanelLeft size={16} aria-hidden="true" />
          </button>
          <BranchChip {...branch} />
          <NavigationButtons {...navigation} />
        </>
      }
      trailing={
        <>
          <ReasonButton
            className={styles.iconButton}
            aria-label="Apply patch…"
            title="Apply a patch file to the working tree"
            reason={applyPatch.disabledReason}
            onClick={applyPatch.onApplyPatch}
          >
            <FileDiff size={14} aria-hidden="true" />
          </ReasonButton>
          <UndoRedoButtons {...undo} />
          <button
            type="button"
            className={styles.iconButton}
            aria-label="Refresh"
            title={`Refresh (${shortcutLabel("refresh")})`}
            aria-keyshortcuts={ariaKeyShortcuts("refresh")}
            onClick={onRefresh}
          >
            <RefreshCw size={14} aria-hidden="true" />
          </button>
        </>
      }
    />
  );
}

/** "main → origin/main": the current branch and its upstream; a click opens "Switch branch…". */
function BranchChip({
  current,
  detachedAt,
  upstream,
  onOpenSwitcher,
}: {
  current: string | null;
  detachedAt: string | null;
  upstream: UpstreamInfo | null;
  onOpenSwitcher: () => void;
}) {
  const name = current ?? (detachedAt !== null ? `detached at ${detachedAt}` : "no branch");
  const tracking = current !== null && upstream !== null ? `${upstream.remoteName}/${upstream.remoteBranch}` : null;
  const label = `Current branch ${name}${tracking === null ? "" : `, tracking ${tracking}`}. Switch branch…`;
  return (
    <button type="button" className={styles.branchChip} aria-label={label} title="Switch branch, or create one" onClick={onOpenSwitcher}>
      <GitBranch size={14} aria-hidden="true" />
      <span className={styles.branchName}>{name}</span>
      {tracking !== null && <span className={styles.tracking}>→ {tracking}</span>}
    </button>
  );
}
