import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { COMMIT_MESSAGE_COLUMNS, type RepoClient } from "../ipc/RepoClient";
import { CommitMessageSettingsProvider } from "../state/CommitMessageSettingsProvider";
import { __resetAutoFetchForTests, readAutoFetchOverride } from "../lib/autoFetch";
import { FetchSettingsProvider } from "../state/FetchSettingsProvider";
import {
  CommitMessageSection,
  DiffSection,
  ExternalToolsSection,
  FetchSection,
  PreferencesDialog,
  PreferencesSection,
  UpdatesSection,
  type PreferencesGroup,
  type PreferencesGroupId,
} from "./PreferencesDialog";
import { UpdateSettingsProvider } from "../state/UpdateSettingsProvider";
import type { UpdateCheckOutcome } from "../state/updateCheck";
import { DiffSettingsProvider } from "../state/DiffSettingsProvider";
import { within } from "@testing-library/react";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { AppearanceSettingsProvider } from "../state/AppearanceSettingsProvider";
import { AppearanceSection } from "./PreferencesDialog";

function renderTools(client: Partial<RepoClient>, toolHost: "desktop" | "vscode" = "desktop", repoPath: string | null = "/r") {
  const onClose = vi.fn();
  render(
    <PreferencesDialog
      onClose={onClose}
      groups={[{ id: "external-tools", label: "External tools", content: <ExternalToolsSection client={fakeRepoClient(client)} repoPath={repoPath} toolHost={toolHost} /> }]}
    />,
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

describe("Commit message preferences", () => {
  function renderCommitMessage(client: Partial<RepoClient>) {
    render(
      <CommitMessageSettingsProvider client={fakeRepoClient(client)}>
        <PreferencesDialog
          onClose={vi.fn()}
          groups={[{ id: "commit-message", label: "Commit message", content: <CommitMessageSection /> }]}
        />
      </CommitMessageSettingsProvider>,
    );
  }

  it("edits the commit message settings, clamping numbers on blur", async () => {
    const setCommitMessageSettings = vi.fn(async () => {});
    renderCommitMessage({
      getCommitMessageSettings: async () => ({ summaryLimit: 50, bodyRuler: 72, showRuler: true }),
      setCommitMessageSettings,
    });
    const group = await screen.findByRole("group", { name: "Commit message" });
    const summary = within(group).getByRole("spinbutton", { name: "Summary length limit" });
    await waitFor(() => expect(summary).toHaveValue(50));
    // The backend's clamp range (#497).
    expect(summary).toHaveAttribute("min", String(COMMIT_MESSAGE_COLUMNS.min));
    expect(summary).toHaveAttribute("max", String(COMMIT_MESSAGE_COLUMNS.max));
    fireEvent.change(summary, { target: { value: "60" } });
    expect(setCommitMessageSettings).not.toHaveBeenCalled();
    fireEvent.blur(summary);
    await waitFor(() =>
      expect(setCommitMessageSettings).toHaveBeenLastCalledWith({ summaryLimit: 60, bodyRuler: 72, showRuler: true }),
    );
    fireEvent.click(within(group).getByRole("checkbox", { name: "Show ruler" }));
    await waitFor(() =>
      expect(setCommitMessageSettings).toHaveBeenLastCalledWith({ summaryLimit: 60, bodyRuler: 72, showRuler: false }),
    );
    const ruler = within(group).getByRole("spinbutton", { name: "Ruler column" });
    fireEvent.change(ruler, { target: { value: "500" } });
    fireEvent.blur(ruler);
    await waitFor(() =>
      expect(setCommitMessageSettings).toHaveBeenLastCalledWith({ summaryLimit: 60, bodyRuler: 200, showRuler: false }),
    );
    expect(ruler).toHaveValue(200);
  });

  it("puts back the saved value for an empty or unchanged field", async () => {
    const setCommitMessageSettings = vi.fn(async () => {});
    renderCommitMessage({
      getCommitMessageSettings: async () => ({ summaryLimit: 50, bodyRuler: 72, showRuler: true }),
      setCommitMessageSettings,
    });
    const summary = await screen.findByRole("spinbutton", { name: "Summary length limit" });
    await waitFor(() => expect(summary).toHaveValue(50));
    fireEvent.change(summary, { target: { value: "" } });
    fireEvent.blur(summary);
    expect(summary).toHaveValue(50);
    fireEvent.blur(summary);
    expect(setCommitMessageSettings).not.toHaveBeenCalled();
  });

  it("shows a failed save", async () => {
    renderCommitMessage({
      getCommitMessageSettings: async () => ({ summaryLimit: 50, bodyRuler: 72, showRuler: true }),
      setCommitMessageSettings: async () => {
        throw new Error("read-only config");
      },
    });
    fireEvent.click(await screen.findByRole("checkbox", { name: "Show ruler" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("read-only config");
  });
});

describe("DiffSection", () => {
  it("loads and saves the word wrap setting with the other diff settings", async () => {
    const setDiffSettings = vi.fn(async () => {});
    const client = fakeRepoClient({
      getDiffSettings: async () => ({ ignoreWhitespace: true, contextLines: 5, syntaxHighlight: true, wordWrap: "Off" as const, algorithm: "Default" as const, showWhitespace: false, expandUntracked: true }),
      setDiffSettings,
    });
    render(
      <DiffSettingsProvider client={client}>
        <PreferencesDialog
          onClose={vi.fn()}
          groups={[{ id: "diff", label: "Diff", content: <DiffSection /> }]}
        />
      </DiffSettingsProvider>,
    );
    const group = screen.getByRole("group", { name: "Diff" });
    const wrap = within(group).getByRole("combobox", { name: "Word wrap" });
    await waitFor(() => expect(wrap).toHaveValue("Off"));
    expect(within(wrap).getAllByRole("option").map((o) => o.textContent)).toEqual(["Auto", "On", "Off"]);
    fireEvent.change(wrap, { target: { value: "Auto" } });
    await waitFor(() =>
      expect(setDiffSettings).toHaveBeenCalledWith({
        ignoreWhitespace: true,
        contextLines: 5,
        syntaxHighlight: true,
        wordWrap: "Auto",
        algorithm: "Default",
        showWhitespace: false,
        expandUntracked: true,
      }),
    );
  });

  it("loads and saves the diff algorithm (#469)", async () => {
    const setDiffSettings = vi.fn(async () => {});
    const client = fakeRepoClient({
      getDiffSettings: async () => ({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto" as const, algorithm: "Patience" as const, showWhitespace: false, expandUntracked: true }),
      setDiffSettings,
    });
    render(
      <DiffSettingsProvider client={client}>
        <PreferencesDialog
          onClose={vi.fn()}
          groups={[{ id: "diff", label: "Diff", content: <DiffSection /> }]}
        />
      </DiffSettingsProvider>,
    );
    const algorithm = within(screen.getByRole("group", { name: "Diff" })).getByRole("combobox", { name: "Diff algorithm" });
    await waitFor(() => expect(algorithm).toHaveValue("Patience"));
    expect(within(algorithm).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Default (Myers)",
      "Patience",
      "Minimal",
    ]);
    fireEvent.change(algorithm, { target: { value: "Minimal" } });
    await waitFor(() =>
      expect(setDiffSettings).toHaveBeenCalledWith({
        ignoreWhitespace: false,
        contextLines: 3,
        syntaxHighlight: true,
        wordWrap: "Auto",
        algorithm: "Minimal",
        showWhitespace: false,
        expandUntracked: true,
      }),
    );
  });
  it("loads and saves Expand untracked files (#623)", async () => {
    const setDiffSettings = vi.fn(async () => {});
    const client = fakeRepoClient({
      getDiffSettings: async () => ({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto" as const, algorithm: "Default" as const, showWhitespace: false, expandUntracked: true }),
      setDiffSettings,
    });
    render(
      <DiffSettingsProvider client={client}>
        <PreferencesDialog
          onClose={vi.fn()}
          groups={[{ id: "diff", label: "Diff", content: <DiffSection /> }]}
        />
      </DiffSettingsProvider>,
    );
    const expand = within(screen.getByRole("group", { name: "Diff" })).getByRole("checkbox", { name: "Expand untracked files" });
    await waitFor(() => expect(expand).toBeChecked());
    fireEvent.click(expand);
    await waitFor(() =>
      expect(setDiffSettings).toHaveBeenCalledWith({
        ignoreWhitespace: false,
        contextLines: 3,
        syntaxHighlight: true,
        wordWrap: "Auto",
        algorithm: "Default",
        showWhitespace: false,
        expandUntracked: false,
      }),
    );
    expect(expand).not.toBeChecked();
  });
});

// #729: the working tree's flat | tree choice, also on the file list's own toggle.
describe("DiffSection — changed files as a tree", () => {
  beforeEach(() => localStorage.clear());

  it("shows the per-machine file list layout, flat by default, and saves a change", () => {
    render(
      <PreferencesDialog onClose={vi.fn()} groups={[{ id: "diff", label: "Diff", content: <DiffSection /> }]} />,
    );
    const tree = within(screen.getByRole("group", { name: "Diff" })).getByRole("checkbox", {
      name: "Show changed files as a tree",
    });
    expect(tree).not.toBeChecked();
    fireEvent.click(tree);
    expect(tree).toBeChecked();
    expect(localStorage.getItem("working-tree-file-layout")).toBe("tree");
  });
});

describe("Fetch preferences", () => {
  afterEach(() => {
    localStorage.clear();
    __resetAutoFetchForTests();
  });

  function renderFetch(client: Partial<RepoClient>, repoPath: string | null = "/work/app") {
    render(
      <FetchSettingsProvider client={fakeRepoClient(client)}>
        <PreferencesDialog
          onClose={() => {}}
          groups={[{ id: "fetch", label: "Fetch", content: <FetchSection repoPath={repoPath} /> }]}
        />
      </FetchSettingsProvider>,
    );
  }

  it("is off by default and saves the app-wide interval", async () => {
    const setFetchSettings = vi.fn(async () => {});
    renderFetch({ getFetchSettings: async () => ({ autoFetchMinutes: 0 }), setFetchSettings });
    const group = await screen.findByRole("group", { name: "Fetch" });
    const app = within(group).getByRole("combobox", { name: "Fetch automatically" });
    expect(app).toHaveValue("0");
    expect(within(app).getAllByRole("option").map((option) => option.textContent)).toEqual([
      "Off",
      "Every 5 minutes",
      "Every 10 minutes",
      "Every 30 minutes",
      "Every hour",
    ]);

    fireEvent.change(app, { target: { value: "10" } });

    await waitFor(() => expect(setFetchSettings).toHaveBeenLastCalledWith({ autoFetchMinutes: 10 }));
    expect(within(group).getByRole("combobox", { name: "In app" })).toHaveDisplayValue(
      "Use the app setting (every 10 minutes)",
    );
  });

  it("keeps an interval set in config.toml that the list doesn't offer", async () => {
    renderFetch({ getFetchSettings: async () => ({ autoFetchMinutes: 15 }), setFetchSettings: async () => {} });
    const app = await screen.findByRole("combobox", { name: "Fetch automatically" });
    await waitFor(() => expect(app).toHaveDisplayValue("Every 15 minutes"));
  });

  it("overrides the interval for the active repository only", async () => {
    const setFetchSettings = vi.fn(async () => {});
    renderFetch({ getFetchSettings: async () => ({ autoFetchMinutes: 10 }), setFetchSettings });
    const repo = await screen.findByRole("combobox", { name: "In app" });
    expect(repo).toHaveValue("app");

    fireEvent.change(repo, { target: { value: "0" } });
    expect(repo).toHaveValue("0");
    expect(readAutoFetchOverride("/work/app")).toBe(0);
    expect(readAutoFetchOverride("/work/other")).toBeNull();

    fireEvent.change(repo, { target: { value: "60" } });
    expect(readAutoFetchOverride("/work/app")).toBe(60);
    fireEvent.change(repo, { target: { value: "app" } });
    expect(readAutoFetchOverride("/work/app")).toBeNull();
    expect(setFetchSettings).not.toHaveBeenCalled();
  });

  it("offers no repository override without an open repository", async () => {
    renderFetch({ getFetchSettings: async () => ({ autoFetchMinutes: 0 }), setFetchSettings: async () => {} }, null);
    await screen.findByRole("combobox", { name: "Fetch automatically" });
    expect(screen.getAllByRole("combobox")).toHaveLength(1);
  });

  it("shows a failed save and goes back to the saved interval", async () => {
    renderFetch({
      getFetchSettings: async () => ({ autoFetchMinutes: 0 }),
      setFetchSettings: async () => {
        throw new Error("read-only config");
      },
    });
    const app = await screen.findByRole("combobox", { name: "Fetch automatically" });
    fireEvent.change(app, { target: { value: "5" } });
    expect(await screen.findByRole("alert")).toHaveTextContent("read-only config");
    expect(app).toHaveValue("0");
  });
});

describe("AppearanceSection (#615)", () => {
  const defaults = { codeFont: null, codeFontSize: null, uiSize: "Normal" as const, tabSize: 4, dateFormat: "Iso" as const };
  const root = document.documentElement;

  afterEach(() => {
    root.removeAttribute("style");
    delete root.dataset.uiSize;
  });

  function renderAppearance(client: Partial<RepoClient>) {
    render(
      <AppearanceSettingsProvider client={fakeRepoClient(client)}>
        <PreferencesDialog
          onClose={() => {}}
          groups={[{ id: "appearance", label: "Appearance", content: <AppearanceSection /> }]}
        />
      </AppearanceSettingsProvider>,
    );
    return screen.getByRole("group", { name: "Appearance" });
  }

  it("keeps the built-in look by default", async () => {
    const group = renderAppearance({ getAppearanceSettings: async () => defaults });
    expect(within(group).getByRole("textbox", { name: "Code font" })).toHaveValue("");
    expect(within(group).getByRole("spinbutton", { name: "Code font size (px)" })).toHaveValue(null);
    expect(within(group).getByRole("combobox", { name: "Interface size" })).toHaveValue("Normal");
    expect(within(group).getByRole("spinbutton", { name: "Tab size" })).toHaveValue(4);
    await waitFor(() => expect(root.style.getPropertyValue("--tab-size")).toBe("4"));
    await waitFor(() => expect(root.style.getPropertyValue("--font-mono")).toBe(""));
    await waitFor(() => expect(root.style.getPropertyValue("--code-font-size")).toBe(""));
    await waitFor(() => expect(root.dataset.uiSize).toBeUndefined());
  });

  it("shows and applies the stored settings, clamped", async () => {
    const group = renderAppearance({
      getAppearanceSettings: async () => ({ codeFont: "Example Mono", codeFontSize: 99, uiSize: "Small" as const, tabSize: 2, dateFormat: "Iso" as const }),
    });
    const size = within(group).getByRole("spinbutton", { name: "Code font size (px)" });
    await waitFor(() => expect(size).toHaveValue(24));
    expect(within(group).getByRole("textbox", { name: "Code font" })).toHaveValue("Example Mono");
    await waitFor(() => expect(root.style.getPropertyValue("--font-mono")).toBe('"Example Mono", var(--font-mono-default)'));
    await waitFor(() => expect(root.style.getPropertyValue("--code-font-size")).toBe("24px"));
    await waitFor(() => expect(root.style.getPropertyValue("--tab-size")).toBe("2"));
    await waitFor(() => expect(root.dataset.uiSize).toBe("small"));
  });

  it("saves and applies the code font on blur, and clears it", async () => {
    const setAppearanceSettings = vi.fn(async () => {});
    const group = renderAppearance({ getAppearanceSettings: async () => defaults, setAppearanceSettings });
    const font = within(group).getByRole("textbox", { name: "Code font" });
    fireEvent.change(font, { target: { value: " Example Mono, monospace " } });
    expect(setAppearanceSettings).not.toHaveBeenCalled();
    fireEvent.blur(font);
    await waitFor(() =>
      expect(setAppearanceSettings).toHaveBeenLastCalledWith({ ...defaults, codeFont: "Example Mono, monospace" }),
    );
    await waitFor(() => expect(root.style.getPropertyValue("--font-mono")).toBe('"Example Mono", monospace, var(--font-mono-default)'));

    fireEvent.change(font, { target: { value: "" } });
    fireEvent.keyDown(font, { key: "Enter" });
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith(defaults));
    await waitFor(() => expect(root.style.getPropertyValue("--font-mono")).toBe(""));
  });

  it("saves and applies the code font size, clamped, and an empty one as the default", async () => {
    const setAppearanceSettings = vi.fn(async () => {});
    const group = renderAppearance({ getAppearanceSettings: async () => defaults, setAppearanceSettings });
    const size = within(group).getByRole("spinbutton", { name: "Code font size (px)" });
    fireEvent.change(size, { target: { value: "16" } });
    fireEvent.blur(size);
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith({ ...defaults, codeFontSize: 16 }));
    await waitFor(() => expect(root.style.getPropertyValue("--code-font-size")).toBe("16px"));

    fireEvent.change(size, { target: { value: "3" } });
    fireEvent.blur(size);
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith({ ...defaults, codeFontSize: 10 }));
    expect(size).toHaveValue(10);

    fireEvent.change(size, { target: { value: "" } });
    fireEvent.blur(size);
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith(defaults));
    await waitFor(() => expect(root.style.getPropertyValue("--code-font-size")).toBe(""));
  });

  it("saves and applies the interface size", async () => {
    const setAppearanceSettings = vi.fn(async () => {});
    const group = renderAppearance({ getAppearanceSettings: async () => defaults, setAppearanceSettings });
    const uiSize = within(group).getByRole("combobox", { name: "Interface size" });
    expect(within(uiSize).getAllByRole("option").map((o) => o.textContent)).toEqual(["Small", "Normal", "Large"]);
    fireEvent.change(uiSize, { target: { value: "Large" } });
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith({ ...defaults, uiSize: "Large" }));
    await waitFor(() => expect(root.dataset.uiSize).toBe("large"));
    fireEvent.change(uiSize, { target: { value: "Normal" } });
    await waitFor(() => expect(root.dataset.uiSize).toBeUndefined());
  });

  // #675: ISO by default; System uses the operating system's (or webview's) locale.
  it("saves the date format, with an example of each", async () => {
    const setAppearanceSettings = vi.fn(async () => {});
    const group = renderAppearance({ getAppearanceSettings: async () => defaults, setAppearanceSettings });
    const select = within(group).getByRole("combobox", { name: "Date format" });
    expect(select).toHaveValue("Iso");
    const example = new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" }).format(
      new Date(2026, 10, 11, 14, 5),
    );
    expect(within(select).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "ISO (2026-11-11 14:05)",
      `System (${example})`,
    ]);
    fireEvent.change(select, { target: { value: "System" } });
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith({ ...defaults, dateFormat: "System" }));
  });

  it("saves and applies the tab size, clamped, and ignores an empty one", async () => {
    const setAppearanceSettings = vi.fn(async () => {});
    const group = renderAppearance({ getAppearanceSettings: async () => defaults, setAppearanceSettings });
    const tab = within(group).getByRole("spinbutton", { name: "Tab size" });
    fireEvent.change(tab, { target: { value: "40" } });
    fireEvent.keyDown(tab, { key: "Enter" });
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith({ ...defaults, tabSize: 16 }));
    await waitFor(() => expect(root.style.getPropertyValue("--tab-size")).toBe("16"));

    setAppearanceSettings.mockClear();
    fireEvent.change(tab, { target: { value: "" } });
    fireEvent.blur(tab);
    expect(tab).toHaveValue(16);
    expect(setAppearanceSettings).not.toHaveBeenCalled();
  });

  it("shows a failed save and goes back to the saved look", async () => {
    const group = renderAppearance({
      getAppearanceSettings: async () => defaults,
      setAppearanceSettings: async () => {
        throw new Error("read-only config");
      },
    });
    fireEvent.change(within(group).getByRole("combobox", { name: "Interface size" }), { target: { value: "Small" } });
    expect(await screen.findByRole("alert")).toHaveTextContent("read-only config");
    expect(within(group).getByRole("combobox", { name: "Interface size" })).toHaveValue("Normal");
    await waitFor(() => expect(root.dataset.uiSize).toBeUndefined());
  });

  it("offers no theme when the host has no theme setting (VSCode)", () => {
    const group = renderAppearance({ getAppearanceSettings: async () => defaults });
    expect(within(group).queryByRole("combobox", { name: "Theme" })).toBeNull();
  });

  it("offers System, Light and Dark as the theme and reports a change (#691)", () => {
    const onChange = vi.fn();
    render(
      <AppearanceSettingsProvider client={fakeRepoClient({ getAppearanceSettings: async () => defaults })}>
        <PreferencesDialog
          onClose={() => {}}
          groups={[{ id: "appearance", label: "Appearance", content: <AppearanceSection theme={{ preference: "system", onChange }} /> }]}
        />
      </AppearanceSettingsProvider>,
    );
    const theme = within(screen.getByRole("group", { name: "Appearance" })).getByRole("combobox", { name: "Theme" });
    expect(theme).toHaveValue("system");
    expect(within(theme).getAllByRole("option").map((option) => option.textContent)).toEqual([
      "System",
      "Light",
      "Dark",
    ]);

    fireEvent.change(theme, { target: { value: "dark" } });

    expect(onChange).toHaveBeenCalledWith("dark");
  });
});

describe("Updates preferences (#630)", () => {
  function renderUpdates(
    client: Partial<RepoClient>,
    onCheckNow: () => Promise<UpdateCheckOutcome> = async () => ({ kind: "none" }),
  ) {
    render(
      <UpdateSettingsProvider client={fakeRepoClient(client)}>
        <PreferencesDialog
          onClose={() => {}}
          groups={[{ id: "updates", label: "Updates", content: <UpdatesSection onCheckNow={onCheckNow} /> }]}
        />
      </UpdateSettingsProvider>,
    );
    return screen.getByRole("group", { name: "Updates" });
  }

  it("shows the saved setting and saves a change", async () => {
    const setUpdateSettings = vi.fn(async () => {});
    const group = renderUpdates({ getUpdateSettings: async () => ({ checkAutomatically: false }), setUpdateSettings });
    const box = within(group).getByRole("checkbox", { name: "Check for updates automatically" });
    await waitFor(() => expect(box).not.toBeChecked());

    fireEvent.click(box);
    expect(box).toBeChecked();
    await waitFor(() => expect(setUpdateSettings).toHaveBeenLastCalledWith({ checkAutomatically: true }));

    fireEvent.click(box);
    await waitFor(() => expect(setUpdateSettings).toHaveBeenLastCalledWith({ checkAutomatically: false }));
  });

  it("is on by default", async () => {
    renderUpdates({ getUpdateSettings: async () => ({ checkAutomatically: true }) });
    expect(await screen.findByRole("checkbox", { name: "Check for updates automatically" })).toBeChecked();
  });

  it("shows a failed save and goes back to the saved setting", async () => {
    renderUpdates({
      getUpdateSettings: async () => ({ checkAutomatically: true }),
      setUpdateSettings: async () => {
        throw new Error("read-only config");
      },
    });
    const box = screen.getByRole("checkbox", { name: "Check for updates automatically" });
    fireEvent.click(box);
    expect(await screen.findByRole("alert")).toHaveTextContent("read-only config");
    expect(box).toBeChecked();
  });

  it("checks for updates on demand and shows what it found", async () => {
    let answer: (outcome: UpdateCheckOutcome) => void = () => {};
    const onCheckNow = vi.fn(() => new Promise<UpdateCheckOutcome>((resolve) => (answer = resolve)));
    const group = renderUpdates({ getUpdateSettings: async () => ({ checkAutomatically: false }) }, onCheckNow);
    const button = screen.getByRole("button", { name: "Check for updates" });

    fireEvent.click(button);
    expect(onCheckNow).toHaveBeenCalledTimes(1);
    expect(button).toBeDisabled();
    expect(within(group).getByRole("status")).toHaveTextContent("Checking for updates…");

    answer({ kind: "none" });
    await waitFor(() => expect(within(group).getByRole("status")).toHaveTextContent("Browsitory is up to date."));
    expect(button).toBeEnabled();

    fireEvent.click(button);
    answer({ kind: "ready", version: "1.2.3" });
    await waitFor(() => expect(within(group).getByRole("status")).toHaveTextContent("Update v1.2.3 is ready."));

    fireEvent.click(button);
    answer({ kind: "failed", message: "no endpoint answered" });
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't check for updates: no endpoint answered");
  });
});

describe("Preference groups (#784)", () => {
  afterEach(() => localStorage.clear());

  const field = (label: string) => (
    <label>
      {label}
      <input type="text" />
    </label>
  );
  const groups: PreferencesGroup[] = [
    {
      id: "appearance",
      label: "Appearance",
      content: <PreferencesSection title="Appearance">{[field("Code font"), field("Tab size")]}</PreferencesSection>,
    },
    {
      id: "diff",
      label: "Diff",
      content: <PreferencesSection title="Diff">{[field("Word wrap"), field("Diff algorithm")]}</PreferencesSection>,
    },
    {
      id: "commit-message",
      label: "Commit message",
      content: <PreferencesSection title="Commit message">{field("Summary length limit")}</PreferencesSection>,
    },
    {
      id: "fetch",
      label: "Fetch",
      content: <PreferencesSection title="Fetch">{field("Fetch automatically")}</PreferencesSection>,
    },
  ];

  function renderGroups(props: { initialGroup?: PreferencesGroupId; onClose?: () => void } = {}) {
    return render(<PreferencesDialog onClose={props.onClose ?? (() => {})} groups={groups} initialGroup={props.initialGroup} />);
  }
  const tab = (name: string) => screen.getByRole("tab", { name });

  it("lists the groups as tabs and shows only the first group's settings", () => {
    renderGroups();
    const list = screen.getByRole("tablist", { name: "Preference groups" });
    expect(list).toHaveAttribute("aria-orientation", "vertical");
    expect(within(list).getAllByRole("tab").map((t) => t.textContent)).toEqual([
      "Appearance",
      "Diff",
      "Commit message",
      "Fetch",
    ]);
    expect(tab("Appearance")).toHaveAttribute("aria-selected", "true");
    expect(tab("Diff")).toHaveAttribute("aria-selected", "false");
    const panel = screen.getByRole("tabpanel", { name: "Appearance" });
    expect(tab("Appearance")).toHaveAttribute("aria-controls", panel.id);
    expect(within(panel).getByRole("group", { name: "Appearance" })).toBeInTheDocument();
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
    expect(screen.queryByRole("group", { name: "Diff" })).toBeNull();
  });

  it("shows another group when its tab is clicked", () => {
    renderGroups();
    fireEvent.click(tab("Diff"));
    expect(tab("Diff")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel", { name: "Diff" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Word wrap" })).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Appearance" })).toBeNull();
  });

  it("keeps a group's unsaved edits while another group is shown", () => {
    renderGroups();
    fireEvent.change(screen.getByRole("textbox", { name: "Code font" }), { target: { value: "Fira Code" } });
    fireEvent.click(tab("Diff"));
    fireEvent.click(tab("Appearance"));
    expect(screen.getByRole("textbox", { name: "Code font" })).toHaveValue("Fira Code");
  });

  it("moves between groups with the arrow keys, Home and End, wrapping around", () => {
    renderGroups();
    tab("Appearance").focus();
    fireEvent.keyDown(tab("Appearance"), { key: "ArrowDown" });
    expect(tab("Diff")).toHaveFocus();
    expect(tab("Diff")).toHaveAttribute("aria-selected", "true");
    expect(tab("Diff")).toHaveAttribute("tabindex", "0");
    expect(tab("Appearance")).toHaveAttribute("tabindex", "-1");
    fireEvent.keyDown(tab("Diff"), { key: "ArrowRight" });
    expect(tab("Commit message")).toHaveFocus();
    fireEvent.keyDown(tab("Commit message"), { key: "ArrowUp" });
    expect(tab("Diff")).toHaveFocus();
    fireEvent.keyDown(tab("Diff"), { key: "ArrowLeft" });
    expect(tab("Appearance")).toHaveFocus();
    fireEvent.keyDown(tab("Appearance"), { key: "ArrowUp" });
    expect(tab("Fetch")).toHaveFocus();
    expect(screen.getByRole("tabpanel", { name: "Fetch" })).toBeInTheDocument();
    fireEvent.keyDown(tab("Fetch"), { key: "ArrowDown" });
    expect(tab("Appearance")).toHaveFocus();
    fireEvent.keyDown(tab("Appearance"), { key: "End" });
    expect(tab("Fetch")).toHaveFocus();
    fireEvent.keyDown(tab("Fetch"), { key: "Home" });
    expect(tab("Appearance")).toHaveFocus();
  });

  it("starts with the focus in the filter field", () => {
    renderGroups();
    expect(screen.getByRole("searchbox", { name: "Filter settings" })).toHaveFocus();
  });

  it("reopens on the last group shown on this machine", () => {
    const view = renderGroups();
    fireEvent.click(tab("Commit message"));
    expect(localStorage.getItem("preferences-group")).toBe("commit-message");
    view.unmount();

    renderGroups();
    expect(tab("Commit message")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel", { name: "Commit message" })).toBeInTheDocument();
  });

  it("opens on the first group when the remembered one isn't offered", () => {
    localStorage.setItem("preferences-group", "updates");
    renderGroups();
    expect(tab("Appearance")).toHaveAttribute("aria-selected", "true");
  });

  it("opens on the group it is asked for", () => {
    localStorage.setItem("preferences-group", "fetch");
    renderGroups({ initialGroup: "diff" });
    expect(tab("Diff")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel", { name: "Diff" })).toBeInTheDocument();
  });

  it("filters the groups by setting label and says how many match", () => {
    renderGroups();
    const filter = screen.getByRole("searchbox", { name: "Filter settings" });
    expect(screen.getByRole("status").textContent).toBe("");

    fireEvent.change(filter, { target: { value: "WRAP" } });
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual(["Diff"]);
    expect(tab("Diff")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel", { name: "Diff" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("1 group matches");

    fireEvent.change(filter, { target: { value: "f" } });
    // "Code font" and "Diff algorithm", "Fetch automatically"; the shown group stays shown.
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual(["Appearance", "Diff", "Fetch"]);
    expect(tab("Diff")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("status")).toHaveTextContent("3 groups match");

    fireEvent.change(filter, { target: { value: "commit" } });
    // A group's own name matches too.
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual(["Commit message"]);

    fireEvent.change(filter, { target: { value: "" } });
    expect(screen.getAllByRole("tab")).toHaveLength(4);
    expect(tab("Commit message")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("status").textContent).toBe("");
  });

  it("says so when no setting matches the filter", () => {
    renderGroups();
    fireEvent.change(screen.getByRole("searchbox", { name: "Filter settings" }), { target: { value: "zzz" } });
    expect(screen.queryAllByRole("tab")).toHaveLength(0);
    expect(screen.queryByRole("tabpanel")).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("No settings match “zzz”");
    expect(screen.getByText("No settings match “zzz”")).toBeVisible();
  });

  it("Escape in the filter clears it before it closes the dialog", () => {
    const onClose = vi.fn();
    renderGroups({ onClose });
    const filter = screen.getByRole("searchbox", { name: "Filter settings" });
    fireEvent.change(filter, { target: { value: "wrap" } });
    fireEvent.keyDown(filter, { key: "Escape" });
    expect(filter).toHaveValue("");
    expect(screen.getAllByRole("tab")).toHaveLength(4);
    expect(onClose).not.toHaveBeenCalled();
  });
});
