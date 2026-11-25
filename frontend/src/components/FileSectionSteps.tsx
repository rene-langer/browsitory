import { useState } from "react";
import { FILE_SECTION_STEP } from "../lib/largeDiff";
import styles from "./DiffPane.module.css";

const fileCount = new Intl.NumberFormat("en-US");

/**
 * How many of a list's file sections render (#1153): `FILE_SECTION_STEP` at first, more on
 * request, and a step again for another `listKey` (another range or parent). `more` is the row
 * under the list that asks for them, or `null` when all show.
 */
export function useFileSectionSteps(total: number, listKey = "") {
  const [state, setState] = useState({ listKey, limit: FILE_SECTION_STEP });
  // Reset during render, as `useDiffSizeLimit` does for another file.
  const current = state.listKey === listKey ? state : { listKey, limit: FILE_SECTION_STEP };
  if (current !== state) setState(current);
  const setLimit = (limit: number) => setState({ listKey, limit });

  const shown = Math.min(total, current.limit);
  const notShown = total - shown;
  const more =
    notShown > 0 ? (
      <div className={styles.moreFiles}>
        <span>
          {fileCount.format(notShown)} more {notShown === 1 ? "file" : "files"} not shown.
        </span>
        <button type="button" onClick={() => setLimit(current.limit + FILE_SECTION_STEP)}>
          Show {fileCount.format(Math.min(FILE_SECTION_STEP, notShown))} more
        </button>
        {notShown > FILE_SECTION_STEP && (
          <button type="button" onClick={() => setLimit(Number.POSITIVE_INFINITY)}>
            Show all
          </button>
        )}
      </div>
    ) : null;
  return { shown, more };
}
