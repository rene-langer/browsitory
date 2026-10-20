import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CommitSignature, GraphCommit, RepoClient } from "../ipc/RepoClient";
import { CommitHeader } from "./CommitHeader";

const FULL_ID = "abc1234567890abcdef1234567890abcdef12345";
const PARENT_ID = "def4567890abcdef1234567890abcdef12345678";

const commit: GraphCommit = {
  id: FULL_ID,
  shortId: "abc1234",
  summary: "Fix the thing",
  authorName: "Jane Doe",
  authorEmail: "jane@example.com",
  timestamp: 1_700_000_000,
  parentIds: [PARENT_ID],
  branchRefs: [],
  remoteBranchRefs: [],
};

const UNSIGNED: CommitSignature = { status: "unsigned", format: null, signer: null, key: null, detail: null };

function clientWithMessage(
  message: string,
  signature: CommitSignature | Promise<CommitSignature> | Error = UNSIGNED,
): RepoClient {
  return {
    getCommitMessage: vi.fn(async () => message),
    getCommitSignature: vi.fn(async () => {
      if (signature instanceof Error) throw signature;
      return signature;
    }),
  } as unknown as RepoClient;
}

function renderHeader(overrides: Partial<React.ComponentProps<typeof CommitHeader>> = {}) {
  return render(
    <CommitHeader
      repoPath="/repo"
      client={clientWithMessage("Fix the thing\n")}
      commitId={FULL_ID}
      commit={commit}
      knownCommitIds={new Set()}
      onSelectRow={() => {}}
      {...overrides}
    />,
  );
}

describe("CommitHeader", () => {
  it("shows author, full SHA, subject, body and parents", async () => {
    renderHeader({
      client: clientWithMessage("Fix the thing\n\nLonger explanation\nover two lines.\n"),
      knownCommitIds: new Set([PARENT_ID]),
    });

    expect(screen.getByText(/Jane Doe/)).toBeInTheDocument();
    expect(screen.getByText(/jane@example.com/)).toBeInTheDocument();
    expect(screen.getByText(FULL_ID)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Fix the thing" })).toBeInTheDocument();
    expect(await screen.findByText(/Longer explanation/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Go to parent ${PARENT_ID.slice(0, 7)}` })).toBeInTheDocument();
  });

  it("selects a parent commit when its link is clicked", async () => {
    const onSelectRow = vi.fn();
    renderHeader({ knownCommitIds: new Set([PARENT_ID]), onSelectRow });

    fireEvent.click(screen.getByRole("button", { name: /Go to parent/ }));

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: PARENT_ID });
  });

  it("renders a parent that is not in the loaded history as plain text", () => {
    renderHeader();

    expect(screen.queryByRole("button", { name: /Go to parent/ })).toBeNull();
    expect(screen.getByText(PARENT_ID.slice(0, 7))).toBeInTheDocument();
  });

  it("copies the full SHA", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    renderHeader();

    fireEvent.click(screen.getByRole("button", { name: "Copy full SHA" }));

    await waitFor(() => expect(writeText).toHaveBeenCalledWith(FULL_ID));
  });

  describe("signature row", () => {
    it("is hidden for an unsigned commit", async () => {
      renderHeader();
      await screen.findByText(FULL_ID);
      await waitFor(() => expect(screen.queryByText("Checking signature…")).not.toBeInTheDocument());
      expect(screen.queryByText("Signature")).not.toBeInTheDocument();
    });

    it("shows a good signature with signer and short key", async () => {
      renderHeader({
        client: clientWithMessage("Fix the thing\n", {
          status: "good",
          format: "openpgp",
          signer: "Jane Doe <jane@example.com>",
          key: "0123456789ABCDEF0123456789ABCDEF01234567",
          detail: null,
        }),
      });
      expect(await screen.findByText("Good signature")).toBeInTheDocument();
      expect(screen.getByText("Signature")).toBeInTheDocument();
      expect(screen.getByText(/Jane Doe <jane@example.com>/, { selector: "span" })).toBeInTheDocument();
      expect(screen.getByText("89ABCDEF01234567")).toHaveAttribute(
        "title",
        "0123456789ABCDEF0123456789ABCDEF01234567",
      );
    });

    it("shows the untrusted warning with the detail as a tooltip", async () => {
      renderHeader({
        client: clientWithMessage("Fix the thing\n", {
          status: "goodUntrusted",
          format: "ssh",
          signer: null,
          key: "SHA256:abcdefghijklmnop",
          detail: "signer is not in gpg.ssh.allowedSignersFile",
        }),
      });
      const pill = await screen.findByText("Valid signature, signer not trusted");
      expect(pill.closest("[data-tone]")).toHaveAttribute("data-tone", "warning");
      expect(pill.closest("[title]")).toHaveAttribute("title", "signer is not in gpg.ssh.allowedSignersFile");
    });

    it("marks a bad signature as danger", async () => {
      renderHeader({
        client: clientWithMessage("Fix the thing\n", {
          status: "bad", format: "openpgp", signer: null, key: null, detail: null,
        }),
      });
      const pill = await screen.findByText("Bad signature");
      expect(pill.closest("[data-tone]")).toHaveAttribute("data-tone", "danger");
    });

    it("shows the error status text", async () => {
      renderHeader({
        client: clientWithMessage("Fix the thing\n", {
          status: "error", format: "openpgp", signer: null, key: null, detail: "gpg was not found",
        }),
      });
      expect(await screen.findByText("Signature could not be checked")).toBeInTheDocument();
    });

    it("shows a loading text while checking", async () => {
      renderHeader({ client: clientWithMessage("Fix the thing\n", new Promise(() => {})) });
      expect(await screen.findByText("Checking signature…")).toBeInTheDocument();
    });

    it("hides the row when the fetch fails", async () => {
      renderHeader({ client: clientWithMessage("Fix the thing\n", new Error("boom")) });
      await waitFor(() => expect(screen.queryByText("Checking signature…")).not.toBeInTheDocument());
      expect(screen.queryByText("Signature")).not.toBeInTheDocument();
    });
  });
});
