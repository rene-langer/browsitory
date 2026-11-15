import { useEffect, useState } from "react";
import { ExternalLink, GitMerge, GitPullRequest, GitPullRequestClosed, MoreHorizontal } from "lucide-react";
import type {
  CreatePullRequest,
  ForgeProvider,
  ForgeRepository,
  PullRequestList,
} from "../ipc/RepoClient";
import { rectAnchor } from "../lib/menuKeys";
import { useForgeAccount } from "../state/useForgeAccount";
import { AccordionGroup } from "./primitives/AccordionGroup";
import { AccordionSection } from "./primitives/AccordionSection";
import { ContextMenu } from "./primitives/ContextMenu";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import { ConnectForgeDialog } from "./ConnectForgeDialog";
import { NewPullRequestDialog } from "./NewPullRequestDialog";
import styles from "./PullRequestPanel.module.css";

// Providers report state as free-form strings ("open"/"closed" from GitHub, "OPEN"/"MERGED"/…
// from Bitbucket, lowercased by the backend — see `crates/repo-service/src/pull_requests.rs`), so
// this only special-cases the two states worth a distinct glyph and falls back to a generic
// closed icon for everything else (declined, superseded, …).
function pullRequestStateIcon(state: string) {
  if (state === "open") return GitPullRequest;
  if (state === "merged") return GitMerge;
  return GitPullRequestClosed;
}

interface ForgeRepositorySectionProps {
  repository: ForgeRepository;
  // `undefined` means "never listed for this remote yet". A per-remote entry — see
  // `useAppState.ts`'s `AppState.pullRequests`, keyed by `remoteName` so listing/creating
  // against one remote can never clobber or hide another remote's rows.
  pullRequests: PullRequestList | undefined;
  onListPullRequests: (remoteName: string, account: string) => Promise<void>;
  /** Resolves to `null` once saved, else the failure the Connect dialog shows. */
  onSaveForgeToken: (provider: ForgeProvider, account: string, token: string) => Promise<string | null>;
  onForgetForgeToken: (provider: ForgeProvider, account: string) => Promise<void>;
  /** Resolves to `null` once created, else the failure the New pull request dialog shows. */
  onCreatePullRequest: (remoteName: string, account: string, pullRequest: CreatePullRequest) => Promise<string | null>;
  onOpenExternalUrl: (url: string) => Promise<void>;
  operationDisabled: boolean;
  // Human-readable reason `operationDisabled` is true, shown as a `title` on the controls it
  // disables (issue #31/UX-003). `null` when nothing is blocking.
  operationDisabledReason: string | null;
  // Known local branches: offered as suggestions for the source/target fields and used to
  // default them (current branch, and main/master).
  branches: { name: string; isCurrent: boolean }[];
}

// Rendered once outside the sections (see `PullRequestPanel`): WebKitWebDriver cannot compute the
// text of an element that contains a `<datalist>`, which breaks the e2e `toHaveText` assertions.
const BRANCH_SUGGESTIONS_ID = "pr-branch-suggestions";

/**
 * One forge repository's card (#1012). Not connected, it offers "Connect to <provider>", a dialog
 * for the account and its token. Connected (an account is remembered for the provider), it lists
 * the pull requests by itself, "New pull request…" opens a dialog, and its "…" menu has Refresh
 * and Disconnect, which forgets the token.
 */
function ForgeRepositorySection({
  repository,
  pullRequests,
  onListPullRequests,
  onSaveForgeToken,
  onForgetForgeToken,
  onCreatePullRequest,
  onOpenExternalUrl,
  operationDisabled,
  operationDisabledReason,
  branches,
}: ForgeRepositorySectionProps) {
  const [account, setAccount] = useForgeAccount(repository.provider);
  // True while a listing is out; a connected section starts with one (the effect below).
  const [listing, setListing] = useState(account !== null);
  const [dialog, setDialog] = useState<"connect" | "create" | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; returnFocusTo?: HTMLElement } | null>(null);
  const busyReason = operationDisabled ? operationDisabledReason : null;
  const menuKey = `pull-requests:${repository.remoteName}`;

  // `listing` is set by whoever starts it: the Refresh item, Connect, or the initial state.
  const listPullRequests = (connectedAccount: string) =>
    onListPullRequests(repository.remoteName, connectedAccount).finally(() => setListing(false));
  const refresh = (connectedAccount: string) => {
    setListing(true);
    void listPullRequests(connectedAccount);
  };

  // Connected, the section lists its pull requests by itself (#1012): each time it opens, so the
  // list is fresh, and right after connecting.
  useEffect(() => {
    if (account !== null) void listPullRequests(account);
    // Not on every change of the list: a failed listing clears it, and must not retry in a loop.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [account]);

  const disconnect = async () => {
    if (account === null) return;
    await onForgetForgeToken(repository.provider, account);
    setAccount(null);
  };

  // The card's title — provider, owner/name, and remoteName together, since remoteName is the
  // true disambiguator (a repo can be listed under multiple remotes/providers with identical
  // owner/name, per `AppState.pullRequests`' per-remote keying above).
  const sectionLabel = `${repository.provider}: ${repository.owner}/${repository.name} (${repository.remoteName})`;
  const visibleRows = account === null ? null : (pullRequests?.pullRequests ?? null);

  return (
    <>
    <AccordionSection
      title={sectionLabel}
      storageKey={`sidebar-pr-${repository.remoteName}`}
      headingLevel={3}
      defaultOpen
      icon={GitPullRequest}
      count={account === null ? undefined : pullRequests?.pullRequests.length}
      actions={
        account === null ? undefined : (
          <button
            type="button"
            className={styles.iconButton}
            aria-label={`Pull request actions for ${repository.remoteName}`}
            aria-haspopup="menu"
            aria-expanded={menu !== null}
            data-menu-key={menuKey}
            onClick={(event) => {
              const trigger = event.currentTarget;
              if (menu !== null) {
                trigger.focus({ preventScroll: true });
                setMenu(null);
              } else setMenu({ ...rectAnchor(trigger), returnFocusTo: trigger });
            }}
          >
            <MoreHorizontal size={14} aria-hidden="true" />
          </button>
        )
      }
    >
      {account === null ? (
        <>
          <p className={styles.empty}>
            Connect with a personal access token to see this repository&apos;s pull requests and create new ones.
          </p>
          <Toolbar>
            <button type="button" onClick={() => setDialog("connect")}>
              Connect to {repository.provider}
            </button>
          </Toolbar>
        </>
      ) : (
        <>
          <Toolbar>
            <button type="button" onClick={() => setDialog("create")}>
              New pull request…
            </button>
          </Toolbar>
          {visibleRows === null ? (
            listing && <p className={styles.empty}>Loading pull requests…</p>
          ) : (
            <>
              {pullRequests?.truncated === true && (
                <p role="status" className={styles.empty}>
                  Showing the first {visibleRows.length} pull requests — more may be available on the provider.
                </p>
              )}
              {visibleRows.length === 0 ? (
                <p className={styles.empty}>No pull requests found for this remote.</p>
              ) : (
                <ul className={styles.prList} aria-label={`Pull requests for ${repository.remoteName}`}>
                  {visibleRows.map((pullRequest) => {
                    const StateIcon = pullRequestStateIcon(pullRequest.state);
                    return (
                      <ListRow key={pullRequest.id}>
                        <StateIcon size={14} aria-hidden="true" className={styles.rowIcon} />
                        <span className={styles.text}>
                          <span className={styles.title} title={pullRequest.title}>
                            #{pullRequest.number} {pullRequest.title}
                          </span>
                          <span className={styles.details}>
                            {pullRequest.sourceBranch} → {pullRequest.targetBranch} · {pullRequest.author} ·{" "}
                            {pullRequest.state}
                          </span>
                        </span>
                        <button
                          type="button"
                          className={styles.linkButton}
                          aria-label={`Open #${pullRequest.number} in your browser`}
                          title={pullRequest.url}
                          onClick={() => void onOpenExternalUrl(pullRequest.url)}
                        >
                          <ExternalLink size={14} aria-hidden="true" />
                        </button>
                      </ListRow>
                    );
                  })}
                </ul>
              )}
            </>
          )}
        </>
      )}
    </AccordionSection>
      {/* Outside the card's body, so its header's "…" works while the card is collapsed. */}
      {menu !== null && account !== null && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          returnFocusTo={menu.returnFocusTo}
          isTrigger={(button) => button.getAttribute("data-menu-key") === menuKey}
          onClose={() => setMenu(null)}
          items={[
            {
              label: "New pull request…",
              onSelect: () => setDialog("create"),
              group: "pull-requests",
            },
            {
              label: "Refresh",
              disabled: listing,
              onSelect: () => refresh(account),
              group: "pull-requests",
            },
            {
              label: "Disconnect",
              disabled: operationDisabled,
              title: busyReason ?? `Forget the token of ${account} for ${repository.provider}`,
              onSelect: () => void disconnect(),
              group: "connection",
            },
          ]}
        />
      )}
      {dialog === "connect" && (
        <ConnectForgeDialog
          provider={repository.provider}
          disabled={operationDisabled}
          disabledReason={operationDisabledReason}
          onConnect={async (name, token) => {
            const failure = await onSaveForgeToken(repository.provider, name, token);
            if (failure === null) {
              setDialog(null);
              setListing(true);
              setAccount(name);
            }
            return failure;
          }}
          onCancel={() => setDialog(null)}
        />
      )}
      {dialog === "create" && account !== null && (
        <NewPullRequestDialog
          remoteName={repository.remoteName}
          branches={branches}
          branchSuggestionsId={BRANCH_SUGGESTIONS_ID}
          disabled={operationDisabled}
          disabledReason={operationDisabledReason}
          onCreate={async (pullRequest) => {
            const failure = await onCreatePullRequest(repository.remoteName, account, pullRequest);
            if (failure === null) setDialog(null);
            return failure;
          }}
          onCancel={() => setDialog(null)}
        />
      )}
    </>
  );
}

export function PullRequestPanel({
  forgeRepositories,
  pullRequests,
  onListPullRequests,
  onSaveForgeToken,
  onForgetForgeToken,
  onCreatePullRequest,
  onOpenExternalUrl,
  operationDisabled,
  operationDisabledReason,
  branches,
  revealRequest,
}: {
  forgeRepositories: ForgeRepository[];
  pullRequests: Record<string, PullRequestList>;
  onListPullRequests: (remoteName: string, account: string) => Promise<void>;
  onSaveForgeToken: (provider: ForgeProvider, account: string, token: string) => Promise<string | null>;
  onForgetForgeToken: (provider: ForgeProvider, account: string) => Promise<void>;
  onCreatePullRequest: (remoteName: string, account: string, pullRequest: CreatePullRequest) => Promise<string | null>;
  onOpenExternalUrl: (url: string) => Promise<void>;
  operationDisabled: boolean;
  // Human-readable reason `operationDisabled` is true, forwarded to each `ForgeRepositorySection`
  // (issue #31/UX-003). `null` when nothing is blocking.
  operationDisabledReason: string | null;
  branches: { name: string; isCurrent: boolean }[];
  /** Expands and scrolls to the section (`useSidebarReveal`, #494). */
  revealRequest?: number;
}) {
  const [githubAccount] = useForgeAccount("GitHub");
  const [bitbucketAccount] = useForgeAccount("Bitbucket");
  if (forgeRepositories.length === 0) {
    return (
      <AccordionSection
        title="Pull requests"
        storageKey="sidebar-pull-requests"
        icon={GitPullRequest}
        revealRequest={revealRequest}
      >
        <p className={styles.empty}>
          No supported GitHub or Bitbucket remotes. Add one in the Branches section (Add remote) to see its pull requests here.
        </p>
      </AccordionSection>
    );
  }

  // Only connected repositories' rows count: a disconnected one's rows are hidden.
  const connected = (provider: ForgeProvider) => (provider === "GitHub" ? githubAccount : bitbucketAccount) !== null;
  const listed = forgeRepositories.filter(
    (repository) => connected(repository.provider) && pullRequests[repository.remoteName] !== undefined,
  );
  const totalPullRequests = listed.reduce(
    (sum, repository) => sum + pullRequests[repository.remoteName].pullRequests.length,
    0,
  );

  // The inner AccordionSection per forge repository is intentional: each one is a card titled
  // with its own provider/owner/remote, nested inside this section's AccordionSection body, in
  // its own AccordionGroup so its roving-tabindex nav stays scoped to just the repo cards.
  return (
    <>
      <AccordionSection
        title="Pull requests"
        storageKey="sidebar-pull-requests"
        icon={GitPullRequest}
        revealRequest={revealRequest}
        count={listed.length > 0 ? totalPullRequests : undefined}
      >
        <AccordionGroup>
          <div className={styles.sections}>
            {forgeRepositories.map((repository) => (
              <ForgeRepositorySection
                key={repository.remoteName}
                repository={repository}
                pullRequests={pullRequests[repository.remoteName]}
                onListPullRequests={onListPullRequests}
                onForgetForgeToken={onForgetForgeToken}
                onSaveForgeToken={onSaveForgeToken}
                onCreatePullRequest={onCreatePullRequest}
                onOpenExternalUrl={onOpenExternalUrl}
                operationDisabled={operationDisabled}
                operationDisabledReason={operationDisabledReason}
                branches={branches}
              />
            ))}
          </div>
        </AccordionGroup>
      </AccordionSection>
      <datalist id={BRANCH_SUGGESTIONS_ID}>
        {branches.map((branch) => (
          <option key={branch.name} value={branch.name} />
        ))}
      </datalist>
    </>
  );
}
