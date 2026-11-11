import type { PatchApply, PatchFileStatus, PatchPreview } from "../ipc/RepoClient";
import { formatAbsoluteDate } from "../lib/formatDate";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./ApplyPatchDialog.module.css";

const STATUS_LETTER: Record<PatchFileStatus, string> = {
  Added: "A",
  Deleted: "D",
  Modified: "M",
  Renamed: "R",
  Copied: "C",
};

/** Why "Apply as commits" can't run for `preview`, or `null`. */
function commitsBlockedReason(preview: PatchPreview, hasStagedChanges: boolean): string | null {
  if (preview.parts.some((part) => part.mail === null)) {
    return "Only git format-patch mails carry an author and a message to commit with";
  }
  if (hasStagedChanges) return "Commit or unstage the staged changes first: the commits would take them in";
  return null;
}

/**
 * "Apply patch…"'s preview (#658): the patch file's mails (subject, author, date) and the files
 * each one changes, whether it applies, then Apply (to the working tree), Apply and stage, or
 * Apply as commits (each mail committed with its own author, date and message). Rendered by
 * `WorkspaceDialogs` while `AppState.dialog` is an `applyPatch` one.
 */
export function ApplyPatchDialog({
  preview,
  hasStagedChanges,
  blockedReason,
  onApply,
  onCancel,
}: {
  preview: PatchPreview;
  hasStagedChanges: boolean;
  /** Why repository actions can't run now (`operationBlockers`), or `null`. */
  blockedReason: string | null;
  onApply: (how: PatchApply) => void;
  onCancel: () => void;
}) {
  const mails = preview.parts.filter((part) => part.mail !== null).length;
  const reason = blockedReason ?? (preview.problem === null ? null : "The patch doesn't apply");
  const commitsReason = reason ?? commitsBlockedReason(preview, hasStagedChanges);
  const button = (how: PatchApply, label: string, why: string | null) => (
    <button type="button" disabled={why !== null} title={why ?? undefined} onClick={() => why === null && onApply(how)}>
      {label}
    </button>
  );
  return (
    <FormDialog ariaLabel="Apply patch" onCancel={onCancel}>
      <div className={styles.body}>
        <h2 className={styles.title}>Apply patch</h2>
        <p className={styles.summary}>
          {mails > 0
            ? `${mails} ${mails === 1 ? "commit" : "commits"} as git format-patch ${mails === 1 ? "mail" : "mails"}`
            : "A plain diff (no author or message)"}
        </p>
        <ol className={styles.parts} aria-label="Patch contents">
          {preview.parts.map((part, index) => (
            <li key={index} className={styles.part}>
              {part.mail !== null && (
                <div className={styles.mail}>
                  <span className={styles.subject}>{part.mail.subject || "(no subject)"}</span>
                  <span className={styles.author}>
                    {part.mail.authorName} &lt;{part.mail.authorEmail}&gt;
                    {part.mail.timestamp !== null && ` · ${formatAbsoluteDate(part.mail.timestamp)}`}
                  </span>
                </div>
              )}
              <ul className={styles.files} aria-label={part.mail?.subject ? `Files of ${part.mail.subject}` : "Files"}>
                {part.files.map((file) => (
                  <li key={`${file.oldPath ?? ""}>${file.path}`} className={styles.file}>
                    <span className={styles.status} title={file.status}>
                      {STATUS_LETTER[file.status]}
                    </span>
                    <span className={styles.path}>
                      {file.oldPath !== null ? `${file.oldPath} → ${file.path}` : file.path}
                    </span>
                    <span className={styles.stats}>
                      {file.binary ? (
                        "Binary"
                      ) : (
                        <>
                          <span className={styles.added}>+{file.additions}</span>{" "}
                          <span className={styles.removed}>−{file.deletions}</span>
                        </>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ol>
        {preview.problem !== null ? (
          <p role="alert" className={styles.problem}>
            {preview.problem}
          </p>
        ) : (
          preview.parts.length > 1 && (
            <p className={styles.note}>
              The first mail applies to the working tree; each later one is checked when it is applied. If one
              doesn't apply, nothing is changed.
            </p>
          )
        )}
        {blockedReason !== null && <p className={styles.note}>{blockedReason}</p>}
        <div className={styles.actions}>
          {button("WorkTree", "Apply", reason)}
          {button("Stage", "Apply and stage", reason)}
          {button("Commits", mails > 1 ? `Apply as ${mails} commits` : "Apply as commit", commitsReason)}
          <button type="button" data-autofocus onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </FormDialog>
  );
}
