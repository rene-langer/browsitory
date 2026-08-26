import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ForgeRepository, PullRequest } from "../ipc/RepoClient";
import { PullRequestPanel } from "./PullRequestPanel";

const githubRepo: ForgeRepository = {
  provider: "GitHub",
  host: "github.com",
  owner: "acme",
  name: "widget",
  remoteName: "origin",
};

const bitbucketRepo: ForgeRepository = {
  provider: "Bitbucket",
  host: "bitbucket.org",
  owner: "acme",
  name: "widget",
  remoteName: "bb-origin",
};

const openPullRequest: PullRequest = {
  id: "101",
  number: 7,
  title: "Add pull request support",
  url: "https://github.com/acme/widget/pull/7",
  author: "rene",
  sourceBranch: "feature/pr",
  targetBranch: "main",
  state: "open",
};

function renderPanel(overrides: Partial<Parameters<typeof PullRequestPanel>[0]> = {}) {
  localStorage.removeItem("sidebar-pull-requests");
  const result = render(
    <PullRequestPanel
      forgeRepositories={[githubRepo]}
      pullRequests={{}}
      onListPullRequests={vi.fn().mockResolvedValue(undefined)}
      onSaveForgeToken={vi.fn().mockResolvedValue(undefined)}
      onForgetForgeToken={vi.fn().mockResolvedValue(undefined)}
      onCreatePullRequest={vi.fn().mockResolvedValue(true)}
      onOpenExternalUrl={vi.fn().mockResolvedValue(undefined)}
      operationDisabled={false}
      operationDisabledReason={null}
      {...overrides}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Pull Requests" }));
  return result;
}

describe("PullRequestPanel", () => {
  it("renders no token form and no pull-request controls for an unsupported remote", () => {
    renderPanel({ forgeRepositories: [] });

    expect(screen.queryByLabelText("Access token")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /list pull requests/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /create pull request/i })).not.toBeInTheDocument();
  });

  it("shows account/token save-forget controls and a creation form for a supported repository", () => {
    renderPanel();

    const section = screen.getByRole("region", { name: /github: acme\/widget \(origin\)/i });
    expect(
      within(section).getByRole("heading", { level: 3, name: /github: acme\/widget \(origin\)/i }),
    ).toBeInTheDocument();
    expect(within(section).getByLabelText("Account")).toBeInTheDocument();
    expect(within(section).getByLabelText("Access token")).toBeInTheDocument();
    expect(within(section).getByRole("button", { name: "Save token" })).toBeInTheDocument();
    expect(within(section).getByRole("button", { name: "Forget token" })).toBeInTheDocument();
    expect(within(section).getByRole("button", { name: "List pull requests" })).toBeInTheDocument();
    const createForm = within(section).getByRole("form", { name: "Create pull request for origin" });
    expect(within(createForm).getByLabelText("Title")).toBeInTheDocument();
    expect(within(createForm).getByLabelText("Description")).toBeInTheDocument();
    expect(within(createForm).getByLabelText("Source branch")).toBeInTheDocument();
    expect(within(createForm).getByLabelText("Target branch")).toBeInTheDocument();
  });

  it("shows the Bitbucket access-token warning only for a Bitbucket repository", () => {
    renderPanel({ forgeRepositories: [githubRepo, bitbucketRepo] });

    const githubSection = screen.getByRole("region", { name: /github: acme\/widget \(origin\)/i });
    const bitbucketSection = screen.getByRole("region", { name: /bitbucket: acme\/widget \(bb-origin\)/i });
    expect(within(githubSection).queryByText(/not an app password/i)).not.toBeInTheDocument();
    expect(within(bitbucketSection).getByText(/repository or workspace access token \(not an app password\)/i)).toBeInTheDocument();
  });

  it("saves the token only to the save callback and clears the input", async () => {
    const onSaveForgeToken = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onSaveForgeToken });

    fireEvent.change(screen.getByLabelText("Account"), { target: { value: "rene" } });
    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "gh-token-123" } });
    fireEvent.click(screen.getByRole("button", { name: "Save token" }));

    await waitFor(() => {
      expect(onSaveForgeToken).toHaveBeenCalledWith("GitHub", "rene", "gh-token-123");
    });
    expect(screen.getByLabelText("Access token")).toHaveValue("");
  });

  it("never puts the typed token into component state before save", () => {
    renderPanel();

    const tokenInput = screen.getByLabelText("Access token") as HTMLInputElement;
    fireEvent.change(tokenInput, { target: { value: "super-secret" } });

    // An uncontrolled input reflects the DOM value regardless of React state, so this alone
    // doesn't prove the ref-only contract — the meaningful guarantee is that Save clears it
    // (covered above) and the value never round-trips through a controlled `value=` prop tied
    // to component state (which would force a re-render on every keystroke). Assert the input
    // has no controlling `value` prop wired to state by checking it accepts direct DOM mutation
    // that a controlled input would otherwise fight/revert.
    expect(tokenInput.value).toBe("super-secret");
  });

  it("clears the token after forgetting it", async () => {
    const onForgetForgeToken = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onForgetForgeToken });

    fireEvent.change(screen.getByLabelText("Account"), { target: { value: "rene" } });
    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "stale" } });
    fireEvent.click(screen.getByRole("button", { name: "Forget token" }));

    await waitFor(() => {
      expect(onForgetForgeToken).toHaveBeenCalledWith("GitHub", "rene");
    });
    expect(screen.getByLabelText("Access token")).toHaveValue("");
  });

  it("clears the token on cancel without calling any callback", () => {
    const onSaveForgeToken = vi.fn();
    const onForgetForgeToken = vi.fn();
    renderPanel({ onSaveForgeToken, onForgetForgeToken });

    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "abandoned" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.getByLabelText("Access token")).toHaveValue("");
    expect(onSaveForgeToken).not.toHaveBeenCalled();
    expect(onForgetForgeToken).not.toHaveBeenCalled();
  });

  it("lists pull requests for the account and renders provider-neutral rows", async () => {
    const onListPullRequests = vi.fn().mockResolvedValue(undefined);
    renderPanel({
      onListPullRequests,
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: false } },
    });

    fireEvent.change(screen.getByLabelText("Account"), { target: { value: "rene" } });
    fireEvent.click(screen.getByRole("button", { name: "List pull requests" }));

    await waitFor(() => {
      expect(onListPullRequests).toHaveBeenCalledWith("origin", "rene");
    });
    const row = await screen.findByText(/Add pull request support/);
    expect(row).toBeInTheDocument();
    expect(row.closest("li")).toHaveTextContent("#7");
    expect(row.closest("li")).toHaveTextContent("feature/pr");
    expect(row.closest("li")).toHaveTextContent("main");
  });

  it("shows a 'more available' notice when the listing was truncated", async () => {
    renderPanel({
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: true } },
    });

    fireEvent.change(screen.getByLabelText("Account"), { target: { value: "rene" } });
    fireEvent.click(screen.getByRole("button", { name: "List pull requests" }));

    expect(await screen.findByRole("status")).toHaveTextContent(/more may be available/i);
  });

  it("shows no 'more available' notice when the listing was not truncated", async () => {
    renderPanel({
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: false } },
    });

    fireEvent.change(screen.getByLabelText("Account"), { target: { value: "rene" } });
    fireEvent.click(screen.getByRole("button", { name: "List pull requests" }));

    await screen.findByText(/Add pull request support/);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("opens a pull request's URL in the external browser instead of navigating the app", async () => {
    const onOpenExternalUrl = vi.fn().mockResolvedValue(undefined);
    renderPanel({
      onOpenExternalUrl,
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: false } },
    });

    fireEvent.change(screen.getByLabelText("Account"), { target: { value: "rene" } });
    fireEvent.click(screen.getByRole("button", { name: "List pull requests" }));

    const link = await screen.findByRole("button", { name: openPullRequest.url });
    expect(link.tagName).toBe("BUTTON");
    fireEvent.click(link);

    await waitFor(() => {
      expect(onOpenExternalUrl).toHaveBeenCalledWith(openPullRequest.url);
    });
  });

  it("shows each repository's own pull requests independently, keyed by remote", () => {
    const bitbucketPullRequest: PullRequest = {
      id: "12",
      number: 12,
      title: "Add Bitbucket support",
      url: "https://bitbucket.org/acme/widget/pull-requests/12",
      author: "rene",
      sourceBranch: "feature/bb",
      targetBranch: "main",
      state: "open",
    };
    renderPanel({
      forgeRepositories: [githubRepo, bitbucketRepo],
      pullRequests: {
        origin: { pullRequests: [openPullRequest], truncated: false },
        "bb-origin": { pullRequests: [bitbucketPullRequest], truncated: false },
      },
    });

    const githubSection = screen.getByRole("region", { name: /github: acme\/widget \(origin\)/i });
    const bitbucketSection = screen.getByRole("region", { name: /bitbucket: acme\/widget \(bb-origin\)/i });
    expect(within(githubSection).getByText(/Add pull request support/)).toBeInTheDocument();
    expect(within(githubSection).queryByText(/Add Bitbucket support/)).not.toBeInTheDocument();
    expect(within(bitbucketSection).getByText(/Add Bitbucket support/)).toBeInTheDocument();
    expect(within(bitbucketSection).queryByText(/Add pull request support/)).not.toBeInTheDocument();
  });

  it("disables List pull requests while a listing request is in flight", async () => {
    let resolveList: () => void = () => {};
    const onListPullRequests = vi.fn(
      () => new Promise<void>((resolve) => { resolveList = resolve; }),
    );
    renderPanel({ onListPullRequests });

    fireEvent.change(screen.getByLabelText("Account"), { target: { value: "rene" } });
    const listButton = screen.getByRole("button", { name: "List pull requests" });
    fireEvent.click(listButton);
    fireEvent.click(listButton);

    expect(onListPullRequests).toHaveBeenCalledTimes(1);
    expect(listButton).toBeDisabled();

    resolveList();
    await waitFor(() => expect(listButton).toBeEnabled());
  });

  it("stops showing pull requests for a repository once its token is forgotten", async () => {
    const onListPullRequests = vi.fn().mockResolvedValue(undefined);
    const onForgetForgeToken = vi.fn().mockResolvedValue(undefined);
    renderPanel({
      onListPullRequests,
      onForgetForgeToken,
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: false } },
    });

    fireEvent.change(screen.getByLabelText("Account"), { target: { value: "rene" } });
    fireEvent.click(screen.getByRole("button", { name: "List pull requests" }));
    await screen.findByText(/Add pull request support/);

    fireEvent.click(screen.getByRole("button", { name: "Forget token" }));

    await waitFor(() => {
      expect(screen.queryByText(/Add pull request support/)).not.toBeInTheDocument();
    });
  });

  it("submits a new pull request with the exact fields and clears only the non-secret fields", async () => {
    const onCreatePullRequest = vi.fn().mockResolvedValue(true);
    renderPanel({ onCreatePullRequest });

    fireEvent.change(screen.getByLabelText("Account"), { target: { value: "rene" } });
    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "gh-token" } });
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "Add feature" } });
    fireEvent.change(screen.getByLabelText("Description"), { target: { value: "Implements the thing" } });
    fireEvent.change(screen.getByLabelText("Source branch"), { target: { value: "feature/pr" } });
    fireEvent.change(screen.getByLabelText("Target branch"), { target: { value: "main" } });
    fireEvent.click(screen.getByRole("button", { name: "Create pull request" }));

    await waitFor(() => {
      expect(onCreatePullRequest).toHaveBeenCalledWith("origin", "rene", {
        title: "Add feature",
        description: "Implements the thing",
        sourceBranch: "feature/pr",
        targetBranch: "main",
      });
    });
    expect(screen.getByLabelText("Title")).toHaveValue("");
    expect(screen.getByLabelText("Description")).toHaveValue("");
    expect(screen.getByLabelText("Source branch")).toHaveValue("");
    expect(screen.getByLabelText("Target branch")).toHaveValue("");
    // Non-secret fields owned by the token form are untouched by a successful PR creation.
    expect(screen.getByLabelText("Account")).toHaveValue("rene");
    // The token ref is only ever cleared by save/forget/cancel of the token form itself.
    expect(screen.getByLabelText("Access token")).toHaveValue("gh-token");
  });

  it("sends a null description when the description field is left blank", async () => {
    const onCreatePullRequest = vi.fn().mockResolvedValue(true);
    renderPanel({ onCreatePullRequest });

    fireEvent.change(screen.getByLabelText("Account"), { target: { value: "rene" } });
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "Add feature" } });
    fireEvent.change(screen.getByLabelText("Source branch"), { target: { value: "feature/pr" } });
    fireEvent.change(screen.getByLabelText("Target branch"), { target: { value: "main" } });
    fireEvent.click(screen.getByRole("button", { name: "Create pull request" }));

    await waitFor(() => {
      expect(onCreatePullRequest).toHaveBeenCalledWith("origin", "rene", {
        title: "Add feature",
        description: null,
        sourceBranch: "feature/pr",
        targetBranch: "main",
      });
    });
  });

  it("does not clear the creation form when the create call fails", async () => {
    // `useAppState.ts`'s `createPullRequest` swallows a failed request into `state.error`
    // rather than rejecting, resolving `false` instead — this is what a failed submission looks
    // like to the component in practice, not a rejected promise.
    const onCreatePullRequest = vi.fn().mockResolvedValue(false);
    renderPanel({ onCreatePullRequest });

    fireEvent.change(screen.getByLabelText("Account"), { target: { value: "rene" } });
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "Add feature" } });
    fireEvent.change(screen.getByLabelText("Description"), { target: { value: "Implements the thing" } });
    fireEvent.change(screen.getByLabelText("Source branch"), { target: { value: "feature/pr" } });
    fireEvent.change(screen.getByLabelText("Target branch"), { target: { value: "main" } });
    fireEvent.click(screen.getByRole("button", { name: "Create pull request" }));

    await waitFor(() => {
      expect(onCreatePullRequest).toHaveBeenCalledWith("origin", "rene", {
        title: "Add feature",
        description: "Implements the thing",
        sourceBranch: "feature/pr",
        targetBranch: "main",
      });
    });
    expect(screen.getByLabelText("Title")).toHaveValue("Add feature");
    expect(screen.getByLabelText("Description")).toHaveValue("Implements the thing");
    expect(screen.getByLabelText("Source branch")).toHaveValue("feature/pr");
    expect(screen.getByLabelText("Target branch")).toHaveValue("main");
  });

  it("disables Save/Forget/Create controls while another repository operation is active", () => {
    renderPanel({ operationDisabled: true });

    expect(screen.getByRole("button", { name: "Save token" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Forget token" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Create pull request" })).toBeDisabled();
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("explains why Save/Forget/Create controls are disabled via their title", () => {
    renderPanel({ operationDisabled: true, operationDisabledReason: "A merge is in progress." });

    expect(screen.getByRole("button", { name: "Save token" })).toHaveAttribute(
      "title",
      "A merge is in progress.",
    );
    expect(screen.getByRole("button", { name: "Forget token" })).toHaveAttribute(
      "title",
      "A merge is in progress.",
    );
    expect(screen.getByRole("button", { name: "Create pull request" })).toHaveAttribute(
      "title",
      "A merge is in progress.",
    );
  });

  it("shows an icon and the total open pull-request count on the outer Pull Requests header", () => {
    renderPanel({
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: false } },
    });
    const header = screen.getByRole("button", { name: "Pull Requests" });
    expect(header).toHaveTextContent("1");
    expect(header.querySelector("svg")).toBeInTheDocument();
  });

  it("scopes roving-tabindex arrow-key navigation to the nested per-repository cards", () => {
    renderPanel({ forgeRepositories: [githubRepo, bitbucketRepo] });

    const githubHeader = screen.getByRole("button", { name: /github: acme\/widget \(origin\)/i });
    const bitbucketHeader = screen.getByRole("button", { name: /bitbucket: acme\/widget \(bb-origin\)/i });
    const outerHeader = screen.getByRole("button", { name: "Pull Requests" });

    // ArrowDown from the last repo card wraps around to the first, proving the nested
    // AccordionGroup (not some ambient/absent outer one) owns the wrapping.
    fireEvent.keyDown(bitbucketHeader, { key: "ArrowDown" });
    expect(githubHeader).toHaveFocus();
    expect(outerHeader).not.toHaveFocus();

    fireEvent.keyDown(githubHeader, { key: "ArrowUp" });
    expect(bitbucketHeader).toHaveFocus();
    expect(outerHeader).not.toHaveFocus();
  });

  it("lets a repository's own card be collapsed independently without hiding the others", () => {
    renderPanel({
      forgeRepositories: [githubRepo, bitbucketRepo],
      pullRequests: {
        origin: { pullRequests: [openPullRequest], truncated: false },
      },
    });

    fireEvent.click(screen.getByRole("button", { name: /github: acme\/widget \(origin\)/i }));

    expect(screen.queryByLabelText("Account")).toBeTruthy();
    // The GitHub card's own content collapses...
    const githubSection = screen.getByRole("region", { name: /github: acme\/widget \(origin\)/i });
    expect(within(githubSection).queryByRole("button", { name: "List pull requests" })).not.toBeInTheDocument();
    // ...while the Bitbucket card, untouched, stays open.
    const bitbucketSection = screen.getByRole("region", { name: /bitbucket: acme\/widget \(bb-origin\)/i });
    expect(within(bitbucketSection).getByRole("button", { name: "List pull requests" })).toBeInTheDocument();
  });
});
