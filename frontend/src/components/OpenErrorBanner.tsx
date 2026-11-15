import type { OpenFailure } from "../lib/openFailure";
import { repoNameOf } from "../lib/repoName";
import { InlineError } from "./primitives/InlineError";
import styles from "./OpenErrorBanner.module.css";

/**
 * The app-level banner for a folder that didn't open, saying why and what to do (#973, #998):
 * a missing recent repository offers to remove its entry; a folder that isn't a repository lists
 * the repositories in its subfolders, each with Open; anything else offers Retry.
 */
export function OpenErrorBanner({
  failure,
  onOpen,
  onRemoveRecent,
  onDismiss,
}: {
  failure: OpenFailure;
  onOpen: (path: string) => void;
  onRemoveRecent: (path: string) => void;
  onDismiss: () => void;
}) {
  switch (failure.kind) {
    case "missing":
      return (
        <InlineError
          message={`The folder ${failure.path} no longer exists. Remove it from recent repositories?`}
          actions={[{ label: "Remove", primary: true, onClick: () => onRemoveRecent(failure.path) }]}
          onDismiss={onDismiss}
        />
      );
    case "notRepository":
      return (
        <InlineError
          message={`This folder isn't a Git repository: ${failure.path}.`}
          hint={
            failure.repos.length === 0
              ? "Open the folder that holds the repository's .git folder."
              : "These folders in it are repositories:"
          }
          copyDetails
          onDismiss={onDismiss}
        >
          {failure.repos.length > 0 && (
            <ul className={styles.repos} aria-label={`Repositories in ${failure.path}`}>
              {failure.repos.map((path) => {
                const name = repoNameOf(path);
                return (
                  <li key={path} className={styles.repo}>
                    <span className={styles.name} title={path}>
                      {name}
                    </span>
                    <button type="button" className={styles.open} aria-label={`Open ${name}`} onClick={() => onOpen(path)}>
                      Open
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </InlineError>
      );
    case "failed":
      return (
        <InlineError
          message={failure.message}
          hint={failure.path !== "" ? "Check that the folder exists and is a Git repository." : undefined}
          onRetry={failure.path !== "" ? () => onOpen(failure.path) : undefined}
          copyDetails
          onDismiss={onDismiss}
        />
      );
  }
}
