import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderTree } from "../test/branchTreeProps";
import { BranchTreeWithDialogs } from "../test/BranchTreeWithDialogs";
import { describedByText } from "../test/describedBy";

const oneRemote = [{ name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null }];
const up = { localBranch: "main", remoteName: "origin", remoteBranch: "main" };

describe("SetUpstreamDialog (in BranchTree)", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("Set upstream… on the current branch's context menu opens a dialog; submitting calls onSetUpstream", async () => {
    const onSetUpstream = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onSetUpstream });
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    fireEvent.click(screen.getByRole("menuitem", { name: "Set upstream…" }));
    const dialog = await screen.findByRole("dialog", { name: "Set upstream for main" });
    fireEvent.change(within(dialog).getByLabelText("Upstream remote"), { target: { value: "origin" } });
    fireEvent.change(within(dialog).getByLabelText("Upstream branch"), { target: { value: "main" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Set upstream" }));
    expect(onSetUpstream).toHaveBeenCalledWith("origin", "main");
  });

  it("offers branch suggestions from the selected remote for the upstream-branch field", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main", "develop"]);
    renderTree({ remotes: oneRemote, onListRemoteBranches });
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    fireEvent.click(screen.getByRole("menuitem", { name: "Set upstream…" }));
    const dialog = await screen.findByRole("dialog", { name: "Set upstream for main" });
    fireEvent.change(within(dialog).getByLabelText("Upstream remote"), { target: { value: "origin" } });
    await waitFor(() => expect(onListRemoteBranches).toHaveBeenCalledWith("origin"));
    const branchInput = within(dialog).getByLabelText("Upstream branch");
    const datalistId = branchInput.getAttribute("list");
    expect(datalistId).not.toBeNull();
    const datalist = document.getElementById(datalistId!);
    expect(datalist).not.toBeNull();
    const optionValues = Array.from(datalist!.querySelectorAll("option")).map((option) =>
      option.getAttribute("value"),
    );
    expect(optionValues).toEqual(["main", "develop"]);
  });

  it("blocks clearing and setting the upstream, saying why, while the repository is busy (#521)", async () => {
    const reason = "A transfer is in progress.";
    const { props, rerender } = renderTree({ upstream: up });
    fireEvent.click(screen.getByRole("button", { name: "Clear upstream" }));
    await screen.findByRole("dialog", { name: "Clear upstream confirmation", hidden: true });

    rerender(<BranchTreeWithDialogs {...props} operationDisabled operationDisabledReason={reason} />);
    const dialog = screen.getByRole("dialog", { name: "Clear upstream confirmation", hidden: true });
    expect(dialog).toHaveTextContent(reason);
    const confirm = within(dialog).getByRole("button", { name: "Clear upstream", hidden: true });
    expect(confirm).toBeDisabled();
    fireEvent.click(confirm);
    expect(props.onClearUpstream).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel", hidden: true }));

    expect(screen.getByRole("button", { name: "Clear upstream" })).toHaveAttribute("aria-disabled", "true");
    expect(describedByText(screen.getByRole("button", { name: "Clear upstream" }))).toBe(reason);
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    const setItem = screen.getByRole("menuitem", { name: "Set upstream…" });
    expect(setItem).toBeDisabled();
    expect(setItem).toHaveAttribute("title", reason);
  });
});
