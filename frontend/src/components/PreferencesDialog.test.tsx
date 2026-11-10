import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { COMMIT_MESSAGE_COLUMNS, type RepoClient } from "../ipc/RepoClient";
import { CommitMessageSettingsProvider } from "../state/CommitMessageSettingsProvider";
import { __resetAutoFetchForTests, readAutoFetchOverride } from "../lib/autoFetch";
import { FetchSettingsProvider } from "../state/FetchSettingsProvider";
import { CommitMessageSection, DiffSection, ExternalToolsSection, FetchSection, PreferencesDialog } from "./PreferencesDialog";
import { DiffSettingsProvider } from "../state/DiffSettingsProvider";
import { within } from "@testing-library/react";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { AppearanceSettingsProvider } from "../state/AppearanceSettingsProvider";
import { AppearanceSection } from "./PreferencesDialog";

function renderTools(client: Partial<RepoClient>, toolHost: "desktop" | "vscode" = "desktop", repoPath: string | null = "/r") {
  const onClose = vi.fn();
  render(
    <PreferencesDialog onClose={onClose}>
      <ExternalToolsSection client={fakeRepoClient(client)} repoPath={repoPath} toolHost={toolHost} />
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

describe("Commit message preferences", () => {
  function renderCommitMessage(client: Partial<RepoClient>) {
    render(
      <CommitMessageSettingsProvider client={fakeRepoClient(client)}>
        <PreferencesDialog onClose={vi.fn()}>
          <CommitMessageSection />
        </PreferencesDialog>
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
      getDiffSettings: async () => ({ ignoreWhitespace: true, contextLines: 5, syntaxHighlight: true, wordWrap: "Off" as const, algorithm: "Default" as const }),
      setDiffSettings,
    });
    render(
      <DiffSettingsProvider client={client}>
        <PreferencesDialog onClose={vi.fn()}>
          <DiffSection />
        </PreferencesDialog>
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
      }),
    );
  });

  it("loads and saves the diff algorithm (#469)", async () => {
    const setDiffSettings = vi.fn(async () => {});
    const client = fakeRepoClient({
      getDiffSettings: async () => ({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto" as const, algorithm: "Patience" as const }),
      setDiffSettings,
    });
    render(
      <DiffSettingsProvider client={client}>
        <PreferencesDialog onClose={vi.fn()}>
          <DiffSection />
        </PreferencesDialog>
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
      }),
    );
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
        <PreferencesDialog onClose={() => {}}>
          <FetchSection repoPath={repoPath} />
        </PreferencesDialog>
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
  const defaults = { codeFont: null, codeFontSize: null, uiSize: "Normal" as const, tabSize: 4 };
  const root = document.documentElement;

  afterEach(() => {
    root.removeAttribute("style");
    delete root.dataset.uiSize;
  });

  function renderAppearance(client: Partial<RepoClient>) {
    render(
      <AppearanceSettingsProvider client={fakeRepoClient(client)}>
        <PreferencesDialog onClose={() => {}}>
          <AppearanceSection />
        </PreferencesDialog>
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
    expect(root.style.getPropertyValue("--font-mono")).toBe("");
    expect(root.style.getPropertyValue("--code-font-size")).toBe("");
    expect(root.dataset.uiSize).toBeUndefined();
  });

  it("shows and applies the stored settings, clamped", async () => {
    const group = renderAppearance({
      getAppearanceSettings: async () => ({ codeFont: "Example Mono", codeFontSize: 99, uiSize: "Small" as const, tabSize: 2 }),
    });
    const size = within(group).getByRole("spinbutton", { name: "Code font size (px)" });
    await waitFor(() => expect(size).toHaveValue(24));
    expect(within(group).getByRole("textbox", { name: "Code font" })).toHaveValue("Example Mono");
    expect(root.style.getPropertyValue("--font-mono")).toBe('"Example Mono", var(--font-mono-default)');
    expect(root.style.getPropertyValue("--code-font-size")).toBe("24px");
    expect(root.style.getPropertyValue("--tab-size")).toBe("2");
    expect(root.dataset.uiSize).toBe("small");
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
    expect(root.style.getPropertyValue("--font-mono")).toBe('"Example Mono", monospace, var(--font-mono-default)');

    fireEvent.change(font, { target: { value: "" } });
    fireEvent.keyDown(font, { key: "Enter" });
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith(defaults));
    expect(root.style.getPropertyValue("--font-mono")).toBe("");
  });

  it("saves and applies the code font size, clamped, and an empty one as the default", async () => {
    const setAppearanceSettings = vi.fn(async () => {});
    const group = renderAppearance({ getAppearanceSettings: async () => defaults, setAppearanceSettings });
    const size = within(group).getByRole("spinbutton", { name: "Code font size (px)" });
    fireEvent.change(size, { target: { value: "16" } });
    fireEvent.blur(size);
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith({ ...defaults, codeFontSize: 16 }));
    expect(root.style.getPropertyValue("--code-font-size")).toBe("16px");

    fireEvent.change(size, { target: { value: "3" } });
    fireEvent.blur(size);
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith({ ...defaults, codeFontSize: 10 }));
    expect(size).toHaveValue(10);

    fireEvent.change(size, { target: { value: "" } });
    fireEvent.blur(size);
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith(defaults));
    expect(root.style.getPropertyValue("--code-font-size")).toBe("");
  });

  it("saves and applies the interface size", async () => {
    const setAppearanceSettings = vi.fn(async () => {});
    const group = renderAppearance({ getAppearanceSettings: async () => defaults, setAppearanceSettings });
    const uiSize = within(group).getByRole("combobox", { name: "Interface size" });
    expect(within(uiSize).getAllByRole("option").map((o) => o.textContent)).toEqual(["Small", "Normal", "Large"]);
    fireEvent.change(uiSize, { target: { value: "Large" } });
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith({ ...defaults, uiSize: "Large" }));
    expect(root.dataset.uiSize).toBe("large");
    fireEvent.change(uiSize, { target: { value: "Normal" } });
    await waitFor(() => expect(root.dataset.uiSize).toBeUndefined());
  });

  it("saves and applies the tab size, clamped, and ignores an empty one", async () => {
    const setAppearanceSettings = vi.fn(async () => {});
    const group = renderAppearance({ getAppearanceSettings: async () => defaults, setAppearanceSettings });
    const tab = within(group).getByRole("spinbutton", { name: "Tab size" });
    fireEvent.change(tab, { target: { value: "40" } });
    fireEvent.keyDown(tab, { key: "Enter" });
    await waitFor(() => expect(setAppearanceSettings).toHaveBeenLastCalledWith({ ...defaults, tabSize: 16 }));
    expect(root.style.getPropertyValue("--tab-size")).toBe("16");

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
    expect(root.dataset.uiSize).toBeUndefined();
  });
});
