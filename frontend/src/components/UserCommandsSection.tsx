import { useState } from "react";
import type {
  RepoClient,
  UserCommand,
  UserCommandCheck,
  UserCommandInput,
  UserCommandTarget,
} from "../ipc/RepoClient";
import { repoNameOf } from "../lib/repoName";
import { USER_COMMAND_TARGETS, USER_COMMAND_VARIABLES, toUserCommandInput } from "../lib/userCommands";
import { useUserCommands } from "../state/useUserCommands";
import { PreferencesSection } from "./PreferencesDialog";
import styles from "./UserCommandsSection.module.css";
import { Field } from "./primitives/Field";

const message = (err: unknown) => (err instanceof Error ? err.message : String(err));

const TARGET_LABEL: Record<UserCommandTarget, string> = {
  Commit: "Commit",
  Branch: "Branch",
  Tag: "Tag",
  File: "File",
  Repository: "Repository",
};

const BLANK: UserCommandInput = {
  id: "",
  name: "",
  commandLine: "",
  workingDirectory: null,
  targets: [],
  confirm: false,
  showOutput: true,
  refresh: true,
};

/** Which list an editor belongs to, and the command it edits (`null`: a new one). */
interface Editing {
  scope: string | null;
  index: number | null;
}

/**
 * "Commands" in Preferences (#893): the user commands for every repository and the active
 * repository's own, each list ordered as menus and the palette show it. Every change is stored at
 * once (`setUserCommands`), which checks the command line; a refused one keeps its form open with
 * the reason.
 */
export function UserCommandsSection({ client, repoPath }: { client: RepoClient; repoPath: string | null }) {
  const { commands, error, save } = useUserCommands(client, repoPath);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [listError, setListError] = useState<string | null>(null);

  const listOf = (scope: string | null) => (scope === null ? commands?.global : commands?.repository) ?? [];

  const store = async (scope: string | null, next: UserCommandInput[]) => {
    await save(scope, next);
  };

  const change = async (scope: string | null, next: UserCommandInput[]) => {
    try {
      await store(scope, next);
      setListError(null);
    } catch (err) {
      setListError(message(err));
    }
  };

  const lists: { scope: string | null; label: string; addLabel: string }[] = [
    { scope: null, label: "All repositories", addLabel: "Add a command for all repositories" },
    ...(repoPath === null
      ? []
      : [{ scope: repoPath, label: `Only in ${repoNameOf(repoPath)}`, addLabel: `Add a command for ${repoNameOf(repoPath)}` }]),
  ];

  return (
    <PreferencesSection title="Commands">
      <p className={styles.hint}>
        Your own programs, run with the repository and the selection. Each runs as a program with
        arguments, never through a shell. Variables, each one argument:{" "}
        {USER_COMMAND_VARIABLES.map((variable) => `\${${variable}}`).join(", ")}.
      </p>
      {error !== null && (
        <p role="alert" className={styles.problem}>
          {error}
        </p>
      )}
      {listError !== null && (
        <p role="alert" className={styles.problem}>
          {listError}
        </p>
      )}
      {commands !== null &&
        lists.map(({ scope, label, addLabel }) => {
          const list = listOf(scope);
          const inputs = list.map(toUserCommandInput);
          const move = (index: number, by: number) => {
            const next = [...inputs];
            const [moved] = next.splice(index, 1);
            next.splice(index + by, 0, moved);
            void change(scope, next);
          };
          return (
            <section key={scope ?? ""} className={styles.scope}>
              <h3 className={styles.scopeTitle}>{label}</h3>
              {list.length === 0 && <p className={styles.hint}>No commands yet.</p>}
              <ul aria-label={label} className={styles.list}>
                {list.map((command, index) =>
                  editing?.scope === scope && editing.index === index ? (
                    <li key={command.id}>
                      <CommandEditor
                        client={client}
                        title={`Edit ${command.name}`}
                        initial={toUserCommandInput(command)}
                        onCancel={() => setEditing(null)}
                        onSave={async (edited) => {
                          await store(
                            scope,
                            inputs.map((other, at) => (at === index ? edited : other)),
                          );
                          setEditing(null);
                        }}
                      />
                    </li>
                  ) : (
                    <CommandRow
                      key={command.id}
                      command={command}
                      first={index === 0}
                      last={index === list.length - 1}
                      onUp={() => move(index, -1)}
                      onDown={() => move(index, 1)}
                      onEdit={() => setEditing({ scope, index })}
                      onDelete={() => void change(scope, inputs.filter((_, at) => at !== index))}
                    />
                  ),
                )}
              </ul>
              {editing?.scope === scope && editing.index === null ? (
                <CommandEditor
                  client={client}
                  title="New command"
                  initial={BLANK}
                  onCancel={() => setEditing(null)}
                  onSave={async (added) => {
                    await store(scope, [...inputs, added]);
                    setEditing(null);
                  }}
                />
              ) : (
                <button
                  type="button"
                  className={styles.add}
                  aria-label={addLabel}
                  onClick={() => setEditing({ scope, index: null })}
                >
                  Add command
                </button>
              )}
            </section>
          );
        })}
    </PreferencesSection>
  );
}

function CommandRow({
  command,
  first,
  last,
  onUp,
  onDown,
  onEdit,
  onDelete,
}: {
  command: UserCommand;
  first: boolean;
  last: boolean;
  onUp: () => void;
  onDown: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <li className={styles.row}>
      <div className={styles.rowText}>
        <span className={styles.name}>{command.name}</span>
        <code className={styles.commandLine}>{command.commandLine}</code>
        {command.problem !== null && <span className={styles.problem}>{command.problem}</span>}
      </div>
      <div className={styles.rowActions}>
        <button type="button" aria-label={`Move ${command.name} up`} disabled={first} onClick={onUp}>
          ↑
        </button>
        <button type="button" aria-label={`Move ${command.name} down`} disabled={last} onClick={onDown}>
          ↓
        </button>
        <button type="button" aria-label={`Edit ${command.name}`} onClick={onEdit}>
          Edit
        </button>
        <button type="button" aria-label={`Delete ${command.name}`} onClick={onDelete}>
          Delete
        </button>
      </div>
    </li>
  );
}

function checkText(check: UserCommandCheck): string {
  if (check.program === null) return "The program is found when the command runs.";
  if (check.programPath === null) return `Couldn't find the program “${check.program}”.`;
  return `Runs ${check.programPath}`;
}

function CommandEditor({
  client,
  title,
  initial,
  onCancel,
  onSave,
}: {
  client: RepoClient;
  title: string;
  initial: UserCommandInput;
  onCancel: () => void;
  /** Stores the command; rejects with the reason it was refused. */
  onSave: (command: UserCommandInput) => Promise<void>;
}) {
  const [draft, setDraft] = useState<UserCommandInput>(initial);
  const [saving, setSaving] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [check, setCheck] = useState<UserCommandCheck | null>(null);

  const workingDirectory = (draft.workingDirectory ?? "").trim() === "" ? null : (draft.workingDirectory ?? "").trim();
  const ready = draft.name.trim() !== "" && draft.commandLine.trim() !== "" && !saving;
  const update = (patch: Partial<UserCommandInput>) => setDraft((current) => ({ ...current, ...patch }));
  const toggleTarget = (target: UserCommandTarget) =>
    update({
      targets: draft.targets.includes(target)
        ? draft.targets.filter((other) => other !== target)
        : USER_COMMAND_TARGETS.filter((other) => other === target || draft.targets.includes(other)),
    });

  const submit = async () => {
    setSaving(true);
    try {
      await onSave({ ...draft, workingDirectory });
    } catch (err) {
      setProblem(message(err));
      setSaving(false);
    }
  };

  const test = async () => {
    setCheck(null);
    try {
      setCheck(await client.checkUserCommand(draft.commandLine, workingDirectory));
      setProblem(null);
    } catch (err) {
      setProblem(message(err));
    }
  };

  return (
    <form
      aria-label={title}
      className={styles.editor}
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) void submit();
      }}
    >
      <Field label="Name">
        {(props) => (
          <input {...props} type="text" value={draft.name} onChange={(event) => update({ name: event.target.value })} />
        )}
      </Field>
      <Field label="Command">
        {(props) => (
          <input
            {...props}
            type="text"
            spellCheck={false}
            className={styles.mono}
            placeholder="git log -1 ${commit}"
            value={draft.commandLine}
            onChange={(event) => update({ commandLine: event.target.value })}
          />
        )}
      </Field>
      <Field label="Working directory">
        {(props) => (
          <input
            {...props}
            type="text"
            spellCheck={false}
            className={styles.mono}
            placeholder="Repository root"
            value={draft.workingDirectory ?? ""}
            onChange={(event) => update({ workingDirectory: event.target.value })}
          />
        )}
      </Field>
      <fieldset className={styles.targets}>
        <legend>Show in menus</legend>
        {USER_COMMAND_TARGETS.map((target) => (
          <label key={target} className={styles.checkbox}>
            <input type="checkbox" checked={draft.targets.includes(target)} onChange={() => toggleTarget(target)} />
            {TARGET_LABEL[target]}
          </label>
        ))}
      </fieldset>
      <label className={styles.checkbox}>
        <input type="checkbox" checked={draft.confirm} onChange={(event) => update({ confirm: event.target.checked })} />
        Ask before running
      </label>
      <label className={styles.checkbox}>
        <input
          type="checkbox"
          checked={draft.showOutput}
          onChange={(event) => update({ showOutput: event.target.checked })}
        />
        Show output
      </label>
      <label className={styles.checkbox}>
        <input type="checkbox" checked={draft.refresh} onChange={(event) => update({ refresh: event.target.checked })} />
        Refresh the repository afterwards
      </label>
      {problem !== null && (
        <p role="alert" className={styles.problem}>
          {problem}
        </p>
      )}
      {check !== null && (
        <div className={styles.check}>
          <p className={styles.hint}>{checkText(check)}</p>
          <ol aria-label="Arguments" className={styles.args}>
            {check.words.map((word, index) => (
              <li key={index}>
                <code>{word}</code>
              </li>
            ))}
          </ol>
        </div>
      )}
      <div className={styles.buttons}>
        <button type="button" disabled={draft.commandLine.trim() === ""} onClick={() => void test()}>
          Test
        </button>
        <button type="button" onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" disabled={!ready}>
          Save
        </button>
      </div>
    </form>
  );
}
