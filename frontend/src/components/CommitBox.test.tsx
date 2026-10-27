import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
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

    expect(onCommit).toHaveBeenCalledWith("my message");
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

  it("clears the message on a one-shot request", () => {
    const onClearMessageHandled = vi.fn();
    const { rerender } = render(<CommitBox onCommit={async () => true} disabled={false} onAbortMerge={() => {}} />);
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "kept" } });
    rerender(
      <CommitBox
        onCommit={async () => true}
        disabled={false}
        onAbortMerge={() => {}}
        clearMessage
        onClearMessageHandled={onClearMessageHandled}
      />,
    );
    expect(screen.getByLabelText("Commit message")).toHaveValue("");
    expect(onClearMessageHandled).toHaveBeenCalled();
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

    expect(onCommit).toHaveBeenCalledWith("my message");
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
    expect(onDraftChange).toHaveBeenLastCalledWith({ message: "restored, then edited", lastSeeded: "", stashed: null });

    fireEvent.click(screen.getByText("Commit"));
    await waitFor(() =>
      expect(onDraftChange).toHaveBeenLastCalledWith({ message: "", lastSeeded: "", stashed: null }),
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
      onAmend: vi.fn(),
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
    expect(amend.onAmend).toHaveBeenCalledWith("previous message");
  });

  it("restores the typed draft exactly when toggled off", () => {
    const props = { onCommit: vi.fn(), disabled: true, onAbortMerge: () => {} };
    const amend = {
      available: true,
      headMessage: "previous message",
      pushed: false,
      onToggle: vi.fn(),
      onAmend: vi.fn(),
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
    onAmend: vi.fn(),
  };

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
    const onAmend = vi.fn();
    const { rerender } = render(<CommitBox {...base} amend={{ ...amend, on: false, onAmend }} />);
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "my draft" } });
    rerender(<CommitBox {...base} amend={{ ...amend, on: true, onAmend }} />);
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "new text" } });
    fireEvent.click(screen.getByRole("button", { name: "Amend" }));
    expect(onAmend).toHaveBeenCalledWith("new text");
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
});
