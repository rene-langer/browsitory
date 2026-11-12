import { useEffect, useId, useRef, useState, useSyncExternalStore, type KeyboardEvent, type ReactNode } from "react";
import {
  CODE_FONT_SIZES,
  COMMIT_MESSAGE_COLUMNS,
  DATE_FORMAT_VALUES,
  DIFF_ALGORITHM_VALUES,
  TAB_SIZES,
  UI_SIZE_VALUES,
  WORD_WRAP_VALUES,
  type DateFormat,
  type DiffAlgorithm,
  type GitConfigSource,
  type GitToolInfo,
  type GitTools,
  type RepoClient,
  type UiSize,
  type WordWrap,
} from "../ipc/RepoClient";
import { formatAbsoluteDate } from "../lib/formatDate";
import { useAppearanceSettingsContext } from "../state/useAppearanceSettings";
import { useCommitMessageSettingsContext } from "../state/useCommitMessageSettings";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { useFileListLayout, writeFileListLayout } from "../lib/fileListLayoutPreference";
import { useExternalToolSettings } from "../state/useExternalToolSettings";
import { useFetchSettingsContext } from "../state/useFetchSettings";
import { useUpdateSettingsContext } from "../state/useUpdateSettings";
import type { UpdateCheckOutcome } from "../state/updateCheck";
import {
  AUTO_FETCH_CHOICES,
  autoFetchLabel,
  readAutoFetchOverride,
  subscribeAutoFetchOverride,
  writeAutoFetchOverride,
} from "../lib/autoFetch";
import { readPreferencesGroup, writePreferencesGroup, type PreferencesGroupId } from "../lib/preferencesGroup";
import { repoNameOf } from "../lib/repoName";
import { THEME_PREFERENCES, type ThemePreference } from "../lib/theme";
import styles from "./PreferencesDialog.module.css";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";

export type { PreferencesGroupId } from "../lib/preferencesGroup";

/** One entry of the category list and the settings it shows (`content`, usually one section). */
export interface PreferencesGroup {
  id: PreferencesGroupId;
  label: string;
  content: ReactNode;
}

/** The groups that have a setting labelled with `query` (or are named so); `null` without one. */
function matchingGroups(
  query: string,
  groups: PreferencesGroup[],
  panels: Map<PreferencesGroupId, HTMLElement>,
): Set<PreferencesGroupId> | null {
  const needle = query.trim().toLowerCase();
  if (needle === "") return null;
  const matches = new Set<PreferencesGroupId>();
  for (const group of groups) {
    // Every field's name is a <label> (`Field`, and the checkboxes), so labels are what we search.
    const labels = [group.label, ...Array.from(panels.get(group.id)?.querySelectorAll("label") ?? [], (label) => label.textContent ?? "")];
    if (labels.some((label) => label.toLowerCase().includes(needle))) matches.add(group.id);
  }
  return matches;
}

function matchText(query: string, matches: Set<PreferencesGroupId> | null): string {
  if (matches === null) return "";
  if (matches.size === 0) return `No settings match “${query.trim()}”`;
  return matches.size === 1 ? "1 group matches" : `${matches.size} groups match`;
}

/**
 * The Preferences dialog shell (#784): a filter and the category list on the left, the selected
 * group's settings on the right (the list above them in a narrow window). It holds no settings of
 * its own: each group's content is an independent `PreferencesSection` that loads and saves its
 * own. Every group stays mounted, hidden while another one is shown, so a draft survives a switch
 * and the filter can search every group's labels. It opens on `initialGroup`, else on the group
 * shown last on this machine, else on the first.
 */
export function PreferencesDialog({
  onClose,
  groups,
  initialGroup,
}: {
  onClose: () => void;
  groups: PreferencesGroup[];
  initialGroup?: PreferencesGroupId;
}) {
  const baseId = useId();
  const offered = (id: PreferencesGroupId | null | undefined): id is PreferencesGroupId =>
    id !== null && id !== undefined && groups.some((group) => group.id === id);
  const [selected, setSelected] = useState<PreferencesGroupId | undefined>(() => {
    if (offered(initialGroup)) return initialGroup;
    const remembered = readPreferencesGroup();
    return offered(remembered) ? remembered : groups[0]?.id;
  });
  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState<Set<PreferencesGroupId> | null>(null);
  const panels = useRef(new Map<PreferencesGroupId, HTMLElement>());
  const tablistRef = useRef<HTMLDivElement>(null);

  const select = (id: PreferencesGroupId) => {
    setSelected(id);
    writePreferencesGroup(id);
  };
  const listed = matches === null ? groups : groups.filter((group) => matches.has(group.id));
  const shown = listed.some((group) => group.id === selected) ? selected : undefined;

  const filter = (text: string) => {
    setQuery(text);
    const next = matchingGroups(text, groups, panels.current);
    setMatches(next);
    // The shown group stays while it matches; otherwise the first match takes its place.
    if (next !== null && selected !== undefined && !next.has(selected)) {
      const first = groups.find((group) => next.has(group.id));
      if (first !== undefined) select(first.id);
    }
  };

  const onTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const keys = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End"];
    if (!keys.includes(event.key)) return;
    event.preventDefault();
    const step = event.key === "ArrowDown" || event.key === "ArrowRight" ? 1 : -1;
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? listed.length - 1
          : (index + step + listed.length) % listed.length;
    select(listed[next].id);
    tablistRef.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
  };

  return (
    <FormDialog ariaLabel="Preferences" onCancel={onClose} className={styles.dialog}>
      <h2 className={styles.title}>Preferences</h2>
      <div className={styles.body}>
        <div className={styles.nav}>
          <input
            type="search"
            className={styles.filter}
            aria-label="Filter settings"
            placeholder="Filter settings"
            spellCheck={false}
            data-autofocus
            value={query}
            onChange={(event) => filter(event.target.value)}
            onKeyDown={(event) => {
              // Escape clears a filter first; the next one closes the dialog.
              if (event.key === "Escape" && query !== "") {
                event.preventDefault();
                event.stopPropagation();
                filter("");
              }
            }}
          />
          <p role="status" className={styles.matchCount}>
            {matchText(query, matches)}
          </p>
          <div
            ref={tablistRef}
            role="tablist"
            aria-label="Preference groups"
            aria-orientation="vertical"
            className={styles.tablist}
          >
            {listed.map((group, index) => (
              <button
                key={group.id}
                type="button"
                role="tab"
                id={`${baseId}-tab-${group.id}`}
                aria-selected={group.id === shown}
                aria-controls={`${baseId}-panel-${group.id}`}
                tabIndex={group.id === (shown ?? listed[0]?.id) ? 0 : -1}
                className={styles.tab}
                onClick={() => select(group.id)}
                onKeyDown={(event) => onTabKeyDown(event, index)}
              >
                {group.label}
              </button>
            ))}
          </div>
        </div>
        <div className={styles.panes}>
          {groups.map((group) => (
            <div
              key={group.id}
              ref={(element) => {
                if (element === null) panels.current.delete(group.id);
                else panels.current.set(group.id, element);
              }}
              role="tabpanel"
              id={`${baseId}-panel-${group.id}`}
              aria-labelledby={`${baseId}-tab-${group.id}`}
              hidden={group.id !== shown}
              className={styles.panel}
            >
              {group.content}
            </div>
          ))}
        </div>
      </div>
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

/** A `COMMIT_MESSAGE_COLUMNS` number field that saves on blur (or Enter), clamped by the settings hook. */
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
          min={COMMIT_MESSAGE_COLUMNS.min}
          max={COMMIT_MESSAGE_COLUMNS.max}
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

const WORD_WRAP_LABEL: Record<WordWrap, string> = { Auto: "Auto", On: "On", Off: "Off" };
const DIFF_ALGORITHM_LABEL: Record<DiffAlgorithm, string> = {
  Default: "Default (Myers)",
  Patience: "Patience",
  Minimal: "Minimal",
};

/**
 * "Diff": wrapping long lines (#444; the diff toolbar's "Wrap lines" sets the same value, #640),
 * the diff algorithm every diff uses (#469), whether untracked files load at once (#623), and the
 * working tree's file list layout (#729, a per-machine preference, not a diff setting).
 */
export function DiffSection() {
  const { settings, update, error, dismissError } = useDiffSettingsContext();
  const fileListLayout = useFileListLayout();
  return (
    <PreferencesSection title="Diff">
      <Field label="Word wrap">
        {(props) => (
          <select
            {...props}
            value={settings.wordWrap}
            onChange={(event) => update({ wordWrap: event.target.value as WordWrap })}
          >
            {WORD_WRAP_VALUES.map((value) => (
              <option key={value} value={value}>
                {WORD_WRAP_LABEL[value]}
              </option>
            ))}
          </select>
        )}
      </Field>
      <p className={styles.hint}>
        Auto wraps the split view, prose files and diffs with very long lines. The Wrap lines button
        in the diff toolbar switches between On and Off.
      </p>
      <Field label="Diff algorithm">
        {(props) => (
          <select
            {...props}
            value={settings.algorithm}
            onChange={(event) => update({ algorithm: event.target.value as DiffAlgorithm })}
          >
            {DIFF_ALGORITHM_VALUES.map((value) => (
              <option key={value} value={value}>
                {DIFF_ALGORITHM_LABEL[value]}
              </option>
            ))}
          </select>
        )}
      </Field>
      <p className={styles.hint}>
        Patience lines up the lines that occur once on each side, which often keeps moved or
        reordered blocks readable. Minimal searches longer for the smallest diff.
      </p>
      <label className={styles.checkbox}>
        <input
          type="checkbox"
          checked={settings.expandUntracked}
          onChange={(event) => update({ expandUntracked: event.target.checked })}
        />
        Expand untracked files
      </label>
      <p className={styles.hint}>
        Off, an untracked file's contents load only when you ask for them in its diff. Changes to
        tracked files always load at once.
      </p>
      <label className={styles.checkbox}>
        <input
          type="checkbox"
          checked={fileListLayout === "tree"}
          onChange={(event) => writeFileListLayout(event.target.checked ? "tree" : "flat")}
        />
        Show changed files as a tree
      </label>
      <p className={styles.hint}>
        Groups the working tree&apos;s changed files by folder. The Flat and Tree buttons above the
        file list switch it too. Kept on this computer.
      </p>
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

/** Off, the offered intervals, and `current` when the config holds another one. */
function intervalChoices(current: number): number[] {
  const choices: number[] = [0, ...AUTO_FETCH_CHOICES];
  if (current > 0 && !choices.includes(current)) choices.push(current);
  return choices.sort((a, b) => a - b);
}

/**
 * "Fetch": the app-wide "Fetch automatically" interval (`[fetch]` in config.toml), and the
 * active repository's override of it (a per-repository preference, like Prune).
 */
export function FetchSection({ repoPath }: { repoPath: string | null }) {
  const { settings, update, error, dismissError } = useFetchSettingsContext();
  const override = useSyncExternalStore(subscribeAutoFetchOverride, () =>
    repoPath === null ? null : readAutoFetchOverride(repoPath),
  );
  const appMinutes = settings.autoFetchMinutes;
  return (
    <PreferencesSection title="Fetch">
      <Field label="Fetch automatically">
        {(props) => (
          <select
            {...props}
            value={String(appMinutes)}
            onChange={(event) => update({ autoFetchMinutes: Number(event.target.value) })}
          >
            {intervalChoices(appMinutes).map((minutes) => (
              <option key={minutes} value={String(minutes)}>
                {autoFetchLabel(minutes)}
              </option>
            ))}
          </select>
        )}
      </Field>
      {repoPath !== null && (
        <Field label={`In ${repoNameOf(repoPath)}`}>
          {(props) => (
            <select
              {...props}
              value={override === null ? "app" : String(override)}
              onChange={(event) =>
                writeAutoFetchOverride(repoPath, event.target.value === "app" ? null : Number(event.target.value))
              }
            >
              <option value="app">Use the app setting ({autoFetchLabel(appMinutes).toLowerCase()})</option>
              {intervalChoices(override ?? 0).map((minutes) => (
                <option key={minutes} value={String(minutes)}>
                  {autoFetchLabel(minutes)}
                </option>
              ))}
            </select>
          )}
        </Field>
      )}
      <p className={styles.hint}>
        Fetches all remotes in the background while Browsitory is visible and no other action runs, using
        each repository's Prune choice. A remote that would need you to sign in is skipped and named once.
      </p>
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

/**
 * A text or number field that saves on blur (or Enter), for settings a keystroke must not save.
 * `onCommit` gets the trimmed text, `""` when cleared.
 */
function CommitOnBlurField({
  label,
  value,
  onCommit,
  ...input
}: {
  label: string;
  value: string;
  onCommit: (value: string) => void;
  type: "text" | "number";
  placeholder?: string;
  min?: number;
  max?: number;
}) {
  // Local text while typing; `null` shows the saved value.
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft === null) return;
    setDraft(null);
    if (draft.trim() !== value) onCommit(draft.trim());
  };
  return (
    <Field label={label}>
      {(props) => (
        <input
          {...props}
          {...input}
          step={input.type === "number" ? 1 : undefined}
          spellCheck={false}
          className={input.type === "number" ? styles.number : undefined}
          value={draft ?? value}
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

const UI_SIZE_LABEL: Record<UiSize, string> = { Small: "Small", Normal: "Normal", Large: "Large" };

/** Each date format with an example, 2026-11-11 14:05 local time (#675). */
function dateFormatLabel(format: DateFormat): string {
  const example = new Date(2026, 10, 11, 14, 5).getTime() / 1000;
  return `${format === "Iso" ? "ISO" : "System"} (${formatAbsoluteDate(example, format)})`;
}

const THEME_LABEL: Record<ThemePreference, string> = { system: "System", light: "Light", dark: "Dark" };

/**
 * "Appearance" (#615): the theme (#691), the code font, its size and the tab size of file
 * content, and the interface text size. Applied at once by `AppearanceSettingsProvider`; the
 * theme by `useTheme`, stored in this webview, not in the config file. Without `theme` (VSCode,
 * where the editor's theme decides, #676) there is no theme field.
 */
export function AppearanceSection({
  theme,
}: {
  theme?: { preference: ThemePreference; onChange: (preference: ThemePreference) => void };
} = {}) {
  const { settings, update, error, dismissError } = useAppearanceSettingsContext();
  return (
    <PreferencesSection title="Appearance">
      {theme !== undefined && (
        <Field label="Theme">
          {(props) => (
            <select
              {...props}
              value={theme.preference}
              onChange={(event) => theme.onChange(event.target.value as ThemePreference)}
            >
              {THEME_PREFERENCES.map((value) => (
                <option key={value} value={value}>
                  {THEME_LABEL[value]}
                </option>
              ))}
            </select>
          )}
        </Field>
      )}
      <CommitOnBlurField
        label="Code font"
        type="text"
        placeholder="Default monospace font"
        value={settings.codeFont ?? ""}
        onCommit={(text) => update({ codeFont: text === "" ? null : text })}
      />
      <CommitOnBlurField
        label="Code font size (px)"
        type="number"
        min={CODE_FONT_SIZES.min}
        max={CODE_FONT_SIZES.max}
        placeholder="Default"
        value={settings.codeFontSize === null ? "" : String(settings.codeFontSize)}
        onCommit={(text) => {
          const size = Number(text);
          if (text === "") update({ codeFontSize: null });
          else if (Number.isFinite(size)) update({ codeFontSize: size });
        }}
      />
      <Field label="Interface size">
        {(props) => (
          <select
            {...props}
            value={settings.uiSize}
            onChange={(event) => update({ uiSize: event.target.value as UiSize })}
          >
            {UI_SIZE_VALUES.map((value) => (
              <option key={value} value={value}>
                {UI_SIZE_LABEL[value]}
              </option>
            ))}
          </select>
        )}
      </Field>
      <Field label="Date format">
        {(props) => (
          <select
            {...props}
            value={settings.dateFormat}
            onChange={(event) => update({ dateFormat: event.target.value as DateFormat })}
          >
            {DATE_FORMAT_VALUES.map((value) => (
              <option key={value} value={value}>
                {dateFormatLabel(value)}
              </option>
            ))}
          </select>
        )}
      </Field>
      <CommitOnBlurField
        label="Tab size"
        type="number"
        min={TAB_SIZES.min}
        max={TAB_SIZES.max}
        value={String(settings.tabSize)}
        onCommit={(text) => {
          const size = Number(text);
          if (text !== "" && Number.isFinite(size)) update({ tabSize: size });
        }}
      />
      <p className={styles.hint}>
        {theme !== undefined && "System follows your operating system's light or dark setting. "}
        The code font and its size apply to diffs, blame, the file viewer and conflicts; a font that
        is not installed falls back to the default. Leave them empty for the defaults. The tab size
        is in columns. System dates use your operating system's date and time format.
      </p>
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

function updateCheckText(check: UpdateCheckOutcome | "checking"): string {
  if (check === "checking") return "Checking for updates…";
  switch (check.kind) {
    case "none":
      return "Browsitory is up to date.";
    case "ready":
      return `Update v${check.version} is ready. Restart from the banner at the top to install it.`;
    case "failed":
      return `Couldn't check for updates: ${check.message}`;
  }
}

/**
 * "Updates" (desktop app only, #630): whether Browsitory checks for a new version on its own,
 * and a button that checks now either way.
 */
export function UpdatesSection({ onCheckNow }: { onCheckNow: () => Promise<UpdateCheckOutcome> }) {
  const { settings, update, error, dismissError } = useUpdateSettingsContext();
  const [check, setCheck] = useState<UpdateCheckOutcome | "checking" | null>(null);
  return (
    <PreferencesSection title="Updates">
      <label className={styles.checkbox}>
        <input
          type="checkbox"
          checked={settings.checkAutomatically}
          onChange={(event) => update({ checkAutomatically: event.target.checked })}
        />
        Check for updates automatically
      </label>
      <p className={styles.hint}>
        At start and every 6 hours, Browsitory asks GitHub for the latest release and downloads a newer one.
        Turned off, it checks only when you click Check for updates.
      </p>
      <div>
        <button
          type="button"
          disabled={check === "checking"}
          onClick={() => {
            setCheck("checking");
            void onCheckNow().then(setCheck);
          }}
        >
          Check for updates
        </button>
      </div>
      {check !== null && (
        <p role={check !== "checking" && check.kind === "failed" ? "alert" : "status"} className={styles.hint}>
          {updateCheckText(check)}
        </p>
      )}
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
