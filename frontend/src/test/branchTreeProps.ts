import { render } from "@testing-library/react";
import { createElement } from "react";
import { vi } from "vitest";
import { BranchTreeWithDialogs, type BranchTreeProps } from "./BranchTreeWithDialogs";

export type { BranchTreeProps };

/**
 * Props and a render helper for the `BranchTree` tests, which are split by job like the component
 * (`BranchTree.test.tsx`, `NewBranchForm.test.tsx`, `AddRemoteForm.test.tsx`,
 * `SetUpstreamDialog.test.tsx`, `useRemoteBranchCache.test.tsx`). They all render the whole
 * `BranchTree`, so they guard its props API too. `renderTree` renders it with the remote dialogs it
 * opens (`BranchTreeWithDialogs`), as the workspace does.
 */

export const baseBranches = [
  { name: "main", isCurrent: true },
  { name: "feat/foo", isCurrent: false },
];

export function renderTree(overrides: Partial<BranchTreeProps> = {}) {
  const props = treeProps(overrides);
  return { ...render(createElement(BranchTreeWithDialogs, props)), props };
}

export function treeProps(overrides: Partial<BranchTreeProps>): BranchTreeProps {
  return {
    branches: baseBranches,
    createBranchDraft: null,
    onSwitchBranch: vi.fn(),
    onCreateBranch: vi.fn().mockResolvedValue(null),
    onDeleteBranch: vi.fn().mockResolvedValue(undefined),
    onRenameBranch: vi.fn(),
    onOpenCreateBranchDraft: vi.fn(),
    onCloseCreateBranchDraft: vi.fn(),
    onMergeBranch: vi.fn(),
    operationDisabled: false,
    operationDisabledReason: null,
    graphBranchSelection: null,
    onSetGraphBranchSelection: vi.fn(),
    onClearGraphBranchSelection: vi.fn(),
    onShowAllGraphBranches: vi.fn(),
    graphRemoteBranchSelection: null,
    onSetGraphRemoteBranchSelection: vi.fn(),
    onIsolateGraphBranch: vi.fn(),
    remoteBranchesInGraph: [],
    remotes: [],
    upstream: null,
    remoteUpstreams: {},
    onAddRemote: vi.fn().mockResolvedValue(null),
    onRenameRemote: vi.fn().mockResolvedValue(true),
    onUpdateRemoteUrls: vi.fn().mockResolvedValue(undefined),
    onRemoveRemote: vi.fn().mockResolvedValue(undefined),
    onSaveHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onForgetHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onSetRemoteAuthMode: vi.fn().mockResolvedValue(true),
    onSetUpstream: vi.fn().mockResolvedValue(undefined),
    onClearUpstream: vi.fn().mockResolvedValue(undefined),
    onListRemoteBranches: vi.fn().mockResolvedValue([]),
    onFetchRemote: vi.fn().mockResolvedValue(undefined),
    onPushCurrentBranch: vi.fn().mockResolvedValue(undefined),
    addRemoteDraftOpen: false,
    onOpenAddRemoteDraft: vi.fn(),
    onCloseAddRemoteDraft: vi.fn(),
    onOpenPushDialog: vi.fn(),
    onOpenDeleteRemoteBranch: vi.fn(),
    refreshGeneration: 0,
    onOpenRemoteDialog: vi.fn(),
    ...overrides,
  };
}
