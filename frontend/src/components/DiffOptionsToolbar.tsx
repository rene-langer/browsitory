import { Columns2, Minus, Plus, Rows2 } from "lucide-react";
import { MAX_CONTEXT_LINES } from "../ipc/RepoClient";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { useDiffViewOptions } from "../state/useDiffViewOptions";
import type { ContextMenuItem } from "./primitives/ContextMenu";
import { ViewMenuButton } from "./ViewMenuButton";
import styles from "./DiffOptionsToolbar.module.css";

/**
 * The one row of diff view options in a pane (#640): it applies to every diff in the pane and
 * persists (Split view and "Show whole file" through `useDiffViewOptions`, the rest through the
 * diff settings). The row keeps the layout and the context lines; the rarely changed toggles are
 * in "View ▾" with text labels (#1007), named like the palette's commands that turn them on.
 * `wholeFile={false}` leaves out "Show whole file" where no diff below supports it.
 */
export function DiffOptionsToolbar({ wholeFile = true, sticky = false }: { wholeFile?: boolean; sticky?: boolean }) {
  const { settings, update } = useDiffSettingsContext();
  const options = useDiffViewOptions();
  const wholeFileOn = wholeFile && options.wholeFile;
  // `Auto` (the default) decides per diff (`lib/wordWrap`), so the item shows it as mixed, with an
  // "A" badge (#1007); choosing it sets `On`, the next time `Off`. Preferences still offers `Auto`.
  const wrapChecked = settings.wordWrap === "Auto" ? "mixed" : settings.wordWrap === "On";
  const wrapTitle =
    settings.wordWrap === "Auto"
      ? "Automatic: split view, prose and very long lines wrap. Choose to always wrap."
      : settings.wordWrap === "On"
        ? "On. Choose to scroll long lines sideways."
        : "Off. Choose to wrap long lines.";
  const viewItems: ContextMenuItem[] = [
    {
      label: "Wrap lines",
      checked: wrapChecked,
      title: wrapTitle,
      onSelect: () => update({ wordWrap: settings.wordWrap === "On" ? "Off" : "On" }),
    },
    ...(wholeFile
      ? [
          {
            label: "Show whole file",
            checked: options.wholeFile,
            title: "Every line of the file, not just the changes with context",
            onSelect: options.toggleWholeFile,
          },
        ]
      : []),
    {
      label: "Ignore whitespace",
      checked: settings.ignoreWhitespace,
      title: "Hide changes that only add or remove spaces",
      onSelect: () => update({ ignoreWhitespace: !settings.ignoreWhitespace }),
    },
    {
      label: "Show whitespace",
      checked: settings.showWhitespace,
      title: "Mark spaces, tabs and carriage returns",
      onSelect: () => update({ showWhitespace: !settings.showWhitespace }),
    },
    {
      label: "Show syntax highlighting",
      checked: settings.syntaxHighlight,
      title: "Colour the code by its language",
      onSelect: () => update({ syntaxHighlight: !settings.syntaxHighlight }),
    },
  ];
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
      <ViewMenuButton
        label="Diff view"
        title="Wrapping, whitespace, the whole file and syntax highlighting"
        items={viewItems}
        className={styles.viewButton}
      />
    </div>
  );
}
