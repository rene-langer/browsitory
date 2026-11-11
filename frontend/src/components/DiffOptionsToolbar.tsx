import { Code, Columns2, Minus, Pilcrow, Plus, Rows2, Space, UnfoldVertical, WrapText, type LucideIcon } from "lucide-react";
import { MAX_CONTEXT_LINES } from "../ipc/RepoClient";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { useDiffViewOptions } from "../state/useDiffViewOptions";
import styles from "./DiffOptionsToolbar.module.css";

function IconToggle({
  label,
  title,
  icon: Icon,
  pressed,
  onClick,
}: {
  label: string;
  title: string;
  icon: LucideIcon;
  pressed: boolean | "mixed";
  onClick: () => void;
}) {
  return (
    <button type="button" aria-label={label} title={title} aria-pressed={pressed} onClick={onClick}>
      <Icon size={14} aria-hidden="true" />
    </button>
  );
}

/**
 * The one row of diff view options in a pane (#640): it applies to every diff in the pane and
 * persists (Split view and "Show whole file" through `useDiffViewOptions`, the rest through the
 * diff settings). Icon buttons keep the row on one line; each has a tooltip and the accessible
 * name the old text buttons had. `wholeFile={false}` leaves out "Show whole file" where no diff
 * below supports it.
 */
export function DiffOptionsToolbar({ wholeFile = true, sticky = false }: { wholeFile?: boolean; sticky?: boolean }) {
  const { settings, update } = useDiffSettingsContext();
  const options = useDiffViewOptions();
  const wholeFileOn = wholeFile && options.wholeFile;
  // `Auto` (the default) decides per diff (`lib/wordWrap`), so the button shows it as mixed; a
  // click sets `On`, the next `Off`. Preferences still offers `Auto`.
  const wrapPressed = settings.wordWrap === "Auto" ? "mixed" : settings.wordWrap === "On";
  const wrapTitle =
    settings.wordWrap === "Auto"
      ? "Wrap lines: automatic (split view, prose and very long lines wrap). Click to always wrap."
      : settings.wordWrap === "On"
        ? "Wrap lines: on. Click to scroll long lines sideways."
        : "Wrap lines: off. Click to wrap long lines.";
  return (
    <div
      className={sticky ? `${styles.toolbar} ${styles.sticky}` : styles.toolbar}
      role="group"
      aria-label="Diff view options"
    >
      <span className={styles.segmented} role="group" aria-label="Diff layout">
        <button
          type="button"
          aria-label="Unified view"
          title="Unified view: old and new lines in one column"
          aria-pressed={!options.split}
          onClick={() => options.setSplit(false)}
        >
          <Rows2 size={14} aria-hidden="true" />
          <span className={styles.segmentLabel}>Unified</span>
        </button>
        <button
          type="button"
          aria-label="Split view"
          title="Split view: old lines on the left, new lines on the right"
          aria-pressed={options.split}
          onClick={() => options.setSplit(true)}
        >
          <Columns2 size={14} aria-hidden="true" />
          <span className={styles.segmentLabel}>Split</span>
        </button>
      </span>
      <IconToggle
        label="Wrap lines"
        title={wrapTitle}
        icon={WrapText}
        pressed={wrapPressed}
        onClick={() => update({ wordWrap: settings.wordWrap === "On" ? "Off" : "On" })}
      />
      {wholeFile && (
        <IconToggle
          label="Show whole file"
          title="Show whole file: every line of the file, not just the changes with context"
          icon={UnfoldVertical}
          pressed={options.wholeFile}
          onClick={options.toggleWholeFile}
        />
      )}
      <IconToggle
        label="Ignore whitespace"
        title="Ignore whitespace: hide changes that only add or remove spaces"
        icon={Space}
        pressed={settings.ignoreWhitespace}
        onClick={() => update({ ignoreWhitespace: !settings.ignoreWhitespace })}
      />
      <IconToggle
        label="Show whitespace"
        title="Show whitespace: mark spaces, tabs and carriage returns"
        icon={Pilcrow}
        pressed={settings.showWhitespace}
        onClick={() => update({ showWhitespace: !settings.showWhitespace })}
      />
      <IconToggle
        label="Syntax highlighting"
        title="Syntax highlighting"
        icon={Code}
        pressed={settings.syntaxHighlight}
        onClick={() => update({ syntaxHighlight: !settings.syntaxHighlight })}
      />
      <span className={styles.contextStepper} title="Context lines around each change">
        <button
          type="button"
          aria-label="Fewer context lines"
          disabled={wholeFileOn || settings.contextLines <= 0}
          onClick={() => update({ contextLines: settings.contextLines - 1 })}
        >
          <Minus size={12} aria-hidden="true" />
        </button>
        <span aria-label="Context lines">{settings.contextLines}</span>
        <button
          type="button"
          aria-label="More context lines"
          disabled={wholeFileOn || settings.contextLines >= MAX_CONTEXT_LINES}
          onClick={() => update({ contextLines: settings.contextLines + 1 })}
        >
          <Plus size={12} aria-hidden="true" />
        </button>
      </span>
    </div>
  );
}
