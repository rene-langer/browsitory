import formStyles from "./FormDialog.module.css";

/**
 * Why a form's or dialog's buttons can't run now, in visible text (#664, #748: content guidelines
 * rule 3, `title` alone is not enough). Give it an id from `useId` and name that id in the
 * buttons' `aria-describedby` (`lib/ariaDescribedBy`), so the reason is their description too.
 */
export function BlockedReason({ id, reason }: { id: string; reason: string | null | undefined }) {
  if (reason === null || reason === undefined) return null;
  return (
    <p id={id} className={formStyles.blocked}>
      {reason}
    </p>
  );
}

