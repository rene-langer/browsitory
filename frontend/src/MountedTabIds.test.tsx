import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { RepoClient } from "./ipc/RepoClient";
import { fakeRepoClient, graphPage } from "./test/fakeRepoClient";

// Whole-App renders, like App.test.tsx (#217's slow-machine budget).
vi.setConfig({ testTimeout: 20_000 });

const change = { path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const };

function twoTabClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(
    {
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [change],
      getCommitGraph: async () => graphPage([]),
      listStashes: async () => [],
      listBranches: async () => [{ name: "main", isCurrent: true, upstream: null, ahead: 0, behind: 0 }],
      listRemotes: async () => [],
      getCurrentUpstream: async () => null,
      ...overrides,
    },
    "empty",
  );
}

const shownWorkspace = () => document.querySelector<HTMLElement>('[data-active-repo="true"]')!;

/** The ids that more than one element in the document has. */
function duplicateIds(): string[] {
  const counts = new Map<string, number>();
  for (const element of document.querySelectorAll("[id]")) counts.set(element.id, (counts.get(element.id) ?? 0) + 1);
  return [...counts].filter(([, count]) => count > 1).map(([id]) => id);
}

async function openNewBranchForm() {
  fireEvent.keyDown(window, { key: "k", ctrlKey: true });
  fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), { target: { value: "Create branch" } });
  fireEvent.click(await screen.findByRole("option", { name: "Create branch…" }));
  return within(shownWorkspace()).findByRole("form", { name: "New branch" });
}

afterEach(() => {
  localStorage.clear();
});

describe("ids across the mounted tabs (#1135)", () => {
  it("gives every element its own id while two workspaces are mounted, with their forms open", async () => {
    const view = render(<App client={twoTabClient()} />);
    await screen.findByRole("tab", { name: /^a(,|$)/ });
    await screen.findByRole("listbox", { name: "Unstaged changes" });
    await openNewBranchForm();

    fireEvent.click(screen.getByRole("tab", { name: /^b(,|$)/ }));
    await waitFor(() => expect(view.container.querySelectorAll("[data-active-repo]")).toHaveLength(2));
    await waitFor(() => expect(shownWorkspace().getAttribute("aria-labelledby")).toBe(screen.getByRole("tab", { selected: true }).id));
    await within(shownWorkspace()).findByRole("listbox", { name: "Unstaged changes" });
    const form = await openNewBranchForm();

    expect(duplicateIds()).toEqual([]);
    // The shown tab's label names the shown tab's field.
    const field = within(form).getByPlaceholderText("New branch name");
    expect(field).toHaveAccessibleName("Branch name");
    fireEvent.click(within(form).getByText("Branch name"));
    expect(field).toHaveFocus();
  });
});
