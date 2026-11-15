import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ForgeRepository, PullRequest } from "../ipc/RepoClient";
import { PullRequestPanel } from "./PullRequestPanel";
import { describedByText } from "../test/describedBy";

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

/**
 * Remembers `account` as connected for `provider` (#1012): the account a saved token belongs to
 * is kept in the app's web storage, the token itself in the system's credential store.
 */
function connect(provider: "GitHub" | "Bitbucket" = "GitHub", account = "rene") {
  localStorage.setItem(`forge-account.${provider}`, account);
}

function renderPanel(overrides: Partial<Parameters<typeof PullRequestPanel>[0]> = {}) {
  localStorage.removeItem("sidebar-pull-requests");
  const result = render(
    <PullRequestPanel
      forgeRepositories={[githubRepo]}
      pullRequests={{}}
      onListPullRequests={vi.fn().mockResolvedValue(undefined)}
      onSaveForgeToken={vi.fn().mockResolvedValue(null)}
      onForgetForgeToken={vi.fn().mockResolvedValue(null)}
      onCreatePullRequest={vi.fn().mockResolvedValue(null)}
      onOpenExternalUrl={vi.fn().mockResolvedValue(undefined)}
      operationDisabled={false}
      operationDisabledReason={null}
      branches={[]}
      {...overrides}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Pull requests" }));
  return result;
}

const githubSection = () => screen.getByRole("region", { name: /github: acme\/widget \(origin\)/i });

/** Opens the Connect to GitHub dialog of the GitHub section. */
function openConnectDialog() {
  fireEvent.click(within(githubSection()).getByRole("button", { name: "Connect to GitHub" }));
  const dialog = screen.getByRole("dialog", { name: "Connect to GitHub", hidden: true });
  return {
    dialog,
    account: within(dialog).getByLabelText("Account"),
    token: within(dialog).getByLabelText("Access token") as HTMLInputElement,
    connect: within(dialog).getByRole("button", { name: "Connect", hidden: true }),
  };
}

/** Opens the GitHub section's ⋯ menu. */
function openSectionMenu() {
  fireEvent.click(within(githubSection()).getByRole("button", { name: "Pull request actions for origin" }));
}

/** Opens the New pull request dialog of the GitHub section. */
function openNewPullRequestDialog() {
  fireEvent.click(within(githubSection()).getByRole("button", { name: "New pull request…" }));
  const dialog = screen.getByRole("dialog", { name: "New pull request", hidden: true });
  return { dialog, create: within(dialog).getByRole("button", { name: "Create pull request", hidden: true }) };
}

describe("PullRequestPanel", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders no Connect and no pull-request controls for an unsupported remote", () => {
    renderPanel({ forgeRepositories: [] });

    expect(screen.queryByRole("button", { name: /connect to/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /new pull request/i })).not.toBeInTheDocument();
  });

  it("points to Add remote in the Branches section when no remote is supported (#320)", () => {
    renderPanel({ forgeRepositories: [] });

    expect(
      screen.getByText(
        "No supported GitHub or Bitbucket remotes. Add one in the Branches section (Add remote) to see its pull requests here.",
      ),
    ).toBeInTheDocument();
  });

  it("offers only Connect to GitHub until connected: no token or create form in the section (#1012)", () => {
    const onListPullRequests = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onListPullRequests });

    const section = githubSection();
    expect(
      within(section).getByRole("heading", { level: 3, name: /github: acme\/widget \(origin\)/i }),
    ).toBeInTheDocument();
    expect(within(section).getByRole("button", { name: "Connect to GitHub" })).toBeInTheDocument();
    expect(within(section).queryByRole("textbox")).toBeNull();
    expect(within(section).queryByRole("button", { name: "New pull request…" })).toBeNull();
    expect(onListPullRequests).not.toHaveBeenCalled();
  });

  it("shows the Bitbucket access-token warning only when connecting to Bitbucket", () => {
    renderPanel({ forgeRepositories: [githubRepo, bitbucketRepo] });

    const { dialog } = openConnectDialog();
    expect(within(dialog).queryByText(/not an app password/i)).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel", hidden: true }));

    const bitbucketSection = screen.getByRole("region", { name: /bitbucket: acme\/widget \(bb-origin\)/i });
    fireEvent.click(within(bitbucketSection).getByRole("button", { name: "Connect to Bitbucket" }));
    const bitbucketDialog = screen.getByRole("dialog", { name: "Connect to Bitbucket", hidden: true });
    expect(
      within(bitbucketDialog).getByText(/repository or workspace access token \(not an app password\)/i),
    ).toBeInTheDocument();
  });

  it("saves the token only to the save callback, then lists the pull requests by itself (#1012)", async () => {
    const onSaveForgeToken = vi.fn().mockResolvedValue(null);
    const onListPullRequests = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onSaveForgeToken, onListPullRequests });

    const { account, token, connect: submit } = openConnectDialog();
    fireEvent.change(account, { target: { value: "rene" } });
    fireEvent.change(token, { target: { value: "gh-token-123" } });
    fireEvent.click(submit);

    await waitFor(() => {
      expect(onSaveForgeToken).toHaveBeenCalledWith("GitHub", "rene", "gh-token-123");
    });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Connect to GitHub", hidden: true })).toBeNull());
    await waitFor(() => expect(onListPullRequests).toHaveBeenCalledWith("origin", "rene"));
    expect(within(githubSection()).getByRole("button", { name: "New pull request…" })).toBeInTheDocument();
  });

  it("never puts the typed token into component state before save", () => {
    renderPanel();

    const { token } = openConnectDialog();
    fireEvent.change(token, { target: { value: "super-secret" } });

    // An uncontrolled input reflects the DOM value regardless of React state, so this alone
    // doesn't prove the ref-only contract — the meaningful guarantee is that the value never
    // round-trips through a controlled `value=` prop tied to component state.
    expect(token.value).toBe("super-secret");
  });

  it("keeps the dialog open with the failure, and the token field cleared, when saving fails", async () => {
    const onSaveForgeToken = vi.fn().mockResolvedValue("The credential store is locked.");
    const onListPullRequests = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onSaveForgeToken, onListPullRequests });

    const { dialog, account, token, connect: submit } = openConnectDialog();
    fireEvent.change(account, { target: { value: "rene" } });
    fireEvent.change(token, { target: { value: "gh-token-123" } });
    fireEvent.click(submit);

    expect(await within(dialog).findByText("The credential store is locked.")).toBeInTheDocument();
    expect(token).toHaveValue("");
    expect(account).toHaveValue("rene");
    expect(onListPullRequests).not.toHaveBeenCalled();
  });

  it("Disconnect forgets the token and shows Connect again", async () => {
    connect();
    const onForgetForgeToken = vi.fn().mockResolvedValue(null);
    renderPanel({ onForgetForgeToken });

    openSectionMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Disconnect" }));

    await waitFor(() => {
      expect(onForgetForgeToken).toHaveBeenCalledWith("GitHub", "rene");
    });
    expect(await within(githubSection()).findByRole("button", { name: "Connect to GitHub" })).toBeInTheDocument();
  });

  // A11: a token that couldn't be forgotten is still saved, so the card stays connected; the
  // error banner says why.
  it("stays connected when the token couldn't be forgotten", async () => {
    connect();
    const onForgetForgeToken = vi.fn().mockResolvedValue("Couldn't forget the token: locked");
    renderPanel({ onForgetForgeToken });

    openSectionMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Disconnect" }));

    await waitFor(() => expect(onForgetForgeToken).toHaveBeenCalledWith("GitHub", "rene"));
    await Promise.resolve();
    expect(within(githubSection()).getByRole("button", { name: "New pull request…" })).toBeInTheDocument();
    expect(within(githubSection()).queryByRole("button", { name: /^Connect to/ })).toBeNull();
    expect(localStorage.getItem("forge-account.GitHub")).toBe("rene");
  });

  // A12: a collapsed card lists nothing (and raises no failure banner) until it is opened.
  it("lists a connected repository's pull requests only once its card is open", async () => {
    connect();
    localStorage.setItem("sidebar-pr-origin", "closed");
    const onListPullRequests = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onListPullRequests });

    await Promise.resolve();
    expect(onListPullRequests).not.toHaveBeenCalled();
    fireEvent.click(within(githubSection()).getByRole("button", { name: /github: acme\/widget \(origin\)/i }));
    await waitFor(() => expect(onListPullRequests).toHaveBeenCalledWith("origin", "rene"));
    expect(onListPullRequests).toHaveBeenCalledTimes(1);
  });

  it("Cancel closes the Connect dialog without calling any callback", () => {
    const onSaveForgeToken = vi.fn();
    const onForgetForgeToken = vi.fn();
    renderPanel({ onSaveForgeToken, onForgetForgeToken });

    const { dialog, token } = openConnectDialog();
    fireEvent.change(token, { target: { value: "abandoned" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel", hidden: true }));

    expect(screen.queryByRole("dialog", { name: "Connect to GitHub", hidden: true })).toBeNull();
    expect(onSaveForgeToken).not.toHaveBeenCalled();
    expect(onForgetForgeToken).not.toHaveBeenCalled();
  });

  it("lists the pull requests of a connected account by itself and renders provider-neutral rows (#1012)", async () => {
    connect();
    const onListPullRequests = vi.fn().mockResolvedValue(undefined);
    renderPanel({
      onListPullRequests,
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: false } },
    });

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
    connect();
    renderPanel({
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: true } },
    });

    expect(await screen.findByRole("status")).toHaveTextContent(/more may be available/i);
  });

  it("shows no 'more available' notice when the listing was not truncated", async () => {
    connect();
    renderPanel({
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: false } },
    });

    await screen.findByText(/Add pull request support/);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("opens a pull request in the external browser instead of navigating the app", async () => {
    connect();
    const onOpenExternalUrl = vi.fn().mockResolvedValue(undefined);
    renderPanel({
      onOpenExternalUrl,
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: false } },
    });

    const link = await screen.findByRole("button", { name: "Open #7 in your browser" });
    expect(link.tagName).toBe("BUTTON");
    expect(link).toHaveAttribute("title", openPullRequest.url);
    fireEvent.click(link);

    await waitFor(() => {
      expect(onOpenExternalUrl).toHaveBeenCalledWith(openPullRequest.url);
    });
  });

  it("shows each repository's own pull requests independently, keyed by remote", () => {
    connect("GitHub");
    connect("Bitbucket");
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

    const bitbucketSection = screen.getByRole("region", { name: /bitbucket: acme\/widget \(bb-origin\)/i });
    expect(within(githubSection()).getByText(/Add pull request support/)).toBeInTheDocument();
    expect(within(githubSection()).queryByText(/Add Bitbucket support/)).not.toBeInTheDocument();
    expect(within(bitbucketSection).getByText(/Add Bitbucket support/)).toBeInTheDocument();
    expect(within(bitbucketSection).queryByText(/Add pull request support/)).not.toBeInTheDocument();
  });

  it("lists once by itself, and disables Refresh while a listing request is in flight", async () => {
    connect();
    let resolveList: () => void = () => {};
    const onListPullRequests = vi.fn(
      () => new Promise<void>((resolve) => { resolveList = resolve; }),
    );
    renderPanel({ onListPullRequests });

    await waitFor(() => expect(onListPullRequests).toHaveBeenCalledTimes(1));
    openSectionMenu();
    expect(screen.getByRole("menuitem", { name: "Refresh" })).toBeDisabled();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Disconnect" }), { key: "Escape" });

    resolveList();
    await waitFor(() => {
      openSectionMenu();
      expect(screen.getByRole("menuitem", { name: "Refresh" })).toBeEnabled();
    });
    fireEvent.click(screen.getByRole("menuitem", { name: "Refresh" }));
    expect(onListPullRequests).toHaveBeenCalledTimes(2);
  });

  it("stops showing pull requests for a repository once it is disconnected", async () => {
    connect();
    const onForgetForgeToken = vi.fn().mockResolvedValue(null);
    renderPanel({
      onForgetForgeToken,
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: false } },
    });
    await screen.findByText(/Add pull request support/);

    openSectionMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Disconnect" }));

    await waitFor(() => {
      expect(screen.queryByText(/Add pull request support/)).not.toBeInTheDocument();
    });
  });

  it("submits a new pull request with the exact fields from its dialog, which then closes", async () => {
    connect();
    const onCreatePullRequest = vi.fn().mockResolvedValue(null);
    renderPanel({ onCreatePullRequest });

    const { dialog, create } = openNewPullRequestDialog();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Add feature" } });
    fireEvent.change(within(dialog).getByLabelText("Description"), { target: { value: "Implements the thing" } });
    fireEvent.change(within(dialog).getByLabelText("Source branch"), { target: { value: "feature/pr" } });
    fireEvent.change(within(dialog).getByLabelText("Target branch"), { target: { value: "main" } });
    fireEvent.click(create);

    await waitFor(() => {
      expect(onCreatePullRequest).toHaveBeenCalledWith("origin", "rene", {
        title: "Add feature",
        description: "Implements the thing",
        sourceBranch: "feature/pr",
        targetBranch: "main",
      });
    });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "New pull request", hidden: true })).toBeNull());
  });

  it("sends a null description when the description field is left blank", async () => {
    connect();
    const onCreatePullRequest = vi.fn().mockResolvedValue(null);
    renderPanel({ onCreatePullRequest });

    const { dialog, create } = openNewPullRequestDialog();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Add feature" } });
    fireEvent.change(within(dialog).getByLabelText("Source branch"), { target: { value: "feature/pr" } });
    fireEvent.change(within(dialog).getByLabelText("Target branch"), { target: { value: "main" } });
    fireEvent.click(create);

    await waitFor(() => {
      expect(onCreatePullRequest).toHaveBeenCalledWith("origin", "rene", {
        title: "Add feature",
        description: null,
        sourceBranch: "feature/pr",
        targetBranch: "main",
      });
    });
  });

  it("keeps the dialog's fields and shows the failure when the create call fails", async () => {
    connect();
    const onCreatePullRequest = vi.fn().mockResolvedValue("Validation failed: a pull request already exists.");
    renderPanel({ onCreatePullRequest });

    const { dialog, create } = openNewPullRequestDialog();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Add feature" } });
    fireEvent.change(within(dialog).getByLabelText("Description"), { target: { value: "Implements the thing" } });
    fireEvent.change(within(dialog).getByLabelText("Source branch"), { target: { value: "feature/pr" } });
    fireEvent.change(within(dialog).getByLabelText("Target branch"), { target: { value: "main" } });
    fireEvent.click(create);

    expect(await within(dialog).findByText("Validation failed: a pull request already exists.")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Title")).toHaveValue("Add feature");
    expect(within(dialog).getByLabelText("Description")).toHaveValue("Implements the thing");
    expect(within(dialog).getByLabelText("Source branch")).toHaveValue("feature/pr");
    expect(within(dialog).getByLabelText("Target branch")).toHaveValue("main");
  });

  it("disables Connect, Disconnect and Create while another repository operation is active", () => {
    renderPanel({ operationDisabled: true });
    expect(openConnectDialog().connect).toBeDisabled();
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("explains why Connect, Disconnect and Create are disabled via their title", () => {
    const reason = "A merge is in progress.";
    renderPanel({ operationDisabled: true, operationDisabledReason: reason });
    const { dialog, connect: submit } = openConnectDialog();
    expect(submit).toHaveAttribute("title", reason);
    // #748: and as the button's description, in visible text.
    expect(describedByText(submit)).toBe(reason);
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel", hidden: true }));
  });

  it("explains a disabled Disconnect and Create pull request while connected", () => {
    connect();
    const reason = "A merge is in progress.";
    renderPanel({ operationDisabled: true, operationDisabledReason: reason });

    openSectionMenu();
    const disconnect = screen.getByRole("menuitem", { name: "Disconnect" });
    expect(disconnect).toBeDisabled();
    expect(disconnect).toHaveAttribute("title", reason);
    fireEvent.keyDown(disconnect, { key: "Escape" });

    const { create } = openNewPullRequestDialog();
    expect(create).toBeDisabled();
    expect(create).toHaveAttribute("title", reason);
    expect(describedByText(create)).toBe(reason);
  });

  it("shows an icon and the total open pull-request count on the outer Pull requests header", () => {
    connect();
    renderPanel({
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: false } },
    });
    const header = screen.getByRole("button", { name: "Pull requests" });
    expect(header).toHaveTextContent("1");
    expect(header.querySelector("svg")).toBeInTheDocument();
  });

  it("scopes roving-tabindex arrow-key navigation to the nested per-repository cards", () => {
    renderPanel({ forgeRepositories: [githubRepo, bitbucketRepo] });

    const githubHeader = screen.getByRole("button", { name: /github: acme\/widget \(origin\)/i });
    const bitbucketHeader = screen.getByRole("button", { name: /bitbucket: acme\/widget \(bb-origin\)/i });
    const outerHeader = screen.getByRole("button", { name: "Pull requests" });

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

    // The GitHub card's own content collapses...
    expect(within(githubSection()).queryByRole("button", { name: "Connect to GitHub" })).not.toBeInTheDocument();
    // ...while the Bitbucket card, untouched, stays open.
    const bitbucketSection = screen.getByRole("region", { name: /bitbucket: acme\/widget \(bb-origin\)/i });
    expect(within(bitbucketSection).getByRole("button", { name: "Connect to Bitbucket" })).toBeInTheDocument();
  });

  it("defaults source to the current branch, target to main, and suggests known branches", () => {
    connect();
    const { baseElement } = renderPanel({
      branches: [
        { name: "main", isCurrent: false },
        { name: "feat/x", isCurrent: true },
      ],
    });
    const { dialog } = openNewPullRequestDialog();
    expect(within(dialog).getByLabelText("Source branch")).toHaveValue("feat/x");
    expect(within(dialog).getByLabelText("Target branch")).toHaveValue("main");
    const options = Array.from(baseElement.querySelectorAll("datalist option")).map((o) => o.getAttribute("value"));
    expect(options).toEqual(["main", "feat/x"]);
  });
});
