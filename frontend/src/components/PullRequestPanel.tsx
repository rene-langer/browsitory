import { useRef, useState } from "react";
import { ExternalLink, GitPullRequest } from "lucide-react";
import type {
  CreatePullRequest,
  ForgeProvider,
  ForgeRepository,
  PullRequestList,
} from "../ipc/RepoClient";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./PullRequestPanel.module.css";

interface ForgeRepositorySectionProps {
  repository: ForgeRepository;
  // `undefined` means "never listed for this remote yet". A per-remote entry — see
  // `useAppState.ts`'s `AppState.pullRequests`, keyed by `remoteName` so listing/creating
  // against one remote can never clobber or hide another remote's rows.
  pullRequests: PullRequestList | undefined;
  onListPullRequests: (remoteName: string, account: string) => Promise<void>;
  onSaveForgeToken: (provider: ForgeProvider, account: string, token: string) => Promise<void>;
  onForgetForgeToken: (provider: ForgeProvider, account: string) => Promise<void>;
  // Resolves `true` only when the create actually succeeded (see `useAppState.ts`'s
  // `createPullRequest` doc comment) — `submitCreate` below uses this to decide whether the
  // form is safe to clear, per the brief's "clears only the non-secret form fields on success".
  onCreatePullRequest: (remoteName: string, account: string, pullRequest: CreatePullRequest) => Promise<boolean>;
  onOpenExternalUrl: (url: string) => Promise<void>;
  operationDisabled: boolean;
}

function ForgeRepositorySection({
  repository,
  pullRequests,
  onListPullRequests,
  onSaveForgeToken,
  onForgetForgeToken,
  onCreatePullRequest,
  onOpenExternalUrl,
  operationDisabled,
}: ForgeRepositorySectionProps) {
  const tokenRef = useRef<HTMLInputElement>(null);
  const [account, setAccount] = useState("");
  const [listing, setListing] = useState(false);
  const [creating, setCreating] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [sourceBranch, setSourceBranch] = useState("");
  const [targetBranch, setTargetBranch] = useState("");
  // Hides this section's rows immediately after its token is forgotten, even though
  // `state.pullRequests[repository.remoteName]` itself isn't cleared until the next list call —
  // showing rows fetched with a token the user just forgot would be misleading. Cleared again by
  // the next successful/attempted listing.
  const [tokenForgotten, setTokenForgotten] = useState(false);

  const clearToken = () => {
    if (tokenRef.current !== null) tokenRef.current.value = "";
  };

  const submitToken = async (event: React.FormEvent) => {
    event.preventDefault();
    const acct = account.trim();
    const token = tokenRef.current?.value ?? "";
    if (acct === "" || token === "") return;
    try {
      await onSaveForgeToken(repository.provider, acct, token);
    } finally {
      clearToken();
    }
  };

  const forgetToken = async () => {
    const acct = account.trim();
    if (acct === "") {
      clearToken();
      return;
    }
    try {
      await onForgetForgeToken(repository.provider, acct);
      setTokenForgotten(true);
    } finally {
      clearToken();
    }
  };

  const cancelToken = () => {
    clearToken();
  };

  const listPullRequests = async () => {
    const acct = account.trim();
    if (acct === "" || listing) return;
    setListing(true);
    try {
      await onListPullRequests(repository.remoteName, acct);
      setTokenForgotten(false);
    } finally {
      setListing(false);
    }
  };

  const submitCreate = async (event: React.FormEvent) => {
    event.preventDefault();
    const acct = account.trim();
    const trimmedTitle = title.trim();
    const trimmedSource = sourceBranch.trim();
    const trimmedTarget = targetBranch.trim();
    if (acct === "" || trimmedTitle === "" || trimmedSource === "" || trimmedTarget === "" || creating) {
      return;
    }
    setCreating(true);
    try {
      const succeeded = await onCreatePullRequest(repository.remoteName, acct, {
        title: trimmedTitle,
        description: description.trim() === "" ? null : description.trim(),
        sourceBranch: trimmedSource,
        targetBranch: trimmedTarget,
      });
      // Only clear the form when the create actually went through — `useAppState.ts`'s
      // `createPullRequest` swallows a failed request into `state.error` rather than
      // rejecting, so `succeeded` is the only way to tell a failure from a success here. A
      // failed submission must leave the user's typed title/description/branches in place
      // rather than silently discarding them.
      if (succeeded) {
        setTitle("");
        setDescription("");
        setSourceBranch("");
        setTargetBranch("");
      }
    } finally {
      setCreating(false);
    }
  };

  // The Panel's identifying title/region-name — provider, owner/name, and remoteName together,
  // since remoteName is the true disambiguator (a repo can be listed under multiple
  // remotes/providers with identical owner/name, per `AppState.pullRequests`' per-remote
  // keying above). `ForgeRepository` has no single "slug" field (confirmed against
  // `RepoClient.ts`: provider/host/owner/name/remoteName), so this composes the closest
  // equivalent — matching the identifying text the section heading showed before this reskin.
  const sectionLabel = `${repository.provider}: ${repository.owner}/${repository.name} (${repository.remoteName})`;
  const visibleRows = tokenForgotten ? null : (pullRequests?.pullRequests ?? null);

  return (
    <Panel title={sectionLabel} ariaLabel={sectionLabel}>
      <form className={styles.form} onSubmit={submitToken} aria-label={`Forge token for ${repository.remoteName}`}>
        <label>
          Account
          <input value={account} onChange={(event) => setAccount(event.target.value)} autoComplete="off" />
        </label>
        <label>
          Access token
          <input ref={tokenRef} type="password" autoComplete="off" />
        </label>
        {repository.provider === "Bitbucket" && (
          <p>
            Requires a Bitbucket repository or workspace access token (not an app password).
          </p>
        )}
        <Toolbar>
          <button type="submit" disabled={operationDisabled}>Save token</button>
          <button type="button" disabled={operationDisabled} onClick={() => void forgetToken()}>
            Forget token
          </button>
          <button type="button" onClick={cancelToken}>Cancel</button>
        </Toolbar>
      </form>

      <Toolbar>
        <button type="button" disabled={listing} onClick={() => void listPullRequests()}>
          List pull requests
        </button>
      </Toolbar>
      {visibleRows !== null && (
        <>
          {!tokenForgotten && pullRequests?.truncated === true && (
            <p role="status">
              Showing the first {visibleRows.length} pull requests — more may be available on the provider.
            </p>
          )}
          <ul className={styles.prList} aria-label={`Pull requests for ${repository.remoteName}`}>
            {visibleRows.map((pullRequest) => (
              <li key={pullRequest.id} className={styles.prRow}>
                <GitPullRequest size={14} aria-hidden="true" />
                #{pullRequest.number} {pullRequest.title} ({pullRequest.state}){" "}
                {pullRequest.sourceBranch} → {pullRequest.targetBranch} by {pullRequest.author}{" "}
                <button type="button" onClick={() => void onOpenExternalUrl(pullRequest.url)}>
                  <ExternalLink size={14} aria-hidden="true" />
                  {pullRequest.url}
                </button>
              </li>
            ))}
          </ul>
        </>
      )}

      <form className={styles.form} onSubmit={submitCreate} aria-label={`Create pull request for ${repository.remoteName}`}>
        <label>
          Title
          <input value={title} onChange={(event) => setTitle(event.target.value)} />
        </label>
        <label>
          Description
          <textarea value={description} onChange={(event) => setDescription(event.target.value)} />
        </label>
        <label>
          Source branch
          <input value={sourceBranch} onChange={(event) => setSourceBranch(event.target.value)} />
        </label>
        <label>
          Target branch
          <input value={targetBranch} onChange={(event) => setTargetBranch(event.target.value)} />
        </label>
        <Toolbar>
          <button type="submit" disabled={operationDisabled || creating}>Create pull request</button>
        </Toolbar>
      </form>
    </Panel>
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
}: {
  forgeRepositories: ForgeRepository[];
  pullRequests: Record<string, PullRequestList>;
  onListPullRequests: (remoteName: string, account: string) => Promise<void>;
  onSaveForgeToken: (provider: ForgeProvider, account: string, token: string) => Promise<void>;
  onForgetForgeToken: (provider: ForgeProvider, account: string) => Promise<void>;
  // Resolves `true` only when the create actually succeeded (see `useAppState.ts`'s
  // `createPullRequest` doc comment); forwarded to each `ForgeRepositorySection`, whose
  // `submitCreate` uses it to decide whether the form is safe to clear, per the brief's
  // "clears only the non-secret form fields on success".
  onCreatePullRequest: (remoteName: string, account: string, pullRequest: CreatePullRequest) => Promise<boolean>;
  onOpenExternalUrl: (url: string) => Promise<void>;
  operationDisabled: boolean;
}) {
  if (forgeRepositories.length === 0) {
    return (
      <section aria-labelledby="pull-request-panel-heading">
        <h2 id="pull-request-panel-heading">Pull Requests</h2>
        <p>No supported GitHub or Bitbucket remotes detected.</p>
      </section>
    );
  }

  return (
    <section aria-labelledby="pull-request-panel-heading">
      <h2 id="pull-request-panel-heading">Pull Requests</h2>
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
        />
      ))}
    </section>
  );
}
