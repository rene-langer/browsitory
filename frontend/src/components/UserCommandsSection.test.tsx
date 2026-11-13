import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient, UserCommand, UserCommandInput, UserCommands } from "../ipc/RepoClient";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { UserCommandsSection } from "./UserCommandsSection";

function stored(name: string, overrides: Partial<UserCommand> = {}): UserCommand {
  return {
    id: `id-${name}`,
    name,
    commandLine: `echo ${name}`,
    workingDirectory: null,
    targets: [],
    confirm: false,
    showOutput: true,
    refresh: true,
    variables: [],
    problem: null,
    ...overrides,
  };
}

function input(command: UserCommand): UserCommandInput {
  const { variables: _variables, problem: _problem, ...rest } = command;
  return rest;
}

/** A client whose stored lists answer `listUserCommands` and follow `setUserCommands`. */
function storeClient(initial: UserCommands, overrides: Partial<RepoClient> = {}) {
  const lists = { ...initial };
  const setUserCommands = vi.fn(async (repoPath: string | null, commands: UserCommandInput[]) => {
    const saved = commands.map((command, index) =>
      stored(command.name, { ...command, id: command.id === "" ? `new-${index}` : command.id }),
    );
    if (repoPath === null) lists.global = saved;
    else lists.repository = saved;
    return saved;
  });
  const listUserCommands = vi.fn(async () => lists);
  const client = fakeRepoClient({ listUserCommands, setUserCommands, ...overrides });
  return { client, listUserCommands, setUserCommands };
}

function renderSection(client: RepoClient, repoPath: string | null = "/repos/app") {
  render(<UserCommandsSection client={client} repoPath={repoPath} />);
}

describe("UserCommandsSection (#893)", () => {
  it("lists the app-wide commands and the active repository's own, in order", async () => {
    const { client, listUserCommands } = storeClient({
      global: [stored("Lint", { commandLine: "make lint" }), stored("Log")],
      repository: [stored("Deploy", { commandLine: "deploy ${branch}" })],
    });
    renderSection(client);

    const global = await screen.findByRole("list", { name: "All repositories" });
    const items = within(global).getAllByRole("listitem");
    expect(items.map((item) => within(item).getByText(/^(Lint|Log)$/).textContent)).toEqual(["Lint", "Log"]);
    expect(within(items[0]).getByText("make lint")).toBeInTheDocument();
    const repository = screen.getByRole("list", { name: "Only in app" });
    expect(within(repository).getByText("Deploy")).toBeInTheDocument();
    expect(within(repository).getByText("deploy ${branch}")).toBeInTheDocument();
    expect(listUserCommands).toHaveBeenCalledWith("/repos/app");
  });

  it("offers only app-wide commands without an open repository", async () => {
    const { client, listUserCommands } = storeClient({ global: [], repository: [] });
    renderSection(client, null);
    expect(await screen.findByRole("list", { name: "All repositories" })).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: /^Only in/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Add a command for app/ })).not.toBeInTheDocument();
    expect(listUserCommands).toHaveBeenCalledWith(null);
  });

  it("says how to use the variables", async () => {
    const { client } = storeClient({ global: [], repository: [] });
    renderSection(client);
    await screen.findByRole("list", { name: "All repositories" });
    expect(
      screen.getByText(/\$\{repo\}, \$\{commit\}, \$\{shortCommit\}, \$\{branch\}, \$\{ref\}, \$\{path\}, \$\{paths\}/),
    ).toBeInTheDocument();
  });

  it("adds an app-wide command with its menus and options", async () => {
    const { client, setUserCommands } = storeClient({ global: [stored("Lint")], repository: [] });
    renderSection(client);
    fireEvent.click(await screen.findByRole("button", { name: "Add a command for all repositories" }));

    const form = screen.getByRole("form", { name: "New command" });
    fireEvent.change(within(form).getByLabelText("Name"), { target: { value: "Show commit" } });
    fireEvent.change(within(form).getByLabelText("Command"), { target: { value: "git show ${commit}" } });
    fireEvent.change(within(form).getByLabelText("Working directory"), { target: { value: "${repo}/web" } });
    fireEvent.click(within(form).getByRole("checkbox", { name: "Commit" }));
    fireEvent.click(within(form).getByRole("checkbox", { name: "Tag" }));
    fireEvent.click(within(form).getByRole("checkbox", { name: "Ask before running" }));
    // Output and refresh start on.
    expect(within(form).getByRole("checkbox", { name: "Show output" })).toBeChecked();
    expect(within(form).getByRole("checkbox", { name: "Refresh the repository afterwards" })).toBeChecked();
    fireEvent.click(within(form).getByRole("checkbox", { name: "Show output" }));
    fireEvent.click(within(form).getByRole("button", { name: "Save" }));

    await waitFor(() => expect(setUserCommands).toHaveBeenCalledTimes(1));
    expect(setUserCommands).toHaveBeenCalledWith(null, [
      input(stored("Lint")),
      {
        id: "",
        name: "Show commit",
        commandLine: "git show ${commit}",
        workingDirectory: "${repo}/web",
        targets: ["Commit", "Tag"],
        confirm: true,
        showOutput: false,
        refresh: true,
      },
    ]);
    await waitFor(() => expect(screen.queryByRole("form", { name: "New command" })).not.toBeInTheDocument());
    const global = screen.getByRole("list", { name: "All repositories" });
    expect(within(global).getByText("Show commit")).toBeInTheDocument();
  });

  it("adds a command for the active repository only, a blank directory as null", async () => {
    const { client, setUserCommands } = storeClient({ global: [stored("Lint")], repository: [] });
    renderSection(client);
    fireEvent.click(await screen.findByRole("button", { name: "Add a command for app" }));
    const form = screen.getByRole("form", { name: "New command" });
    fireEvent.change(within(form).getByLabelText("Name"), { target: { value: "Deploy" } });
    fireEvent.change(within(form).getByLabelText("Command"), { target: { value: "deploy" } });
    fireEvent.change(within(form).getByLabelText("Working directory"), { target: { value: "   " } });
    fireEvent.click(within(form).getByRole("button", { name: "Save" }));

    await waitFor(() => expect(setUserCommands).toHaveBeenCalledTimes(1));
    expect(setUserCommands).toHaveBeenCalledWith("/repos/app", [
      {
        id: "",
        name: "Deploy",
        commandLine: "deploy",
        workingDirectory: null,
        targets: [],
        confirm: false,
        showOutput: true,
        refresh: true,
      },
    ]);
    expect(await within(screen.getByRole("list", { name: "Only in app" })).findByText("Deploy")).toBeInTheDocument();
  });

  it("edits a command in place, keeping its id", async () => {
    const lint = stored("Lint", { targets: ["File"], confirm: true });
    const { client, setUserCommands } = storeClient({ global: [lint, stored("Log")], repository: [] });
    renderSection(client);
    fireEvent.click(await screen.findByRole("button", { name: "Edit Lint" }));

    const form = screen.getByRole("form", { name: "Edit Lint" });
    expect(within(form).getByLabelText("Name")).toHaveValue("Lint");
    expect(within(form).getByLabelText("Command")).toHaveValue("echo Lint");
    expect(within(form).getByRole("checkbox", { name: "File" })).toBeChecked();
    expect(within(form).getByRole("checkbox", { name: "Ask before running" })).toBeChecked();
    fireEvent.change(within(form).getByLabelText("Command"), { target: { value: "make lint ${paths}" } });
    fireEvent.click(within(form).getByRole("button", { name: "Save" }));

    await waitFor(() => expect(setUserCommands).toHaveBeenCalledTimes(1));
    expect(setUserCommands).toHaveBeenCalledWith(null, [
      { ...input(lint), commandLine: "make lint ${paths}" },
      input(stored("Log")),
    ]);
  });

  it("cancels an edit without saving", async () => {
    const { client, setUserCommands } = storeClient({ global: [stored("Lint")], repository: [] });
    renderSection(client);
    fireEvent.click(await screen.findByRole("button", { name: "Edit Lint" }));
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Other" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("form", { name: "Edit Lint" })).not.toBeInTheDocument();
    expect(screen.getByText("Lint")).toBeInTheDocument();
    expect(setUserCommands).not.toHaveBeenCalled();
  });

  it("moves a command up and down and deletes one", async () => {
    const { client, setUserCommands } = storeClient({
      global: [stored("A"), stored("B"), stored("C")],
      repository: [],
    });
    renderSection(client);
    await screen.findByRole("list", { name: "All repositories" });
    expect(screen.getByRole("button", { name: "Move A up" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Move C down" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Move C up" }));
    await waitFor(() =>
      expect(setUserCommands).toHaveBeenLastCalledWith(null, [stored("A"), stored("C"), stored("B")].map(input)),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Move A down" }));
    await waitFor(() =>
      expect(setUserCommands).toHaveBeenLastCalledWith(null, [stored("C"), stored("A"), stored("B")].map(input)),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Delete A" }));
    await waitFor(() =>
      expect(setUserCommands).toHaveBeenLastCalledWith(null, [stored("C"), stored("B")].map(input)),
    );
    await waitFor(() => expect(screen.queryByText("A")).not.toBeInTheDocument());
  });

  it("shows why a save was refused and keeps the form open", async () => {
    const { client } = storeClient(
      { global: [], repository: [] },
      {
        setUserCommands: async () => {
          throw new Error("Broken: Unknown variable ${foo}: use ${repo}, … or ${paths}.");
        },
      },
    );
    renderSection(client);
    fireEvent.click(await screen.findByRole("button", { name: "Add a command for all repositories" }));
    const form = screen.getByRole("form", { name: "New command" });
    fireEvent.change(within(form).getByLabelText("Name"), { target: { value: "Broken" } });
    fireEvent.change(within(form).getByLabelText("Command"), { target: { value: "echo ${foo}" } });
    fireEvent.click(within(form).getByRole("button", { name: "Save" }));
    expect(await within(form).findByRole("alert")).toHaveTextContent("Unknown variable ${foo}");
    expect(within(form).getByLabelText("Command")).toHaveValue("echo ${foo}");
  });

  it("disables Save until a name and a command are typed", async () => {
    const { client } = storeClient({ global: [], repository: [] });
    renderSection(client);
    fireEvent.click(await screen.findByRole("button", { name: "Add a command for all repositories" }));
    const form = screen.getByRole("form", { name: "New command" });
    const save = within(form).getByRole("button", { name: "Save" });
    expect(save).toBeDisabled();
    fireEvent.change(within(form).getByLabelText("Name"), { target: { value: "X" } });
    expect(save).toBeDisabled();
    fireEvent.change(within(form).getByLabelText("Command"), { target: { value: "  " } });
    expect(save).toBeDisabled();
    fireEvent.change(within(form).getByLabelText("Command"), { target: { value: "true" } });
    expect(save).toBeEnabled();
  });

  it("tests a command line: its arguments and where the program is", async () => {
    const checkUserCommand = vi.fn(async () => ({
      words: ["git", "show", "${shortCommit} notes"],
      program: "git",
      programPath: "/usr/bin/git",
      variables: ["shortCommit" as const],
    }));
    const { client } = storeClient({ global: [], repository: [] }, { checkUserCommand });
    renderSection(client);
    fireEvent.click(await screen.findByRole("button", { name: "Add a command for all repositories" }));
    const form = screen.getByRole("form", { name: "New command" });
    fireEvent.change(within(form).getByLabelText("Command"), { target: { value: "git show '${shortCommit} notes'" } });
    fireEvent.change(within(form).getByLabelText("Working directory"), { target: { value: "sub" } });
    fireEvent.click(within(form).getByRole("button", { name: "Test" }));

    expect(checkUserCommand).toHaveBeenCalledWith("git show '${shortCommit} notes'", "sub");
    const args = await within(form).findByRole("list", { name: "Arguments" });
    expect(within(args).getAllByRole("listitem").map((item) => item.textContent)).toEqual([
      "git",
      "show",
      "${shortCommit} notes",
    ]);
    expect(within(form).getByText("Runs /usr/bin/git")).toBeInTheDocument();
  });

  it("tests a command whose program isn't found, or isn't known before it runs", async () => {
    const answers = [
      { words: ["nope"], program: "nope", programPath: null, variables: [] },
      { words: ["${repo}/run.sh"], program: null, programPath: null, variables: ["repo" as const] },
    ];
    const checkUserCommand = vi.fn(async () => answers.shift()!);
    const { client } = storeClient({ global: [], repository: [] }, { checkUserCommand });
    renderSection(client);
    fireEvent.click(await screen.findByRole("button", { name: "Add a command for all repositories" }));
    const form = screen.getByRole("form", { name: "New command" });
    fireEvent.change(within(form).getByLabelText("Command"), { target: { value: "nope" } });
    fireEvent.click(within(form).getByRole("button", { name: "Test" }));
    expect(await within(form).findByText("Couldn't find the program “nope”.")).toBeInTheDocument();

    fireEvent.change(within(form).getByLabelText("Command"), { target: { value: "${repo}/run.sh" } });
    fireEvent.click(within(form).getByRole("button", { name: "Test" }));
    expect(await within(form).findByText("The program is found when the command runs.")).toBeInTheDocument();
  });

  it("shows why a test failed", async () => {
    const { client } = storeClient(
      { global: [], repository: [] },
      {
        checkUserCommand: async () => {
          throw new Error("${paths} must be an argument of its own.");
        },
      },
    );
    renderSection(client);
    fireEvent.click(await screen.findByRole("button", { name: "Add a command for all repositories" }));
    const form = screen.getByRole("form", { name: "New command" });
    fireEvent.change(within(form).getByLabelText("Command"), { target: { value: "x --a=${paths}" } });
    fireEvent.click(within(form).getByRole("button", { name: "Test" }));
    expect(await within(form).findByRole("alert")).toHaveTextContent("${paths} must be an argument of its own.");
  });

  it("marks a hand-written command that can't run", async () => {
    const { client } = storeClient({
      global: [stored("Odd", { commandLine: "echo ${what}", problem: "Unknown variable ${what}." })],
      repository: [],
    });
    renderSection(client);
    const item = (await screen.findByText("Odd")).closest("li")!;
    expect(within(item).getByText("Unknown variable ${what}.")).toBeInTheDocument();
  });

  it("shows a failed load", async () => {
    const client = fakeRepoClient({
      listUserCommands: async () => {
        throw new Error("config unreadable");
      },
    });
    renderSection(client);
    expect(await screen.findByRole("alert")).toHaveTextContent("config unreadable");
  });
});
