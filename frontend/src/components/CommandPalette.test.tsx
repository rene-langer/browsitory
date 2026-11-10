import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CommandPalette } from "./CommandPalette";
import { recordCommandUsed, type Command } from "../lib/commands";

function makeCommands(): Command[] {
  return [
    { id: "switch-main", label: "Switch to main", keywords: ["branch"], run: vi.fn(), group: "General" },
    { id: "switch-feature", label: "Switch to feature", keywords: ["branch"], run: vi.fn(), group: "General" },
    { id: "fetch-origin", label: "Fetch origin", keywords: ["remote"], run: vi.fn(), group: "General" },
  ];
}

describe("CommandPalette", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("opens straight into a picker, and Backspace still leads back to the commands (#638)", () => {
    const pick = vi.fn();
    const commands: Command[] = [
      ...makeCommands(),
      {
        id: "switch-branch",
        label: "Switch branch…",
        keywords: [],
        run: vi.fn(),
        group: "Branches",
        picker: { placeholder: "Branch…", emptyText: "None", items: () => [{ id: "b:dev", label: "dev", keywords: [], run: pick }] },
      },
    ];
    const onRun = vi.fn();
    render(<CommandPalette commands={commands} onRun={onRun} initialPickerId="switch-branch" />);
    const input = screen.getByRole("combobox", { name: "Switch branch" });
    expect(screen.getByRole("option", { name: "dev" })).toBeInTheDocument();
    fireEvent.keyDown(input, { key: "Backspace" });
    expect(screen.getByRole("combobox", { name: "Command palette" })).toBeInTheDocument();
    expect(screen.getByText("Fetch origin")).toBeInTheDocument();
  });

  it("renders every command when the query is empty", () => {
    render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
    expect(screen.getByText("Switch to main")).toBeInTheDocument();
    expect(screen.getByText("Switch to feature")).toBeInTheDocument();
    expect(screen.getByText("Fetch origin")).toBeInTheDocument();
  });

  it("filters as the user types", () => {
    render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "fetch" } });
    expect(screen.getByRole("option", { name: "Fetch origin" })).toBeInTheDocument();
    expect(screen.queryByText("Switch to main")).not.toBeInTheDocument();
  });

  it("runs the highlighted command and calls onRun when Enter is pressed", () => {
    const commands = makeCommands();
    const onRun = vi.fn();
    render(<CommandPalette commands={commands} onRun={onRun} />);
    fireEvent.keyDown(screen.getByRole("combobox"), { key: "Enter" });
    // With an empty query, filterAndSortCommands (Task 1) sorts tied-score results
    // alphabetically by label, so "Fetch origin" (commands[2]) is highlighted first —
    // not the original array order.
    expect(commands[2].run).toHaveBeenCalledOnce();
    expect(onRun).toHaveBeenCalledOnce();
  });

  it("moves the highlight with arrow keys before running", () => {
    const commands = makeCommands();
    render(<CommandPalette commands={commands} onRun={vi.fn()} />);
    const input = screen.getByRole("combobox");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(commands[1].run).toHaveBeenCalledOnce();
    expect(commands[0].run).not.toHaveBeenCalled();
  });

  it("runs a command on click regardless of highlight", () => {
    const commands = makeCommands();
    const onRun = vi.fn();
    render(<CommandPalette commands={commands} onRun={onRun} />);
    fireEvent.click(screen.getByText("Fetch origin"));
    expect(commands[2].run).toHaveBeenCalledOnce();
    expect(onRun).toHaveBeenCalledOnce();
  });

  it("records the run command as recently used", () => {
    const commands = makeCommands();
    render(<CommandPalette commands={commands} onRun={vi.fn()} />);
    fireEvent.click(screen.getByText("Fetch origin"));
    expect(localStorage.getItem("command-palette-recent")).toContain("fetch-origin");
  });

  it("shows an empty-state message when nothing matches", () => {
    render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "zzz-no-match" } });
    expect(screen.getByText("No matching commands")).toBeInTheDocument();
  });

  it("wires the input as a combobox controlling a labeled listbox", () => {
    render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
    const input = screen.getByRole("combobox");
    const listbox = screen.getByRole("listbox", { name: "Command results" });
    expect(input).toHaveAttribute("aria-expanded", "true");
    expect(input).toHaveAttribute("aria-controls", listbox.id);
  });

  it("points aria-activedescendant at the highlighted row", () => {
    const commands = makeCommands();
    render(<CommandPalette commands={commands} onRun={vi.fn()} />);
    const input = screen.getByRole("combobox");
    // Empty query, alphabetical tiebreak: "Fetch origin" (commands[2]) highlighted first.
    const options = screen.getAllByRole("option");
    expect(input).toHaveAttribute("aria-activedescendant", options[0].id);
    expect(options[0]).toHaveTextContent("Fetch origin");

    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(input).toHaveAttribute("aria-activedescendant", options[1].id);
  });

  it("has aria-expanded=false and no aria-activedescendant when nothing matches", () => {
    render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: "zzz-no-match" } });
    expect(input).toHaveAttribute("aria-expanded", "false");
    expect(input).not.toHaveAttribute("aria-activedescendant");
  });

  describe("grouping and hints", () => {
    const grouped = (): Command[] => [
      { id: "fetch-remote:origin", label: "Fetch origin", keywords: [], run: vi.fn(), group: "Remotes" },
      { id: "switch-branch:dev", label: "Switch to dev", keywords: [], run: vi.fn(), group: "Branches" },
      { id: "go-to:Tags", label: "Go to Tags", keywords: [], run: vi.fn(), group: "Go to" },
      { id: "apply-stash:0", label: "Apply stash: wip", keywords: [], run: vi.fn(), group: "Stash" },
    ];

    it("groups commands under kind headings when the query is empty", () => {
      render(<CommandPalette commands={grouped()} onRun={vi.fn()} />);
      for (const name of ["Branches", "Remotes", "Stash", "Go to"]) {
        expect(screen.getByText(name)).toBeInTheDocument();
      }
      // Keyboard order follows the displayed (grouped) order: Branches comes first.
      const input = screen.getByRole("combobox");
      expect(input.getAttribute("aria-activedescendant")).toBe(
        screen.getByText("Switch to dev").closest("li")!.id,
      );
    });

    it("shows a flat, headingless list while a query is typed", () => {
      render(<CommandPalette commands={grouped()} onRun={vi.fn()} />);
      fireEvent.change(screen.getByRole("combobox"), { target: { value: "s" } });
      expect(screen.queryByText("Branches")).not.toBeInTheDocument();
    });

    it("shows a footer hint for the keys", () => {
      render(<CommandPalette commands={grouped()} onRun={vi.fn()} />);
      expect(screen.getByText(/Enter to run/)).toBeInTheDocument();
      expect(screen.getByText(/Esc to close/)).toBeInTheDocument();
    });
  });
  describe("pickers (#441)", () => {
    function pickerCommands() {
      const goMain = vi.fn();
      const goFeature = vi.fn();
      const items = [
        { id: "branch:main", label: "main", keywords: ["main"], run: goMain },
        { id: "branch:feature", label: "feature", keywords: ["feature"], run: goFeature },
      ];
      const pickerRun = vi.fn();
      const commands: Command[] = [
        {
          id: "go-to-branch",
          label: "Go to branch…",
          keywords: [],
          run: pickerRun,
          group: "Go to",
          picker: {
            placeholder: "Branch name…",
            emptyText: "No matching branches",
            items: (query) => items.filter((item) => item.label.includes(query)),
          },
        },
        { id: "fetch-origin", label: "Fetch origin", keywords: [], run: vi.fn(), group: "General" },
      ];
      return { commands, goMain, goFeature, pickerRun };
    }

    it("opens a picker's items instead of running it, then runs the chosen item", () => {
      const { commands, goFeature, pickerRun } = pickerCommands();
      const onRun = vi.fn();
      render(<CommandPalette commands={commands} onRun={onRun} />);
      const input = screen.getByRole("combobox");
      fireEvent.change(input, { target: { value: "go to br" } });
      fireEvent.keyDown(input, { key: "Enter" });

      expect(pickerRun).not.toHaveBeenCalled();
      expect(onRun).not.toHaveBeenCalled();
      expect(input).toHaveValue("");
      expect(input).toHaveAttribute("placeholder", "Branch name…");
      expect(input).toHaveAccessibleName("Go to branch");
      expect(screen.getByText("main")).toBeInTheDocument();
      expect(screen.queryByText("Fetch origin")).not.toBeInTheDocument();

      fireEvent.change(input, { target: { value: "feat" } });
      fireEvent.keyDown(input, { key: "Enter" });
      expect(goFeature).toHaveBeenCalledOnce();
      expect(onRun).toHaveBeenCalledOnce();
    });

    it("shows the picker's empty text and goes back with Backspace on an empty query", () => {
      const { commands } = pickerCommands();
      render(<CommandPalette commands={commands} onRun={vi.fn()} />);
      fireEvent.click(screen.getByText("Go to branch…"));
      const input = screen.getByRole("combobox");
      fireEvent.change(input, { target: { value: "zzz" } });
      expect(screen.getByText("No matching branches")).toBeInTheDocument();

      fireEvent.change(input, { target: { value: "" } });
      fireEvent.keyDown(input, { key: "Backspace" });
      expect(screen.getByText("Fetch origin")).toBeInTheDocument();
      expect(input).toHaveAttribute("placeholder", "Type a command…");
    });

    it("lists a picker in the recent commands once opened", () => {
      const { commands } = pickerCommands();
      render(<CommandPalette commands={commands} onRun={vi.fn()} />);
      fireEvent.click(screen.getByText("Go to branch…"));
      expect(JSON.parse(localStorage.getItem("command-palette-recent") ?? "[]")).toEqual(["go-to-branch"]);
    });
  });
  describe("keeping the highlight in view (#643)", () => {
    const many = (): Command[] =>
      Array.from({ length: 30 }, (_, i) => ({
        id: `cmd-${i}`,
        label: `Command ${String(i).padStart(2, "0")}`,
        keywords: [],
        run: vi.fn(),
        group: "General" as const,
      }));
    let scrollIntoView: ReturnType<typeof vi.fn>;
    beforeEach(() => {
      scrollIntoView = vi.fn();
      Element.prototype.scrollIntoView = scrollIntoView as unknown as Element["scrollIntoView"];
    });
    afterEach(() => {
      delete (Element.prototype as Partial<Element>).scrollIntoView;
    });

    const active = () => {
      const id = screen.getByRole("combobox").getAttribute("aria-activedescendant");
      return id === null ? null : document.getElementById(id);
    };

    it("scrolls the highlighted row into view as it moves", () => {
      render(<CommandPalette commands={many()} onRun={vi.fn()} />);
      fireEvent.keyDown(screen.getByRole("combobox"), { key: "ArrowDown" });
      expect(active()).toHaveTextContent("Command 01");
      expect(scrollIntoView).toHaveBeenLastCalledWith({ block: "nearest" });
      expect(scrollIntoView.mock.contexts.at(-1)).toBe(active());
    });

    it("pages with PageDown/PageUp and jumps with Ctrl+End/Ctrl+Home", () => {
      render(<CommandPalette commands={many()} onRun={vi.fn()} />);
      const input = screen.getByRole("combobox");
      fireEvent.keyDown(input, { key: "PageDown" });
      expect(active()).toHaveTextContent("Command 10");
      fireEvent.keyDown(input, { key: "PageUp" });
      expect(active()).toHaveTextContent("Command 00");
      fireEvent.keyDown(input, { key: "End", ctrlKey: true });
      expect(active()).toHaveTextContent("Command 29");
      fireEvent.keyDown(input, { key: "Home", ctrlKey: true });
      expect(active()).toHaveTextContent("Command 00");
    });

    it("jumps with plain Home/End only while the query is empty, leaving the caret keys to the text", () => {
      render(<CommandPalette commands={many()} onRun={vi.fn()} />);
      const input = screen.getByRole("combobox");
      fireEvent.keyDown(input, { key: "End" });
      expect(active()).toHaveTextContent("Command 29");

      fireEvent.change(input, { target: { value: "command" } });
      const endEvent = fireEvent.keyDown(input, { key: "End" });
      expect(endEvent).toBe(true); // not prevented: the caret moves
      expect(active()).toHaveTextContent("Command 00");
    });

    it("moves the highlight to the row under the pointer", () => {
      render(<CommandPalette commands={many()} onRun={vi.fn()} />);
      fireEvent.mouseMove(screen.getByText("Command 05"));
      expect(active()).toHaveTextContent("Command 05");
      fireEvent.keyDown(screen.getByRole("combobox"), { key: "ArrowDown" });
      expect(active()).toHaveTextContent("Command 06");
    });
  });

  describe("browsing and matching (#644)", () => {
    it("lists every command with an empty query, beyond the filtered cap", () => {
      const commands: Command[] = Array.from({ length: 70 }, (_, i) => ({
        id: `cmd-${i}`,
        label: `Command ${i}`,
        keywords: [],
        run: vi.fn(),
        group: "General",
      }));
      render(<CommandPalette commands={commands} onRun={vi.fn()} />);
      expect(screen.getAllByRole("option")).toHaveLength(70);
    });

    it("lists recently used commands first, under their own heading, and only once", () => {
      recordCommandUsed("fetch-origin");
      recordCommandUsed("switch-feature");
      render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
      const listbox = screen.getByRole("listbox");
      const items = Array.from(listbox.children).map((li) => li.textContent);
      expect(items).toEqual(["Recent", "Switch to feature", "Fetch origin", "General", "Switch to main"]);
      expect(screen.getByRole("combobox")).toHaveAttribute(
        "aria-activedescendant",
        screen.getByText("Switch to feature").closest("li")!.id,
      );
    });

    it("skips recent ids that no longer name a command", () => {
      recordCommandUsed("gone");
      render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
      expect(screen.queryByText("Recent")).not.toBeInTheDocument();
    });

    it("matches the words in any order and highlights the matched letters", () => {
      render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
      fireEvent.change(screen.getByRole("combobox"), { target: { value: "main sw" } });
      const options = screen.getAllByRole("option");
      expect(options).toHaveLength(1);
      expect(options[0]).toHaveTextContent("Switch to main");
      expect(Array.from(options[0].querySelectorAll("mark")).map((mark) => mark.textContent)).toEqual(["Sw", "main"]);
    });
  });
});
