import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { ExternalToolsSection, PreferencesDialog } from "./PreferencesDialog";

function renderTools(client: Partial<RepoClient>, toolHost: "desktop" | "vscode" = "desktop", repoPath: string | null = "/r") {
  const onClose = vi.fn();
  render(
    <PreferencesDialog onClose={onClose}>
      <ExternalToolsSection client={client as RepoClient} repoPath={repoPath} toolHost={toolHost} />
    </PreferencesDialog>,
  );
  return onClose;
}

const noTools = async () => ({ diffTool: null, mergeTool: null });

describe("PreferencesDialog", () => {
  it("is a dialog named Preferences with an External tools section", async () => {
    renderTools({ getExternalToolSettings: async () => ({ editor: null, terminal: null }), getGitTools: noTools });
    expect(screen.getByRole("dialog", { name: "Preferences" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Preferences" })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "External tools" })).toBeInTheDocument();
    await screen.findByLabelText("Editor command");
  });

  it("loads, edits and saves the editor and terminal commands, blank as null", async () => {
    const setExternalToolSettings = vi.fn(async () => {});
    renderTools({
      getExternalToolSettings: async () => ({ editor: "code", terminal: null }),
      setExternalToolSettings,
      getGitTools: noTools,
    });
    const editor = await screen.findByLabelText("Editor command");
    await waitFor(() => expect(editor).toHaveValue("code"));
    expect(editor).toHaveAttribute("placeholder", "System default app");
    const terminal = screen.getByLabelText("Terminal command");
    expect(terminal).toHaveAttribute("placeholder", "System default terminal");
    expect(screen.getByText("Placeholders: {file}, {line}, {dir}")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();

    fireEvent.change(editor, { target: { value: "  " } });
    fireEvent.change(terminal, { target: { value: " kitty --directory {dir} " } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(setExternalToolSettings).toHaveBeenCalledWith({ editor: null, terminal: "kitty --directory {dir}" }),
    );
    await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeDisabled());
  });

  it("shows a failed save", async () => {
    renderTools({
      getExternalToolSettings: async () => ({ editor: null, terminal: null }),
      setExternalToolSettings: async () => {
        throw new Error("disk full");
      },
      getGitTools: noTools,
    });
    fireEvent.change(await screen.findByLabelText("Editor command"), { target: { value: "code" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("disk full")).toBeInTheDocument();
  });

  it("shows git diff and merge tools read-only with their source and problems", async () => {
    const getGitTools = vi.fn(async () => ({
      diffTool: { name: "meld", command: "meld $LOCAL $REMOTE", source: "Global" as const, viaMergeTool: true, problem: null },
      mergeTool: {
        name: "evil",
        command: null,
        source: "Local" as const,
        viaMergeTool: false,
        problem: "merge.tool is set only in this repository's git config, which Browsitory ignores.",
      },
    }));
    renderTools({
      getExternalToolSettings: async () => ({ editor: null, terminal: null }),
      setExternalToolSettings: vi.fn(),
      getGitTools,
    });
    expect(await screen.findByText("meld $LOCAL $REMOTE")).toBeInTheDocument();
    expect(getGitTools).toHaveBeenCalledWith("/r");
    expect(screen.getByText("Diff tool")).toBeInTheDocument();
    expect(screen.getByText("Merge tool")).toBeInTheDocument();
    expect(screen.getByText("meld")).toBeInTheDocument();
    expect(screen.getByText("from merge.tool in global git config")).toBeInTheDocument();
    expect(screen.getByText("evil")).toBeInTheDocument();
    expect(
      screen.getByText("merge.tool is set only in this repository's git config, which Browsitory ignores."),
    ).toBeInTheDocument();
  });

  it("shows Not configured when git has no tools", async () => {
    renderTools({
      getExternalToolSettings: async () => ({ editor: null, terminal: null }),
      setExternalToolSettings: vi.fn(),
      getGitTools: noTools,
    });
    expect(await screen.findAllByText("Not configured")).toHaveLength(2);
  });

  it("skips git tools without an open repository", async () => {
    const getGitTools = vi.fn(noTools);
    renderTools({ getExternalToolSettings: async () => ({ editor: null, terminal: null }), getGitTools }, "desktop", null);
    await screen.findByLabelText("Editor command");
    expect(getGitTools).not.toHaveBeenCalled();
    expect(screen.queryByText("Diff tool")).toBeNull();
  });

  it("only explains VSCode behaviour in VSCode", () => {
    const getExternalToolSettings = vi.fn(async () => ({ editor: null, terminal: null }));
    renderTools({ getExternalToolSettings }, "vscode");
    expect(screen.getByText("In VSCode, files, diffs, merges and terminals open in VSCode itself.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Editor command")).toBeNull();
    expect(screen.queryByText("Diff tool")).toBeNull();
    expect(getExternalToolSettings).not.toHaveBeenCalled();
  });

  it("closes on Close", async () => {
    const onClose = renderTools({
      getExternalToolSettings: async () => ({ editor: null, terminal: null }),
      getGitTools: noTools,
    });
    fireEvent.click(await screen.findByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalled();
  });
});
