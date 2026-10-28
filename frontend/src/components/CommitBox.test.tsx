import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { within } from "@testing-library/react";
import type { SignatureInput } from "../ipc/RepoClient";
import { CommitBox, type CommitDraft } from "./CommitBox";

describe("CommitBox", () => {
  it("Commit button is disabled when disabled=true", () => {
    render(<CommitBox onCommit={vi.fn(async () => true)} disabled={true} onAbortMerge={vi.fn()} />);

    expect(screen.getByText("Commit")).toBeDisabled();
  });

  it("Commit button is disabled with an empty message even when disabled=false", () => {
    render(<CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} />);

    expect(screen.getByText("Commit")).toBeDisabled();
  });

  it("typing a message and clicking Commit calls onCommit and clears the textarea", async () => {
    const onCommit = vi.fn(async () => true);
    render(<CommitBox onCommit={onCommit} disabled={false} onAbortMerge={vi.fn()} />);

    const textarea = screen.getByPlaceholderText("Commit message");
    fireEvent.change(textarea, { target: { value: "my message" } });
    fireEvent.click(screen.getByText("Commit"));

    expect(onCommit).toHaveBeenCalledWith("my message", []);
    await waitFor(() => expect(textarea).toHaveValue(""));
  });

  // Issue #109: the message used to be cleared before the commit's result was known.
  it("keeps the message when the commit fails and clears it when it succeeds", async () => {
    const onCommit = vi.fn<(message: string) => Promise<boolean>>().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    render(<CommitBox onCommit={onCommit} disabled={false} onAbortMerge={() => {}} />);
    const box = screen.getByLabelText("Commit message");
    fireEvent.change(box, { target: { value: "feat: x" } });

    fireEvent.click(screen.getByRole("button", { name: "Commit" }));
    await waitFor(() => expect(onCommit).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByRole("button", { name: "Commit" })).toBeEnabled());
    expect(box).toHaveValue("feat: x");

    fireEvent.click(screen.getByRole("button", { name: "Commit" }));
    await waitFor(() => expect(box).toHaveValue(""));
  });

  it("keeps text typed while the commit was running", async () => {
    let finish: (ok: boolean) => void = () => {};
    const onCommit = vi.fn(() => new Promise<boolean>((resolve) => (finish = resolve)));
    render(<CommitBox onCommit={onCommit} disabled={false} onAbortMerge={() => {}} />);
    const box = screen.getByLabelText("Commit message");
    fireEvent.change(box, { target: { value: "first" } });
    fireEvent.click(screen.getByRole("button", { name: "Commit" }));
    expect(screen.getByRole("button", { name: "Commit" })).toBeDisabled();

    fireEvent.change(box, { target: { value: "first, edited" } });
    finish(true);

    await waitFor(() => expect(screen.getByRole("button", { name: "Commit" })).toBeEnabled());
    expect(box).toHaveValue("first, edited");
    expect(onCommit).toHaveBeenCalledTimes(1);
  });

  it("shows Skip hooks only when hooks are installed", () => {
    const onToggleSkip = vi.fn();
    const hooks = { installed: true, skip: false, onToggleSkip, running: null, onCancel: () => {} };
    const { rerender } = render(
      <CommitBox onCommit={async () => true} disabled={false} onAbortMerge={() => {}} hooks={hooks} />,
    );
    fireEvent.click(screen.getByLabelText("Skip hooks"));
    expect(onToggleSkip).toHaveBeenCalledWith(true);

    rerender(
      <CommitBox onCommit={async () => true} disabled={false} onAbortMerge={() => {}} hooks={{ ...hooks, installed: false }} />,
    );
    expect(screen.queryByLabelText("Skip hooks")).toBeNull();
  });

  it("shows the running hook with a cancel button that latches", () => {
    const onCancel = vi.fn();
    render(
      <CommitBox
        onCommit={async () => true}
        disabled={false}
        onAbortMerge={() => {}}
        hooks={{
          installed: true,
          skip: false,
          onToggleSkip: () => {},
          running: { operationId: "commit-1", hook: "pre-commit" },
          onCancel,
        }}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Running pre-commit hook…");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Cancelling…" })).toBeDisabled();
  });

  it("shows a waiting signing program with the same cancel button", () => {
    const onCancel = vi.fn();
    render(
      <CommitBox
        onCommit={async () => true}
        disabled={false}
        onAbortMerge={() => {}}
        hooks={{
          installed: false,
          skip: false,
          onToggleSkip: () => {},
          running: { operationId: "commit-2", hook: "signing" },
          onCancel,
        }}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Waiting for the signing program…");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("clears the message on a one-shot request only if it still holds the committed text", () => {
    const onClearMessageHandled = vi.fn();
    const { rerender } = render(<CommitBox onCommit={async () => true} disabled={false} onAbortMerge={() => {}} />);
    const box = screen.getByLabelText("Commit message");
    fireEvent.change(box, { target: { value: "committed, then edited" } });
    rerender(
      <CommitBox
        onCommit={async () => true}
        disabled={false}
        onAbortMerge={() => {}}
        clearMessage="committed"
        onClearMessageHandled={onClearMessageHandled}
      />,
    );
    expect(box).toHaveValue("committed, then edited");
    expect(onClearMessageHandled).toHaveBeenCalledTimes(1);

    fireEvent.change(box, { target: { value: "committed" } });
    rerender(<CommitBox onCommit={async () => true} disabled={false} onAbortMerge={() => {}} clearMessage={null} />);
    rerender(
      <CommitBox
        onCommit={async () => true}
        disabled={false}
        onAbortMerge={() => {}}
        clearMessage="committed"
        onClearMessageHandled={onClearMessageHandled}
      />,
    );
    expect(box).toHaveValue("");
    expect(onClearMessageHandled).toHaveBeenCalledTimes(2);
  });

  // AUD-2026-09-05-FE-002: the textarea previously relied on `placeholder` alone for its
  // accessible name (jsdom's role-name computation happens to fall back to placeholder, but
  // that's not a reliable substitute for a real label per the ARIA accessible-name spec) — an
  // explicit `aria-label` makes it robust regardless of how a given assistive-tech implementation
  // computes fallback names.
  it("exposes the textarea as a labeled 'Commit message' textbox", () => {
    render(<CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} />);

    expect(screen.getByRole("textbox", { name: "Commit message" })).toBeInTheDocument();
  });

  it("Cmd/Ctrl+Enter in the textarea commits", () => {
    const onCommit = vi.fn(async () => true);
    render(<CommitBox onCommit={onCommit} disabled={false} onAbortMerge={vi.fn()} />);

    const textarea = screen.getByPlaceholderText("Commit message");
    fireEvent.change(textarea, { target: { value: "my message" } });
    fireEvent.keyDown(textarea, { key: "Enter", metaKey: true });

    expect(onCommit).toHaveBeenCalledWith("my message", []);
  });

  it("Cmd/Ctrl+Enter does nothing when disabled", () => {
    const onCommit = vi.fn(async () => true);
    render(<CommitBox onCommit={onCommit} disabled={true} onAbortMerge={vi.fn()} />);

    const textarea = screen.getByPlaceholderText("Commit message");
    fireEvent.change(textarea, { target: { value: "my message" } });
    fireEvent.keyDown(textarea, { key: "Enter", metaKey: true });

    expect(onCommit).not.toHaveBeenCalled();
  });

  it("pre-fills the message from initialMessage", () => {
    render(<CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} initialMessage="Merge branch 'feature'" />);

    expect(screen.getByDisplayValue("Merge branch 'feature'")).toBeInTheDocument();
  });

  it("does not clobber a user edit if initialMessage is passed again unchanged", () => {
    const { rerender } = render(
      <CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} initialMessage="Merge branch 'feature'" />,
    );
    fireEvent.change(screen.getByPlaceholderText("Commit message"), {
      target: { value: "edited by user" },
    });

    rerender(
      <CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} initialMessage="Merge branch 'feature'" />,
    );

    expect(screen.getByDisplayValue("edited by user")).toBeInTheDocument();
  });

  it("clears the message when initialMessage goes from set to unset (merge ends, e.g. abort)", () => {
    const { rerender } = render(
      <CommitBox
        onCommit={vi.fn(async () => true)}
        disabled={false}
        onAbortMerge={vi.fn()}
        initialMessage="Merge branch 'feature'"
      />,
    );
    expect(screen.getByDisplayValue("Merge branch 'feature'")).toBeInTheDocument();

    rerender(<CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} />);

    expect(screen.getByPlaceholderText("Commit message")).toHaveValue("");
  });

  it("reseeds with the new merge's message (not the leftover from a previous, aborted merge)", () => {
    const { rerender } = render(
      <CommitBox
        onCommit={vi.fn(async () => true)}
        disabled={false}
        onAbortMerge={vi.fn()}
        initialMessage="Merge branch 'A'"
      />,
    );
    expect(screen.getByDisplayValue("Merge branch 'A'")).toBeInTheDocument();

    // Merge A aborts.
    rerender(<CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} />);
    expect(screen.getByPlaceholderText("Commit message")).toHaveValue("");

    // Merge B starts.
    rerender(
      <CommitBox
        onCommit={vi.fn(async () => true)}
        disabled={false}
        onAbortMerge={vi.fn()}
        initialMessage="Merge branch 'B'"
      />,
    );

    expect(screen.getByDisplayValue("Merge branch 'B'")).toBeInTheDocument();
    expect(screen.queryByDisplayValue("Merge branch 'A'")).not.toBeInTheDocument();
  });

  it("does not clobber a user's edit made between an abort and the next merge's auto-seed", () => {
    const { rerender } = render(
      <CommitBox
        onCommit={vi.fn(async () => true)}
        disabled={false}
        onAbortMerge={vi.fn()}
        initialMessage="Merge branch 'A'"
      />,
    );

    // Merge A aborts.
    rerender(<CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} />);
    // User types something of their own before merge B starts.
    fireEvent.change(screen.getByPlaceholderText("Commit message"), {
      target: { value: "my own draft message" },
    });

    // Merge B starts.
    rerender(
      <CommitBox
        onCommit={vi.fn(async () => true)}
        disabled={false}
        onAbortMerge={vi.fn()}
        initialMessage="Merge branch 'B'"
      />,
    );

    expect(screen.getByDisplayValue("my own draft message")).toBeInTheDocument();
  });

  it("re-seeds the message when merging the same branch twice in a row (commit clears lastSeeded too)", () => {
    const { rerender } = render(
      <CommitBox
        onCommit={vi.fn(async () => true)}
        disabled={false}
        onAbortMerge={vi.fn()}
        initialMessage="Merge branch 'feature'"
      />,
    );
    expect(screen.getByDisplayValue("Merge branch 'feature'")).toBeInTheDocument();

    // Commit the first merge. This clears both `message` and `lastSeeded` internally.
    fireEvent.click(screen.getByText("Commit"));

    // Merge ends (mergeMessage goes back to null after the commit).
    rerender(<CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} />);

    // The SAME branch is merged again, producing the identical merge message. Without also
    // clearing `lastSeeded` on commit, `message` ("") would no longer equal the stale
    // `lastSeeded` ("Merge branch 'feature'"), so the seed effect would refuse to re-seed and
    // the box would stay empty.
    rerender(
      <CommitBox
        onCommit={vi.fn(async () => true)}
        disabled={false}
        onAbortMerge={vi.fn()}
        initialMessage="Merge branch 'feature'"
      />,
    );

    expect(screen.getByDisplayValue("Merge branch 'feature'")).toBeInTheDocument();
  });

  it("renders an Abort merge button only when initialMessage is set, and calls onAbortMerge", () => {
    const onAbortMerge = vi.fn();
    const { rerender } = render(
      <CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={onAbortMerge} />,
    );
    expect(screen.queryByText("Abort merge")).not.toBeInTheDocument();

    rerender(
      <CommitBox
        onCommit={vi.fn(async () => true)}
        disabled={false}
        onAbortMerge={onAbortMerge}
        initialMessage="Merge branch 'feature'"
      />,
    );
    fireEvent.click(screen.getByText("Abort merge"));

    expect(onAbortMerge).toHaveBeenCalled();
  });

  it("starts from initialDraft and reports every change through onDraftChange", async () => {
    const onDraftChange = vi.fn();
    render(
      <CommitBox
        onCommit={vi.fn(async () => true)}
        disabled={false}
        onAbortMerge={vi.fn()}
        initialDraft={{ message: "restored", lastSeeded: "" }}
        onDraftChange={onDraftChange}
      />,
    );

    const textarea = screen.getByRole("textbox", { name: "Commit message" });
    expect(textarea).toHaveValue("restored");

    fireEvent.change(textarea, { target: { value: "restored, then edited" } });
    expect(onDraftChange).toHaveBeenLastCalledWith({
      message: "restored, then edited",
      lastSeeded: "",
      stashed: null,
      coAuthors: [],
      stashedCoAuthors: null,
      amendAuthor: null,
    });

    fireEvent.click(screen.getByText("Commit"));
    await waitFor(() =>
      expect(onDraftChange).toHaveBeenLastCalledWith({
        message: "",
        lastSeeded: "",
        stashed: null,
        coAuthors: [],
        stashedCoAuthors: null,
        amendAuthor: null,
      }),
    );
  });

  it("still clears a restored merge pre-fill when the merge ends", () => {
    const seeded = "Merge branch 'feature'";
    const { rerender } = render(
      <CommitBox
        onCommit={vi.fn(async () => true)}
        disabled={false}
        onAbortMerge={vi.fn()}
        initialMessage={seeded}
        initialDraft={{ message: seeded, lastSeeded: seeded }}
      />,
    );
    expect(screen.getByRole("textbox", { name: "Commit message" })).toHaveValue(seeded);

    rerender(<CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} initialDraft={{ message: seeded, lastSeeded: seeded }} />);

    expect(screen.getByRole("textbox", { name: "Commit message" })).toHaveValue("");
  });

  it("shows why Commit is disabled and ties it to the button", () => {
    render(<CommitBox onCommit={vi.fn(async () => true)} disabled disabledReason="Stage changes to commit" onAbortMerge={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Commit" })).toHaveAccessibleDescription("Stage changes to commit");
  });
});

describe("amend", () => {
  function renderBox(
    overrides: Partial<NonNullable<Parameters<typeof CommitBox>[0]["amend"]>> = {},
    draft?: string,
  ) {
    const amend = {
      available: true,
      on: false,
      headMessage: "previous message",
      pushed: false,
      onToggle: vi.fn(),
      onAmend: vi.fn(async () => true),
      ...overrides,
    };
    const onCommit = vi.fn(async () => true);
    const utils = render(
      <CommitBox
        onCommit={onCommit}
        disabled={true}
        disabledReason="Stage changes to commit"
        onAbortMerge={() => {}}
        initialDraft={draft === undefined ? undefined : { message: draft, lastSeeded: "" }}
        amend={amend}
      />,
    );
    return { ...utils, amend, onCommit };
  }

  it("shows the toggle and calls onToggle", () => {
    const { amend } = renderBox();
    fireEvent.click(screen.getByRole("checkbox", { name: "Amend last commit" }));
    expect(amend.onToggle).toHaveBeenCalledWith(true);
  });

  it("prefills HEAD's message, relabels the button and allows an empty index", () => {
    const { amend } = renderBox({ on: true });
    expect(screen.getByLabelText("Commit message")).toHaveValue("previous message");
    const button = screen.getByRole("button", { name: "Amend" });
    expect(button).toBeEnabled();
    fireEvent.click(button);
    expect(amend.onAmend).toHaveBeenCalledWith("previous message", { author: null, trailers: [] });
  });

  it("restores the typed draft exactly when toggled off", () => {
    const props = { onCommit: vi.fn(), disabled: true, onAbortMerge: () => {} };
    const amend = {
      available: true,
      headMessage: "previous message",
      pushed: false,
      onToggle: vi.fn(),
      onAmend: vi.fn(async () => true),
    };
    const { rerender } = render(<CommitBox {...props} amend={{ ...amend, on: false }} />);
    fireEvent.change(screen.getByLabelText("Commit message"), {
      target: { value: "half-typed\nwork" },
    });
    rerender(<CommitBox {...props} amend={{ ...amend, on: true }} />);
    expect(screen.getByLabelText("Commit message")).toHaveValue("previous message");
    rerender(<CommitBox {...props} amend={{ ...amend, on: false }} />);
    expect(screen.getByLabelText("Commit message")).toHaveValue("half-typed\nwork");
  });

  it("warns when HEAD is already pushed", () => {
    renderBox({ on: true, pushed: true });
    expect(screen.getByText("Already pushed — you'll need to force push.")).toBeInTheDocument();
  });

  it("focuses the message field once the amend prefill is in, then reports it handled", () => {
    const onFocusMessageHandled = vi.fn();
    const props = { onCommit: vi.fn(), disabled: true, onAbortMerge: () => {}, focusMessage: true, onFocusMessageHandled };
    const amend = { available: true, on: true, pushed: false, onToggle: vi.fn(), onAmend: vi.fn() };
    const { rerender } = render(<CommitBox {...props} amend={{ ...amend, headMessage: null }} />);
    expect(screen.getByLabelText("Commit message")).not.toHaveFocus();
    expect(onFocusMessageHandled).not.toHaveBeenCalled();
    rerender(<CommitBox {...props} amend={{ ...amend, headMessage: "previous message" }} />);
    expect(screen.getByLabelText("Commit message")).toHaveValue("previous message");
    expect(screen.getByLabelText("Commit message")).toHaveFocus();
    expect(onFocusMessageHandled).toHaveBeenCalledTimes(1);
  });

  it("hides the toggle when amend is unavailable", () => {
    renderBox({ available: false });
    expect(screen.queryByRole("checkbox", { name: "Amend last commit" })).not.toBeInTheDocument();
  });
});

describe("amend draft safety", () => {
  const base = { onCommit: vi.fn(async () => true), disabled: true, onAbortMerge: () => {} };
  const amend = {
    available: true,
    headMessage: "previous message",
    pushed: false,
    onToggle: vi.fn(),
    onAmend: vi.fn(async () => true),
  };

  it("amends once per submit and disables Amend while it runs", async () => {
    let finish: (ok: boolean) => void = () => {};
    const onAmend = vi.fn(() => new Promise<boolean>((resolve) => (finish = resolve)));
    render(<CommitBox {...base} amend={{ ...amend, on: true, onAmend }} />);
    const box = await screen.findByDisplayValue("previous message");

    fireEvent.keyDown(box, { key: "Enter", ctrlKey: true });
    fireEvent.keyDown(box, { key: "Enter", ctrlKey: true });
    expect(onAmend).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Amend" })).toBeDisabled();

    finish(false);
    await waitFor(() => expect(screen.getByRole("button", { name: "Amend" })).toBeEnabled());
  });

  it("survives a remount while amending and still restores the original draft", () => {
    let saved: CommitDraft | undefined;
    const onDraftChange = (draft: CommitDraft) => {
      saved = draft;
    };
    const first = render(
      <CommitBox {...base} amend={{ ...amend, on: false }} onDraftChange={onDraftChange} />,
    );
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "my draft" } });
    first.rerender(<CommitBox {...base} amend={{ ...amend, on: true }} onDraftChange={onDraftChange} />);
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "edited amend" } });
    first.unmount();

    const second = render(
      <CommitBox
        {...base}
        amend={{ ...amend, on: true }}
        initialDraft={saved}
        onDraftChange={onDraftChange}
      />,
    );
    expect(screen.getByLabelText("Commit message")).toHaveValue("edited amend");
    second.rerender(<CommitBox {...base} amend={{ ...amend, on: false }} initialDraft={saved} />);
    expect(screen.getByLabelText("Commit message")).toHaveValue("my draft");
  });

  it("keeps the typed amend message on submit and restores the draft once amend ends", () => {
    const onAmend = vi.fn(async () => true);
    const { rerender } = render(<CommitBox {...base} amend={{ ...amend, on: false, onAmend }} />);
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "my draft" } });
    rerender(<CommitBox {...base} amend={{ ...amend, on: true, onAmend }} />);
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "new text" } });
    fireEvent.click(screen.getByRole("button", { name: "Amend" }));
    expect(onAmend).toHaveBeenCalledWith("new text", { author: null, trailers: [] });
    // Failure: amend still on, typed text kept.
    expect(screen.getByLabelText("Commit message")).toHaveValue("new text");
    // Success: amend mode turns off, original draft returns.
    rerender(<CommitBox {...base} amend={{ ...amend, on: false, onAmend }} />);
    expect(screen.getByLabelText("Commit message")).toHaveValue("my draft");
  });

  it("ignores amend mode while unavailable", () => {
    render(<CommitBox {...base} amend={{ ...amend, available: false, on: true }} />);
    expect(screen.getByRole("button", { name: "Commit" })).toBeInTheDocument();
    expect(screen.getByLabelText("Commit message")).toHaveValue("");
  });
});

describe("CommitBox showAbort", () => {
  it("hides the abort button for a seeded message when showAbort is false", () => {
    render(
      <CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} initialMessage="Squashed commit" showAbort={false} />,
    );

    expect(screen.getByPlaceholderText("Commit message")).toHaveValue("Squashed commit");
    expect(screen.queryByRole("button", { name: "Abort merge" })).toBeNull();
  });

  it("shows the abort button for a seeded message by default", () => {
    render(<CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} initialMessage="Merge branch" />);

    expect(screen.getByRole("button", { name: "Abort merge" })).toBeInTheDocument();
  });

  describe("summary counter and ruler", () => {
    it("counts summary characters and warns past the soft limit without blocking", async () => {
      const onCommit = vi.fn(async () => true);
      render(
        <CommitBox
          onCommit={onCommit}
          disabled={false}
          onAbortMerge={vi.fn()}
          messageSettings={{ summaryLimit: 10, bodyRuler: 72, showRuler: true }}
        />,
      );
      const box = screen.getByRole("textbox", { name: "Commit message" });
      const counter = screen.getByLabelText("Summary length");
      expect(counter).toHaveTextContent("Summary 0/10");
      fireEvent.change(box, { target: { value: "0123456789\nbody that is long enough to pass any limit" } });
      expect(counter).toHaveTextContent("Summary 10/10");
      expect(counter).not.toHaveAttribute("data-over");
      expect(counter).not.toHaveAttribute("title");
      fireEvent.change(box, { target: { value: "0123456789é\n" } });
      expect(counter).toHaveTextContent("Summary 11/10");
      expect(counter).toHaveAttribute("data-over", "true");
      expect(counter).toHaveAttribute("title", "Many tools cut summaries longer than 10 characters.");
      fireEvent.click(screen.getByRole("button", { name: "Commit" }));
      expect(onCommit).toHaveBeenCalled();
      await waitFor(() => expect(box).toHaveValue(""));
    });

    it("counts code points, not UTF-16 units, of the first line only", () => {
      render(<CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} />);
      const box = screen.getByRole("textbox", { name: "Commit message" });
      fireEvent.change(box, { target: { value: "🎉 ok" } });
      expect(screen.getByLabelText("Summary length")).toHaveTextContent("Summary 4/50");
      fireEvent.change(box, { target: { value: "ab\r\nsecond line" } });
      expect(screen.getByLabelText("Summary length")).toHaveTextContent("Summary 2/50");
    });

    it("draws the ruler at the configured column in a monospace field, or not at all", () => {
      const { rerender } = render(
        <CommitBox
          onCommit={vi.fn(async () => true)}
          disabled={false}
          onAbortMerge={vi.fn()}
          messageSettings={{ summaryLimit: 50, bodyRuler: 80, showRuler: true }}
        />,
      );
      const box = screen.getByRole("textbox", { name: "Commit message" });
      expect(box).toHaveAttribute("data-ruler", "true");
      expect(box.style.getPropertyValue("--ruler-column")).toBe("80");
      rerender(
        <CommitBox
          onCommit={vi.fn(async () => true)}
          disabled={false}
          onAbortMerge={vi.fn()}
          messageSettings={{ summaryLimit: 50, bodyRuler: 80, showRuler: false }}
        />,
      );
      expect(box).not.toHaveAttribute("data-ruler");
      expect(box.style.getPropertyValue("--ruler-column")).toBe("");
    });
  });

  describe("co-authors", () => {
    const recent = async (): Promise<SignatureInput[]> => [
      { name: "Ada", email: "ada@x.org" },
      { name: "Bob", email: "bob@x.org" },
    ];

    it("adds co-authors from recent authors and commits them as trailers", async () => {
      const onCommit = vi.fn(async () => true);
      render(<CommitBox onCommit={onCommit} disabled={false} onAbortMerge={vi.fn()} loadRecentAuthors={recent} />);
      fireEvent.change(screen.getByRole("textbox", { name: "Commit message" }), { target: { value: "Fix" } });
      fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
      const list = await screen.findByRole("listbox", { name: "Recent authors" });
      await within(list).findByRole("option", { name: "Ada <ada@x.org>" });
      fireEvent.change(screen.getByRole("combobox", { name: "Filter authors" }), { target: { value: "bo" } });
      expect(within(list).queryByRole("option", { name: /Ada/ })).toBeNull();
      fireEvent.click(within(list).getByRole("option", { name: "Bob <bob@x.org>" }));
      expect(screen.queryByRole("listbox", { name: "Recent authors" })).toBeNull();
      const chips = screen.getByRole("list", { name: "Co-authors" });
      expect(within(chips).getByText("Bob <bob@x.org>")).toBeInTheDocument();

      fireEvent.click(screen.getByRole("button", { name: "Commit" }));
      expect(onCommit).toHaveBeenCalledWith("Fix", [{ key: "Co-authored-by", value: "Bob <bob@x.org>" }]);
      await waitFor(() => expect(screen.queryByRole("list", { name: "Co-authors" })).toBeNull());
    });

    it("keeps the chips when the commit fails", async () => {
      const onCommit = vi.fn(async () => false);
      render(
        <CommitBox
          onCommit={onCommit}
          disabled={false}
          onAbortMerge={vi.fn()}
          loadRecentAuthors={recent}
          initialDraft={{ message: "x", lastSeeded: "", coAuthors: [{ name: "Ada", email: "ada@x.org" }] }}
        />,
      );
      fireEvent.click(screen.getByRole("button", { name: "Commit" }));
      await waitFor(() => expect(screen.getByRole("button", { name: "Commit" })).toBeEnabled());
      expect(screen.getByText("Ada <ada@x.org>")).toBeInTheDocument();
    });

    it("picks with the keyboard, skips chosen authors and adds a typed Name <email>", async () => {
      const onDraftChange = vi.fn();
      render(
        <CommitBox
          onCommit={vi.fn(async () => true)}
          disabled={false}
          onAbortMerge={vi.fn()}
          loadRecentAuthors={recent}
          onDraftChange={onDraftChange}
          initialDraft={{ message: "", lastSeeded: "", coAuthors: [{ name: "Ada", email: "ADA@x.org" }] }}
        />,
      );
      fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
      const list = await screen.findByRole("listbox", { name: "Recent authors" });
      await within(list).findByRole("option", { name: "Bob <bob@x.org>" });
      expect(within(list).queryByRole("option", { name: /Ada/ })).toBeNull();
      const filter = screen.getByRole("combobox", { name: "Filter authors" });
      fireEvent.keyDown(filter, { key: "Enter" });
      expect(screen.getByText("Bob <bob@x.org>")).toBeInTheDocument();

      fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
      const filter2 = await screen.findByRole("combobox", { name: "Filter authors" });
      fireEvent.change(filter2, { target: { value: "Cy Young <cy@x.org>" } });
      fireEvent.keyDown(filter2, { key: "Enter" });
      expect(screen.getByText("Cy Young <cy@x.org>")).toBeInTheDocument();
      expect(onDraftChange).toHaveBeenLastCalledWith(
        expect.objectContaining({
          coAuthors: [
            { name: "Ada", email: "ADA@x.org" },
            { name: "Bob", email: "bob@x.org" },
            { name: "Cy Young", email: "cy@x.org" },
          ],
        }),
      );

      fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
      fireEvent.keyDown(await screen.findByRole("combobox", { name: "Filter authors" }), { key: "Escape" });
      expect(screen.queryByRole("listbox")).toBeNull();
      expect(screen.getByRole("button", { name: "Add co-author" })).toHaveFocus();
    });

    it("says when recent authors can't be loaded", async () => {
      render(
        <CommitBox
          onCommit={vi.fn(async () => true)}
          disabled={false}
          onAbortMerge={vi.fn()}
          loadRecentAuthors={async () => {
            throw new Error("boom");
          }}
        />,
      );
      fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
      expect(await screen.findByText("Couldn't load recent authors.")).toBeInTheDocument();
    });

    it("removes a co-author chip", () => {
      render(
        <CommitBox
          onCommit={vi.fn(async () => true)}
          disabled={false}
          onAbortMerge={vi.fn()}
          initialDraft={{ message: "x", lastSeeded: "", coAuthors: [{ name: "Ada", email: "ada@x.org" }] }}
          loadRecentAuthors={async () => []}
        />,
      );
      fireEvent.click(screen.getByRole("button", { name: "Remove co-author Ada" }));
      expect(screen.queryByText("Ada <ada@x.org>")).toBeNull();
    });

    it("hides co-authors and sends no trailers while a merge message is seeded", async () => {
      const onCommit = vi.fn(async () => true);
      render(
        <CommitBox
          onCommit={onCommit}
          disabled={false}
          onAbortMerge={vi.fn()}
          initialMessage="Merge branch 'x'"
          initialDraft={{ message: "", lastSeeded: "", coAuthors: [{ name: "Ada", email: "ada@x.org" }] }}
          loadRecentAuthors={async () => []}
        />,
      );
      expect(screen.queryByRole("button", { name: "Add co-author" })).toBeNull();
      expect(screen.queryByText("Ada <ada@x.org>")).toBeNull();
      await waitFor(() => expect(screen.getByLabelText("Commit message")).toHaveValue("Merge branch 'x'"));
      fireEvent.click(screen.getByRole("button", { name: "Commit" }));
      expect(onCommit).toHaveBeenCalledWith("Merge branch 'x'", []);
    });

    it("clears the chips with the message on a clear request after a retried commit", () => {
      const props = {
        onCommit: async () => true,
        disabled: false,
        onAbortMerge: () => {},
        loadRecentAuthors: async () => [],
        initialDraft: { message: "Fix", lastSeeded: "", coAuthors: [{ name: "Ada", email: "ada@x.org" }] },
      };
      const { rerender } = render(<CommitBox {...props} />);
      rerender(<CommitBox {...props} clearMessage="Fix" onClearMessageHandled={() => {}} />);
      expect(screen.getByLabelText("Commit message")).toHaveValue("");
      expect(screen.queryByText("Ada <ada@x.org>")).toBeNull();
    });
  });

  describe("amend author and co-authors", () => {
    function amendBox(
      onAmend = vi.fn(async () => true),
      headAuthor: SignatureInput | null = { name: "Me", email: "me@x.org" },
      extra: Partial<React.ComponentProps<typeof CommitBox>> = {},
    ) {
      const amend = { available: true, on: true, headMessage: "Old", pushed: false, onToggle: vi.fn(), onAmend, headAuthor };
      const utils = render(<CommitBox onCommit={vi.fn(async () => true)} disabled={false} onAbortMerge={vi.fn()} amend={amend} {...extra} />);
      return { onAmend, amend, ...utils };
    }

    it("amends with the author unchanged as null, and with an edited author parsed", async () => {
      const { onAmend } = amendBox();
      const author = await screen.findByRole("textbox", { name: "Author" });
      expect(author).toHaveValue("Me <me@x.org>");
      await waitFor(() => expect(screen.getByLabelText("Commit message")).toHaveValue("Old"));
      fireEvent.click(screen.getByRole("button", { name: "Amend" }));
      expect(onAmend).toHaveBeenLastCalledWith("Old", { author: null, trailers: [] });
      await waitFor(() => expect(screen.getByRole("button", { name: "Amend" })).toBeEnabled());
      fireEvent.change(author, { target: { value: "Ada Lovelace <ada@x.org>" } });
      fireEvent.click(screen.getByRole("button", { name: "Amend" }));
      expect(onAmend).toHaveBeenLastCalledWith("Old", { author: { name: "Ada Lovelace", email: "ada@x.org" }, trailers: [] });
    });

    it("blocks amend on an unparseable author", async () => {
      const { onAmend } = amendBox();
      fireEvent.change(await screen.findByRole("textbox", { name: "Author" }), { target: { value: "Ada" } });
      await waitFor(() => expect(screen.getByLabelText("Commit message")).toHaveValue("Old"));
      fireEvent.click(screen.getByRole("button", { name: "Amend" }));
      expect(screen.getByText("Use the form Name <email>.")).toBeInTheDocument();
      expect(screen.getByRole("textbox", { name: "Author" })).toHaveAttribute("aria-invalid", "true");
      expect(onAmend).not.toHaveBeenCalled();
    });

    it("leaves the author empty with a placeholder when HEAD's author is unknown", async () => {
      const { onAmend } = amendBox(vi.fn(async () => true), null);
      const author = await screen.findByRole("textbox", { name: "Author" });
      expect(author).toHaveValue("");
      expect(author).toHaveAttribute("placeholder", "Keep original author");
      await waitFor(() => expect(screen.getByLabelText("Commit message")).toHaveValue("Old"));
      fireEvent.click(screen.getByRole("button", { name: "Amend" }));
      expect(onAmend).toHaveBeenLastCalledWith("Old", { author: null, trailers: [] });
    });

    it("starts amend without the commit's chips, sends its own, and gives the chips back after", async () => {
      const onAmend = vi.fn(async () => true);
      const base = {
        onCommit: vi.fn(async () => true),
        disabled: false,
        onAbortMerge: vi.fn(),
        loadRecentAuthors: async () => [{ name: "Bob", email: "bob@x.org" }],
        initialDraft: { message: "draft", lastSeeded: "", coAuthors: [{ name: "Ada", email: "ada@x.org" }] },
      };
      const amend = { available: true, on: false, headMessage: "Old", pushed: false, onToggle: vi.fn(), onAmend, headAuthor: null };
      const { rerender } = render(<CommitBox {...base} amend={amend} />);
      expect(screen.getByText("Ada <ada@x.org>")).toBeInTheDocument();
      rerender(<CommitBox {...base} amend={{ ...amend, on: true }} />);
      await waitFor(() => expect(screen.getByLabelText("Commit message")).toHaveValue("Old"));
      expect(screen.queryByText("Ada <ada@x.org>")).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
      fireEvent.click(await screen.findByRole("option", { name: "Bob <bob@x.org>" }));
      fireEvent.click(screen.getByRole("button", { name: "Amend" }));
      expect(onAmend).toHaveBeenLastCalledWith("Old", {
        author: null,
        trailers: [{ key: "Co-authored-by", value: "Bob <bob@x.org>" }],
      });
      rerender(<CommitBox {...base} amend={{ ...amend, on: false }} />);
      await waitFor(() => expect(screen.getByLabelText("Commit message")).toHaveValue("draft"));
      expect(screen.getByText("Ada <ada@x.org>")).toBeInTheDocument();
      expect(screen.queryByText("Bob <bob@x.org>")).toBeNull();
      expect(screen.queryByRole("textbox", { name: "Author" })).toBeNull();
    });
  });
});
