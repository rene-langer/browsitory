import { useRef, useState } from "react";
import type {
  CreatePullRequest,
  ForgeProvider,
  ForgeRepository,
  PullRequest,
} from "../ipc/RepoClient";

interface ForgeRepositorySectionProps {
  repository: ForgeRepository;
  // `null` means "not the currently active listing" — the section renders no rows even if the
  // shared `pullRequests` array in app state happens to still hold data from a previous listing
  // (a different remote, or this remote before its token was forgotten). See `PullRequestPanel`
  // below for how `activeRemote` decides this.
  pullRequests: PullRequest[] | null;
  onListPullRequests: (remoteName: string, account: string) => Promise<void>;
  onSaveForgeToken: (provider: ForgeProvider, account: string, token: string) => Promise<void>;
  onForgetForgeToken: (provider: ForgeProvider, account: string) => Promise<void>;
  onCreatePullRequest: (remoteName: string, account: string, pullRequest: CreatePullRequest) => Promise<void>;
  operationDisabled: boolean;
}

function ForgeRepositorySection({
  repository,
  pullRequests,
  onListPullRequests,
  onSaveForgeToken,
  onForgetForgeToken,
  onCreatePullRequest,
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
      await onCreatePullRequest(repository.remoteName, acct, {
        title: trimmedTitle,
        description: description.trim() === "" ? null : description.trim(),
        sourceBranch: trimmedSource,
        targetBranch: trimmedTarget,
      });
      setTitle("");
      setDescription("");
      setSourceBranch("");
      setTargetBranch("");
    } finally {
      setCreating(false);
    }
  };

  const headingId = `pull-request-section-${repository.remoteName}`;

  return (
    <section aria-labelledby={headingId}>
      <h3 id={headingId}>
        {repository.provider}: {repository.owner}/{repository.name} ({repository.remoteName})
      </h3>

      <form onSubmit={submitToken} aria-label={`Forge token for ${repository.remoteName}`}>
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
        <button type="submit" disabled={operationDisabled}>Save token</button>
        <button type="button" disabled={operationDisabled} onClick={() => void forgetToken()}>
          Forget token
        </button>
        <button type="button" onClick={cancelToken}>Cancel</button>
      </form>

      <button type="button" disabled={listing} onClick={() => void listPullRequests()}>
        List pull requests
      </button>
      {pullRequests !== null && (
        <ul aria-label={`Pull requests for ${repository.remoteName}`}>
          {pullRequests.map((pullRequest) => (
            <li key={pullRequest.id}>
              #{pullRequest.number} {pullRequest.title} ({pullRequest.state}){" "}
              {pullRequest.sourceBranch} → {pullRequest.targetBranch} by {pullRequest.author}{" "}
              <a href={pullRequest.url}>{pullRequest.url}</a>
            </li>
          ))}
        </ul>
      )}

      <form onSubmit={submitCreate} aria-label={`Create pull request for ${repository.remoteName}`}>
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
        <button type="submit" disabled={operationDisabled || creating}>Create pull request</button>
      </form>
    </section>
  );
}

export function PullRequestPanel({
  forgeRepositories,
  pullRequests,
  onListPullRequests,
  onSaveForgeToken,
  onForgetForgeToken,
  onCreatePullRequest,
  operationDisabled,
}: {
  forgeRepositories: ForgeRepository[];
  pullRequests: PullRequest[];
  onListPullRequests: (remoteName: string, account: string) => Promise<void>;
  onSaveForgeToken: (provider: ForgeProvider, account: string, token: string) => Promise<void>;
  onForgetForgeToken: (provider: ForgeProvider, account: string) => Promise<void>;
  onCreatePullRequest: (remoteName: string, account: string, pullRequest: CreatePullRequest) => Promise<void>;
  operationDisabled: boolean;
}) {
  // Which forge repository's `pullRequests` prop is currently valid to show. `state.pullRequests`
  // in app state is a single flat list shared by every remote (see useAppState.ts's
  // `listPullRequests`/`createPullRequest`), so switching which remote's list was last requested,
  // or forgetting the token backing the currently-shown list, has to be tracked here to avoid
  // displaying another remote's stale pull requests under this one's heading.
  const [activeRemote, setActiveRemote] = useState<string | null>(null);

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
          pullRequests={activeRemote === repository.remoteName ? pullRequests : null}
          onListPullRequests={async (remoteName, account) => {
            setActiveRemote(remoteName);
            await onListPullRequests(remoteName, account);
          }}
          onForgetForgeToken={async (provider, account) => {
            await onForgetForgeToken(provider, account);
            setActiveRemote((current) => (current === repository.remoteName ? null : current));
          }}
          onSaveForgeToken={onSaveForgeToken}
          onCreatePullRequest={onCreatePullRequest}
          operationDisabled={operationDisabled}
        />
      ))}
    </section>
  );
}
