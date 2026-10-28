import { useEffect, useState, type ReactNode } from "react";
import type { GitConfigSource, GitToolInfo, GitTools, RepoClient } from "../ipc/RepoClient";
import { useCommitMessageSettingsContext } from "../state/useCommitMessageSettings";
import { useExternalToolSettings } from "../state/useExternalToolSettings";
import styles from "./PreferencesDialog.module.css";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";

/**
 * The Preferences dialog shell. It holds no state of its own: each child is an independent
 * `PreferencesSection` that loads and saves its own settings, so a new section (4g's
 * "Commit message") is just another child.
 */
export function PreferencesDialog({ onClose, children }: { onClose: () => void; children: ReactNode }) {
  return (
    <FormDialog ariaLabel="Preferences" onCancel={onClose}>
      <h2 className={styles.title}>Preferences</h2>
      <div className={styles.sections}>{children}</div>
      <div className={styles.footer}>
        <button type="button" onClick={onClose}>
          Close
        </button>
      </div>
    </FormDialog>
  );
}

export function PreferencesSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <fieldset className={styles.section}>
      <legend className={styles.legend}>{title}</legend>
      {children}
    </fieldset>
  );
}

/** A 10–200 number field that saves on blur (or Enter), clamped by the settings hook. */
function ColumnField({ label, value, onCommit }: { label: string; value: number; onCommit: (value: number) => void }) {
  // Local text while typing; `null` shows the saved value.
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft === null) return;
    setDraft(null);
    const parsed = Number(draft);
    if (draft.trim() !== "" && Number.isFinite(parsed) && parsed !== value) onCommit(parsed);
  };
  return (
    <Field label={label}>
      {(props) => (
        <input
          {...props}
          type="number"
          min={10}
          max={200}
          step={1}
          className={styles.number}
          value={draft ?? String(value)}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") commit();
          }}
        />
      )}
    </Field>
  );
}

/** "Commit message": the summary length counter's limit and the body ruler. */
export function CommitMessageSection() {
  const { settings, update, error, dismissError } = useCommitMessageSettingsContext();
  return (
    <PreferencesSection title="Commit message">
      <ColumnField
        label="Summary length limit"
        value={settings.summaryLimit}
        onCommit={(summaryLimit) => update({ summaryLimit })}
      />
      <ColumnField label="Ruler column" value={settings.bodyRuler} onCommit={(bodyRuler) => update({ bodyRuler })} />
      <label className={styles.checkbox}>
        <input
          type="checkbox"
          checked={settings.showRuler}
          onChange={(event) => update({ showRuler: event.target.checked })}
        />
        Show ruler
      </label>
      <p className={styles.hint}>The message field uses a monospace font while the ruler is shown.</p>
      {error !== null && (
        <p role="alert" className={styles.problem}>
          {error}{" "}
          <button type="button" onClick={dismissError}>
            Dismiss
          </button>
        </p>
      )}
    </PreferencesSection>
  );
}

const SOURCE_LABEL: Record<GitConfigSource, string> = {
  System: "system git config",
  Xdg: "XDG git config",
  Global: "global git config",
  Local: "repository git config",
  Worktree: "worktree git config",
  Other: "git config",
};

const sourceText = (tool: GitToolInfo) =>
  tool.viaMergeTool ? `from merge.tool in ${SOURCE_LABEL[tool.source]}` : `from ${SOURCE_LABEL[tool.source]}`;

function GitToolRow({ label, tool }: { label: string; tool: GitToolInfo | null }) {
  return (
    <>
      <dt>{label}</dt>
      <dd className={styles.toolValue}>
        {tool === null ? (
          <span className={styles.muted}>Not configured</span>
        ) : (
          <>
            <span>{tool.name}</span>
            {tool.problem !== null ? (
              <span className={styles.problem}>{tool.problem}</span>
            ) : (
              <>
                {tool.command !== null && <code className={styles.command}>{tool.command}</code>}
                <span className={styles.muted}>{sourceText(tool)}</span>
              </>
            )}
          </>
        )}
      </dd>
    </>
  );
}

/**
 * "External tools": the editor and terminal commands (Browsitory settings), plus the git diff
 * and merge tools of the active repository, read-only. In VSCode everything opens in VSCode
 * itself, so there is nothing to set.
 */
export function ExternalToolsSection({
  client,
  repoPath,
  toolHost,
}: {
  client: RepoClient;
  /** The active repository, for its git tools; `null` hides them. */
  repoPath: string | null;
  toolHost: "desktop" | "vscode";
}) {
  if (toolHost === "vscode") {
    return (
      <PreferencesSection title="External tools">
        <p className={styles.note}>In VSCode, files, diffs, merges and terminals open in VSCode itself.</p>
      </PreferencesSection>
    );
  }
  return <DesktopExternalTools client={client} repoPath={repoPath} />;
}

function DesktopExternalTools({ client, repoPath }: { client: RepoClient; repoPath: string | null }) {
  const { settings, save, error } = useExternalToolSettings(client);
  // Local edits; `null` until the user types, so the loaded settings show until then.
  const [editorDraft, setEditorDraft] = useState<string | null>(null);
  const [terminalDraft, setTerminalDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [gitTools, setGitTools] = useState<GitTools | null>(null);
  const [gitToolsError, setGitToolsError] = useState<string | null>(null);

  useEffect(() => {
    if (repoPath === null) return;
    let ignore = false;
    client.getGitTools(repoPath).then(
      (tools) => {
        if (!ignore) setGitTools(tools);
      },
      (err: unknown) => {
        if (!ignore) setGitToolsError(err instanceof Error ? err.message : String(err));
      },
    );
    return () => {
      ignore = true;
    };
  }, [client, repoPath]);

  const editor = editorDraft ?? settings?.editor ?? "";
  const terminal = terminalDraft ?? settings?.terminal ?? "";
  const next = { editor: editor.trim() || null, terminal: terminal.trim() || null };
  const changed = settings !== null && (next.editor !== settings.editor || next.terminal !== settings.terminal);

  const onSave = async () => {
    setSaving(true);
    const ok = await save(next);
    setSaving(false);
    if (ok) {
      setEditorDraft(null);
      setTerminalDraft(null);
    }
  };

  return (
    <PreferencesSection title="External tools">
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (changed && !saving) void onSave();
        }}
      >
        <Field label="Editor command">
          {(props) => (
            <input
              {...props}
              type="text"
              spellCheck={false}
              placeholder="System default app"
              value={editor}
              disabled={settings === null}
              onChange={(event) => setEditorDraft(event.target.value)}
            />
          )}
        </Field>
        <Field label="Terminal command">
          {(props) => (
            <input
              {...props}
              type="text"
              spellCheck={false}
              placeholder="System default terminal"
              value={terminal}
              disabled={settings === null}
              onChange={(event) => setTerminalDraft(event.target.value)}
            />
          )}
        </Field>
        <p className={styles.hint}>Placeholders: {"{file}, {line}, {dir}"}</p>
        {error !== null && (
          <p role="alert" className={styles.problem}>
            {error}
          </p>
        )}
        <button type="submit" disabled={!changed || saving}>
          Save
        </button>
      </form>
      {repoPath !== null && (
        <>
          {gitToolsError !== null && (
            <p role="alert" className={styles.problem}>
              {gitToolsError}
            </p>
          )}
          {gitTools !== null && (
            <dl className={styles.gitTools}>
              <GitToolRow label="Diff tool" tool={gitTools.diffTool} />
              <GitToolRow label="Merge tool" tool={gitTools.mergeTool} />
            </dl>
          )}
        </>
      )}
    </PreferencesSection>
  );
}
