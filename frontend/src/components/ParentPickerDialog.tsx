import { useState, useId } from "react";
import type { GraphCommit } from "../ipc/RepoClient";
import { FormDialog } from "./primitives/FormDialog";
import { Toolbar } from "./primitives/Toolbar";
import { RefBadges } from "./RefBadges";
import formStyles from "./primitives/FormDialog.module.css";
import styles from "./ParentPickerDialog.module.css";

/**
 * Asks which parent a merge commit is cherry-picked or reverted against (`-m <mainline>`), with
 * parent 1 preselected. Parents the graph has loaded show their summary and branch badges.
 * `blockedReason` (`operationBlockers`) disables the action while it applies: the repository can
 * become busy after the dialog opened (#514).
 */
export function ParentPickerDialog({
  action,
  commit,
  commits,
  onConfirm,
  onCancel,
  returnFocusTo,
  blockedReason = null,
}: {
  action: "Cherry-pick" | "Revert";
  commit: GraphCommit;
  /** The loaded graph, to describe each parent. */
  commits: GraphCommit[];
  /** The chosen 1-based parent number. */
  onConfirm: (mainline: number) => void;
  onCancel: () => void;
  /** `FormDialog`'s: where focus goes on close. */
  returnFocusTo?: HTMLElement | null;
  /** Why repository operations are blocked now, or `null`. */
  blockedReason?: string | null;
}) {
  // The visible reason the submit button is disabled is its description (#664).
  const reasonId = useId();
  const [mainline, setMainline] = useState(1);
  return (
    <FormDialog ariaLabel="Choose parent" onCancel={onCancel} returnFocusTo={returnFocusTo}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          // Enter on a radio submits even with the button disabled.
          if (blockedReason === null) onConfirm(mainline);
        }}
      >
        <p>
          {action} merge commit <code>{commit.shortId}</code> {commit.summary}
        </p>
        <fieldset className={styles.parents} role="radiogroup" aria-label="Mainline parent">
          {commit.parentIds.map((parentId, index) => {
            const parent = commits.find((candidate) => candidate.id === parentId);
            const number = index + 1;
            return (
              <label key={parentId} className={styles.parent}>
                <input
                  type="radio"
                  name="mainline-parent"
                  value={number}
                  checked={mainline === number}
                  data-autofocus={mainline === number ? "" : undefined}
                  onChange={() => setMainline(number)}
                />
                <span>
                  <strong>Parent {number}</strong> <code>{parent?.shortId ?? parentId.slice(0, 7)}</code>
                  {parent !== undefined && <> {parent.summary}</>}
                </span>
                {parent !== undefined && <RefBadges commit={parent} />}
              </label>
            );
          })}
        </fieldset>
        <p className={styles.hint}>Parent 1 is usually the branch that was merged into.</p>
        {blockedReason !== null && <p id={reasonId} className={formStyles.blocked}>{blockedReason}</p>}
        <Toolbar>
          <button type="submit" disabled={blockedReason !== null} title={blockedReason ?? undefined} aria-describedby={blockedReason !== null ? reasonId : undefined}>
            {action}
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </Toolbar>
      </form>
    </FormDialog>
  );
}
