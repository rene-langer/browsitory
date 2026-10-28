import { useState } from "react";
import type { GraphCommit } from "../ipc/RepoClient";
import { FormDialog } from "./primitives/FormDialog";
import { Toolbar } from "./primitives/Toolbar";
import { RefBadges } from "./RefBadges";
import styles from "./ParentPickerDialog.module.css";

/**
 * Asks which parent a merge commit is cherry-picked or reverted against (`-m <mainline>`), with
 * parent 1 preselected. Parents the graph has loaded show their summary and branch badges.
 */
export function ParentPickerDialog({
  action,
  commit,
  commits,
  onConfirm,
  onCancel,
}: {
  action: "Cherry-pick" | "Revert";
  commit: GraphCommit;
  /** The loaded graph, to describe each parent. */
  commits: GraphCommit[];
  /** The chosen 1-based parent number. */
  onConfirm: (mainline: number) => void;
  onCancel: () => void;
}) {
  const [mainline, setMainline] = useState(1);
  return (
    <FormDialog ariaLabel="Choose parent" onCancel={onCancel}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onConfirm(mainline);
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
        <Toolbar>
          <button type="submit">{action}</button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </Toolbar>
      </form>
    </FormDialog>
  );
}
