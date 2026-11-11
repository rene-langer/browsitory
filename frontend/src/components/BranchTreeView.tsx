import { useEffect, useState, type CSSProperties, type KeyboardEvent, type PointerEvent, type RefObject } from "react";
import { ChevronRight, Cloud, MoreHorizontal } from "lucide-react";
import type { BranchInfo, RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";
import { branchActions, remoteBranchActions, type RebaseTarget } from "../lib/actions/branchActions";
import { buildBranchTree, type BranchTreeNode } from "../lib/branchTree";
import { coversEveryBranch } from "../lib/graphSelection";
import { type BranchLaneColors, NO_BRANCH_LANE_COLORS } from "../lib/laneColors";
import { isMenuKey, menuAnchor, rectAnchor } from "../lib/menuKeys";
import type { OperationBlockers } from "../lib/operationStatus";
import { loadPersistedOpen, persistOpen } from "../lib/persistedOpenState";
import { dropTargetProps } from "../state/usePointerDrag";
import { branchDropKey } from "../lib/branchDropKey";
import type { RowContextMenu } from "./branchMenuItems";
import { ListRow } from "./primitives/ListRow";
import type { RemoteBranchCache } from "./useRemoteBranchCache";
import styles from "./BranchTree.module.css";

const LOCAL_FOLDER_KEY = "branchtree.local";

/** The drop-target scope of the Branches section's rows and folder headers (#744). */
export const BRANCH_DROP_SCOPE = "branch";
/** How long a drag hovers a collapsed folder before it opens (#744). */
export const FOLDER_OPEN_DELAY_MS = 600;

/** What the tree shows of a branch drag (#744): the dragged row, the hovered one and its verdict. */
export type BranchTreeDrag = {
  sourceKey: string | null;
  overKey: string | null;
  /** `on`: the drop would offer something; `refused`: it can't run (the label says why). */
  overState: "on" | "refused" | null;
};

/** A folder header's drop key: the drag opens the folder after hovering it a moment. */
function folderDropProps(folderPath: string) {
  return dropTargetProps(BRANCH_DROP_SCOPE, `folder:${folderPath}`);
}

/** Ahead/behind badge text for a local branch, or `null` when in sync or unknown. */
function aheadBehind(branch: BranchInfo): { ahead: number; behind: number; label: string } | null {
  const ahead = branch.ahead ?? null;
  const behind = branch.behind ?? null;
  if (branch.upstream == null || ahead === null || behind === null || (ahead === 0 && behind === 0)) return null;
  return { ahead, behind, label: `${ahead} ahead, ${behind} behind ${branch.upstream}` };
}

/** The inline rename of a local branch row, owned by `BranchTree` (its menu starts it). */
export type BranchRename = {
  /** The branch being renamed, `null` when none. */
  name: string | null;
  value: string;
  /** `branchNameProblem` of the trimmed value, `null` when valid. */
  problem: string | null;
  onChange: (value: string) => void;
  onCancel: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLInputElement>, oldName: string) => void;
};

/**
 * A branch swatch's colour, look and title (#673): the colour of the lane its tip sits on in the
 * graph. A tip that isn't a loaded row has no lane: a neutral swatch, dashed while the branch is
 * shown, with the reason in its title.
 */
function swatchLook(color: string | undefined, shown: boolean): { className: string; style: CSSProperties; title: string } {
  if (color !== undefined) {
    return {
      className: styles.swatch,
      style: { "--swatch": color } as CSSProperties,
      title: shown ? "Shown in the graph, in this colour" : "Hidden from the graph",
    };
  }
  return {
    className: `${styles.swatch} ${styles.swatchNoLane}`,
    style: { "--swatch": "var(--color-text-muted)" } as CSSProperties,
    title: shown
      ? "Its latest commit isn't in the loaded history: older than the loaded commits, or inside a collapsed merge"
      : "Hidden from the graph",
  };
}

/** What every row of the tree needs, passed down the nested folders unchanged. */
type RowContext = {
  branches: BranchInfo[];
  blockers: OperationBlockers;
  remoteCount: number;
  upstream: UpstreamInfo | null;
  graphBranchSelection: string[] | null;
  graphRemoteBranchSelection: string[] | null;
  remoteBranchesInGraph: string[];
  branchColors: BranchLaneColors;
  selectedRow: string | null;
  rename: BranchRename;
  rowMenu: RowContextMenu | null;
  isSubfolderOpen: (folderPath: string) => boolean;
  onSelectRow: (rowKey: string) => void;
  onToggleSubfolder: (folderPath: string) => void;
  onToggleGraphBranch: (name: string) => void;
  onToggleGraphRemoteBranch: (qualifiedName: string) => void;
  onSwitchBranch: (name: string) => void;
  onCheckoutRemoteBranch: (remoteName: string, branchName: string) => void;
  onGoToBranch?: (kind: "local" | "remote", name: string) => void;
  onOpenRowMenu: (menu: RowContextMenu) => void;
  onToggleRowMenu: (menu: RowContextMenu) => void;
  drag: BranchTreeDrag | null;
  onStartDrag?: (event: PointerEvent, source: RebaseTarget) => void;
};

/** A row's drop-target, drag styling and drag start (#744). */
function rowDragProps(ctx: RowContext, ref: RebaseTarget, disabled: boolean) {
  const key = branchDropKey(ref);
  const hovered = ctx.drag !== null && ctx.drag.overKey === key && ctx.drag.sourceKey !== key;
  return {
    dropTarget: { scope: BRANCH_DROP_SCOPE, key, position: hovered ? (ctx.drag?.overState ?? undefined) : undefined },
    dragging: ctx.drag?.sourceKey === key,
    onPointerDown:
      disabled || ctx.onStartDrag === undefined
        ? undefined
        : (event: PointerEvent<HTMLLIElement>) => {
            // The row's own controls (swatch, "…") keep their press.
            if ((event.target as Element).closest("button, input, textarea, select") !== null) return;
            ctx.onStartDrag?.(event, ref);
          },
  };
}

function SubfolderHeader({
  name,
  folderPath,
  open,
  onToggle,
}: {
  name: string;
  folderPath: string;
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      className={styles.folderHeader}
      aria-expanded={open}
      onClick={onToggle}
      {...folderDropProps(folderPath)}
    >
      <ChevronRight
        size={14}
        aria-hidden="true"
        className={open ? `${styles.chevron} ${styles.chevronOpen}` : styles.chevron}
      />
      {name}
    </button>
  );
}

function LocalBranchNodes({ nodes, ctx }: { nodes: BranchTreeNode<BranchInfo>[]; ctx: RowContext }) {
  return nodes.map((node) => {
    if (node.kind === "folder") {
      const folderPath = `local/${node.path}`;
      const open = ctx.isSubfolderOpen(folderPath);
      return (
        <li key={node.path} className={styles.folder}>
          <SubfolderHeader
            name={node.name}
            folderPath={folderPath}
            open={open}
            onToggle={() => ctx.onToggleSubfolder(folderPath)}
          />
          {open && (
            <ul className={styles.folderBody}>
              <LocalBranchNodes nodes={node.children} ctx={ctx} />
            </ul>
          )}
        </li>
      );
    }
    return <LocalBranchRow key={node.value.name} branch={node.value} label={node.name} ctx={ctx} />;
  });
}

function LocalBranchRow({ branch, label, ctx }: { branch: BranchInfo; label: string; ctx: RowContext }) {
  const { rename, rowMenu } = ctx;
  const rowKey = `local:${branch.name}`;
  const shownInGraph = (ctx.graphBranchSelection ?? ctx.branches.map((b) => b.name)).includes(branch.name);
  const isRenaming = rename.name === branch.name;
  const counts = aheadBehind(branch);
  return (
    <ListRow
      {...rowDragProps(ctx, { kind: "local", name: branch.name }, isRenaming)}
      className={ctx.selectedRow === rowKey ? `${styles.treeRow} ${styles.selectedRow}` : styles.treeRow}
      onClick={
        isRenaming
          ? undefined
          : () => {
              ctx.onSelectRow(rowKey);
              ctx.onGoToBranch?.("local", branch.name);
            }
      }
      onDoubleClick={
        isRenaming
          ? undefined
          : () => {
              const context = { blockers: ctx.blockers, remoteCount: ctx.remoteCount, upstream: ctx.upstream };
              if (branchActions(context, branch).checkout.enabled) ctx.onSwitchBranch(branch.name);
            }
      }
      onOpenMenu={
        isRenaming
          ? undefined
          : (anchor, trigger) =>
              ctx.onOpenRowMenu({ kind: "local-branch", name: branch.name, ...anchor, returnFocusTo: trigger })
      }
    >
      <div className={styles.rowInner}>
        <button
          type="button"
          {...swatchLook(ctx.branchColors.local.get(branch.name), shownInGraph)}
          aria-label={`Show ${branch.name} in graph`}
          aria-pressed={shownInGraph}
          onClick={(event) => {
            event.stopPropagation();
            ctx.onToggleGraphBranch(branch.name);
          }}
          onDoubleClick={(event) => event.stopPropagation()}
        />
        {isRenaming ? (
          <input
            autoFocus
            aria-label={`Rename ${branch.name}`}
            value={rename.value}
            aria-invalid={rename.problem !== null}
            aria-describedby={rename.problem !== null ? "rename-branch-problem" : undefined}
            onChange={(event) => rename.onChange(event.target.value)}
            onFocus={(event) => event.currentTarget.select()}
            onBlur={() => {
              if (rename.value.trim() === branch.name) rename.onCancel();
            }}
            onKeyDown={(event) => rename.onKeyDown(event, branch.name)}
          />
        ) : (
          <span
            className={branch.isCurrent ? `${styles.name} ${styles.currentName}` : styles.name}
            title={branch.name}
          >
            {branch.isCurrent && <span className={styles.currentMarker} role="img" aria-label="current branch" />}
            {label}
            {branch.isCurrent && " (current)"}
          </span>
        )}
        {!isRenaming && counts !== null && (
          // role="img": a plain span's aria-label is ignored by screen readers; the arrows
          // themselves are hidden so they are not read as "up arrow 2".
          <span className={styles.aheadBehind} role="img" aria-label={counts.label} title={counts.label}>
            {counts.ahead > 0 && <span aria-hidden="true">↑{counts.ahead}</span>}
            {counts.behind > 0 && <span aria-hidden="true">↓{counts.behind}</span>}
          </span>
        )}
        {!isRenaming && (
          <button
            type="button"
            className={styles.iconButton}
            aria-label={`Actions for ${branch.name}`}
            aria-haspopup="menu"
            data-menu-key={`local-branch:${branch.name}`}
            aria-expanded={rowMenu?.kind === "local-branch" && rowMenu.name === branch.name}
            onClick={(event) => {
              // Opening the menu doesn't also go to the branch (the row's click).
              event.stopPropagation();
              ctx.onToggleRowMenu({
                kind: "local-branch",
                name: branch.name,
                ...rectAnchor(event.currentTarget),
                returnFocusTo: event.currentTarget,
              });
            }}
          >
            <MoreHorizontal size={12} aria-hidden="true" />
          </button>
        )}
      </div>
      {isRenaming && rename.problem !== null && (
        <p id="rename-branch-problem" className={styles.draftProblem}>
          {rename.problem}
        </p>
      )}
    </ListRow>
  );
}

function RemoteBranchNodes({
  remoteName,
  nodes,
  ctx,
}: {
  remoteName: string;
  nodes: BranchTreeNode<string>[];
  ctx: RowContext;
}) {
  return nodes.map((node) => {
    if (node.kind === "folder") {
      const folderPath = `remote/${remoteName}/${node.path}`;
      const open = ctx.isSubfolderOpen(folderPath);
      return (
        <li key={node.path} className={styles.folder}>
          <SubfolderHeader
            name={node.name}
            folderPath={folderPath}
            open={open}
            onToggle={() => ctx.onToggleSubfolder(folderPath)}
          />
          {open && (
            <ul className={styles.folderBody}>
              <RemoteBranchNodes remoteName={remoteName} nodes={node.children} ctx={ctx} />
            </ul>
          )}
        </li>
      );
    }
    return (
      <RemoteBranchRow key={node.value} remoteName={remoteName} branchName={node.value} label={node.name} ctx={ctx} />
    );
  });
}

function RemoteBranchRow({
  remoteName,
  branchName,
  label,
  ctx,
}: {
  remoteName: string;
  branchName: string;
  label: string;
  ctx: RowContext;
}) {
  const { rowMenu } = ctx;
  const rowKey = `remote:${remoteName}:${branchName}`;
  const remoteShown = (ctx.graphRemoteBranchSelection ?? ctx.remoteBranchesInGraph).includes(`${remoteName}/${branchName}`);
  return (
    <ListRow
      {...rowDragProps(ctx, { kind: "remote", remoteName, branchName }, false)}
      className={ctx.selectedRow === rowKey ? `${styles.treeRow} ${styles.selectedRow}` : styles.treeRow}
      onClick={() => {
        ctx.onSelectRow(rowKey);
        ctx.onGoToBranch?.("remote", `${remoteName}/${branchName}`);
      }}
      onDoubleClick={() => {
        if (remoteBranchActions(ctx.blockers, remoteName, branchName).checkout.enabled) {
          ctx.onCheckoutRemoteBranch(remoteName, branchName);
        }
      }}
      onOpenMenu={(anchor, trigger) =>
        ctx.onOpenRowMenu({ kind: "remote-branch", remoteName, branchName, ...anchor, returnFocusTo: trigger })
      }
    >
      <div className={styles.rowInner}>
        <button
          type="button"
          {...swatchLook(ctx.branchColors.remote.get(`${remoteName}/${branchName}`), remoteShown)}
          aria-label={`Show ${remoteName}/${branchName} in graph`}
          aria-pressed={remoteShown}
          onClick={(event) => {
            event.stopPropagation();
            ctx.onToggleGraphRemoteBranch(`${remoteName}/${branchName}`);
          }}
          onDoubleClick={(event) => event.stopPropagation()}
        />
        <span className={styles.name} title={branchName}>
          {label}
        </span>
        {/* Visible affordance for the row's menu (which the row itself also opens on
            right-click, the Menu key or Shift+F10): without it a pointer or screen-reader
            user has no indication those actions exist (AUD-2026-09-05-FE-001). */}
        <button
          type="button"
          className={styles.iconButton}
          aria-label={`Actions for ${remoteName}/${branchName}`}
          aria-haspopup="menu"
          data-menu-key={`remote-branch:${remoteName}/${branchName}`}
          aria-expanded={
            rowMenu?.kind === "remote-branch" && rowMenu.remoteName === remoteName && rowMenu.branchName === branchName
          }
          onClick={(event) => {
            event.stopPropagation();
            ctx.onToggleRowMenu({
              kind: "remote-branch",
              remoteName,
              branchName,
              ...rectAnchor(event.currentTarget),
              returnFocusTo: event.currentTarget,
            });
          }}
        >
          <MoreHorizontal size={12} aria-hidden="true" />
        </button>
      </div>
    </ListRow>
  );
}

/**
 * The Local folder and one folder per remote, each split into "/" subfolders: the rows, their
 * graph swatches, selection, double-click checkout and the menu keys and buttons. The menus
 * themselves and the inline rename's state belong to `BranchTree`; the remote branch lists to
 * `useRemoteBranchCache`.
 */
export function BranchTreeView({
  branches,
  remotes,
  blockers,
  upstream,
  graphBranchSelection,
  onSetGraphBranchSelection,
  onClearGraphBranchSelection,
  graphRemoteBranchSelection,
  onSetGraphRemoteBranchSelection,
  remoteBranchesInGraph,
  branchColors = NO_BRANCH_LANE_COLORS,
  remoteCache,
  remoteHeaders,
  rename,
  rowMenu,
  onOpenRowMenu,
  onToggleRowMenu,
  onSwitchBranch,
  onCheckoutRemoteBranch,
  onGoToBranch,
  drag = null,
  onStartDrag,
}: {
  branches: BranchInfo[];
  remotes: RemoteInfo[];
  blockers: OperationBlockers;
  upstream: UpstreamInfo | null;
  graphBranchSelection: string[] | null;
  onSetGraphBranchSelection: (selectedBranches: string[]) => void;
  onClearGraphBranchSelection: () => void;
  graphRemoteBranchSelection: string[] | null;
  onSetGraphRemoteBranchSelection: (selectedBranches: string[]) => void;
  /** The null-selection fallback for the remote swatches; see `BranchTree`'s prop of that name. */
  remoteBranchesInGraph: string[];
  /** Each branch tip's lane colour in the graph, for the swatches (#673). */
  branchColors?: BranchLaneColors;
  remoteCache: RemoteBranchCache;
  /** Each remote folder's header button by remote name, for `BranchTree`'s reveal request (#494). */
  remoteHeaders?: RefObject<Map<string, HTMLButtonElement>>;
  rename: BranchRename;
  rowMenu: RowContextMenu | null;
  onOpenRowMenu: (menu: RowContextMenu) => void;
  onToggleRowMenu: (menu: RowContextMenu) => void;
  onSwitchBranch: (name: string) => void;
  onCheckoutRemoteBranch: (remoteName: string, branchName: string) => void;
  onGoToBranch?: (kind: "local" | "remote", name: string) => void;
  /** A branch drag in progress (#744), owned by `BranchTree`; `null` while there is none. */
  drag?: BranchTreeDrag | null;
  /** A press on a row that may become a drag of that branch. */
  onStartDrag?: (event: PointerEvent, source: RebaseTarget) => void;
}) {
  const [localOpen, setLocalOpen] = useState(() => loadPersistedOpen(LOCAL_FOLDER_KEY, true));
  // Selection for a single row in the local/remote branch tree — a plain visual highlight, not a
  // listbox/roving-tabindex pattern: see ListRow.tsx's doc comment for why that heavier pattern
  // is reserved for containers that also own keyboard navigation, which this tree doesn't (yet).
  // Keyed "local:<branch>" or "remote:<remoteName>:<branchName>" so the two namespaces can't collide.
  const [selectedRow, setSelectedRow] = useState<string | null>(null);
  // Open/closed state for the "/"-split subfolders inside the Local tree and inside each remote's
  // tree — distinct from `localOpen` (the top-level "Local" folder) and the remote cache's open
  // remotes (the top-level remote folders, which also trigger the lazy branch-list fetch on first expand).
  const [openSubfolders, setOpenSubfolders] = useState<Record<string, boolean>>({});

  function isSubfolderOpen(folderPath: string): boolean {
    return openSubfolders[folderPath] ?? loadPersistedOpen(`branchtree.folder.${folderPath}`, true);
  }

  function toggleSubfolder(folderPath: string): void {
    const next = !isSubfolderOpen(folderPath);
    setOpenSubfolders((prev) => ({ ...prev, [folderPath]: next }));
    persistOpen(`branchtree.folder.${folderPath}`, next);
  }

  // A drag hovering a collapsed folder opens it after a moment (#744), for this session only.
  const hoveredFolder = drag?.overKey?.startsWith("folder:") === true ? drag.overKey.slice("folder:".length) : null;
  useEffect(() => {
    if (hoveredFolder === null) return;
    const timer = setTimeout(() => {
      if (hoveredFolder === "local") setLocalOpen(true);
      else if (hoveredFolder.startsWith("remote-folder/")) remoteCache.openForDrag(hoveredFolder.slice("remote-folder/".length));
      else setOpenSubfolders((prev) => ({ ...prev, [hoveredFolder]: true }));
    }, FOLDER_OPEN_DELAY_MS);
    return () => clearTimeout(timer);
    // Only a newly hovered folder restarts the wait.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hoveredFolder]);

  const toggleGraphBranch = (name: string) => {
    const shown = graphBranchSelection ?? branches.map((b) => b.name);
    const next = shown.includes(name) ? shown.filter((n) => n !== name) : [...shown, name];
    // Every local branch shown is the default graph: no saved list, so later branches join it
    // and a single branch isn't walked by first parents only (#402). The remote selection stays.
    if (branches.length > 0 && coversEveryBranch(next, branches.map((b) => b.name))) onClearGraphBranchSelection();
    else onSetGraphBranchSelection(next);
  };

  const toggleGraphRemoteBranch = (qualifiedName: string) => {
    const shown = graphRemoteBranchSelection ?? remoteBranchesInGraph;
    const next = shown.includes(qualifiedName)
      ? shown.filter((n) => n !== qualifiedName)
      : [...shown, qualifiedName];
    onSetGraphRemoteBranchSelection(next);
  };

  const ctx: RowContext = {
    branches,
    blockers,
    remoteCount: remotes.length,
    upstream,
    graphBranchSelection,
    graphRemoteBranchSelection,
    remoteBranchesInGraph,
    branchColors,
    selectedRow,
    rename,
    rowMenu,
    isSubfolderOpen,
    onSelectRow: setSelectedRow,
    onToggleSubfolder: toggleSubfolder,
    onToggleGraphBranch: toggleGraphBranch,
    onToggleGraphRemoteBranch: toggleGraphRemoteBranch,
    onSwitchBranch,
    onCheckoutRemoteBranch,
    onGoToBranch,
    onOpenRowMenu,
    onToggleRowMenu,
    drag,
    onStartDrag,
  };

  return (
    <ul className={styles.tree}>
      <li className={styles.folder}>
        <button
          type="button"
          className={styles.folderHeader}
          aria-expanded={localOpen}
          {...folderDropProps("local")}
          onClick={() => {
            const next = !localOpen;
            setLocalOpen(next);
            persistOpen(LOCAL_FOLDER_KEY, next);
          }}
        >
          <ChevronRight
            size={14}
            aria-hidden="true"
            className={localOpen ? `${styles.chevron} ${styles.chevronOpen}` : styles.chevron}
          />
          Local
        </button>
        {localOpen && (
          <ul className={styles.folderBody}>
            <LocalBranchNodes
              nodes={buildBranchTree(branches.map((branch) => ({ path: branch.name, value: branch })))}
              ctx={ctx}
            />
          </ul>
        )}
      </li>

      {remotes.map((remote) => {
        const open = remoteCache.isOpen(remote.name);
        const loaded = remoteCache.branches[remote.name];
        return (
          <li key={remote.name} className={styles.folder}>
            <div className={styles.folderRow}>
              <button
                ref={(element) => {
                  if (element === null || remoteHeaders === undefined) return;
                  remoteHeaders.current.set(remote.name, element);
                  return () => {
                    remoteHeaders.current.delete(remote.name);
                  };
                }}
                type="button"
                className={styles.folderHeader}
                aria-expanded={open}
                {...folderDropProps(`remote-folder/${remote.name}`)}
                aria-keyshortcuts="Shift+F10 ContextMenu"
                onClick={() => remoteCache.toggle(remote.name)}
                onKeyDown={(event) => {
                  if (!isMenuKey(event)) return;
                  event.preventDefault();
                  onOpenRowMenu({ kind: "remote-folder", name: remote.name, ...rectAnchor(event.currentTarget) });
                }}
                onContextMenu={(event) => {
                  event.preventDefault();
                  onOpenRowMenu({
                    kind: "remote-folder",
                    name: remote.name,
                    ...menuAnchor(event, event.currentTarget),
                    returnFocusTo: event.currentTarget,
                  });
                }}
              >
                <ChevronRight
                  size={14}
                  aria-hidden="true"
                  className={open ? `${styles.chevron} ${styles.chevronOpen}` : styles.chevron}
                />
                <Cloud size={14} aria-hidden="true" />
                {remote.name}
              </button>
              {/* The remote's actions without knowing Shift+F10 (macOS keyboards have no Menu key). */}
              <button
                type="button"
                className={styles.iconButton}
                aria-label={`Actions for remote ${remote.name}`}
                aria-haspopup="menu"
                data-menu-key={`remote-folder:${remote.name}`}
                aria-expanded={rowMenu?.kind === "remote-folder" && rowMenu.name === remote.name}
                onClick={(event) =>
                  onToggleRowMenu({
                    kind: "remote-folder",
                    name: remote.name,
                    ...rectAnchor(event.currentTarget),
                    returnFocusTo: event.currentTarget,
                  })
                }
              >
                <MoreHorizontal size={12} aria-hidden="true" />
              </button>
            </div>
            {open && (
              <ul className={styles.folderBody}>
                {loaded === undefined && remoteCache.error === null && <li className={styles.statusRow}>Loading…</li>}
                {loaded?.length === 0 && <li className={styles.statusRow}>No branches</li>}
                <RemoteBranchNodes
                  remoteName={remote.name}
                  nodes={buildBranchTree((loaded ?? []).map((name) => ({ path: name, value: name })))}
                  ctx={ctx}
                />
              </ul>
            )}
          </li>
        );
      })}
    </ul>
  );
}
