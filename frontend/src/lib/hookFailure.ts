// What a failed git hook leaves behind for the UI: the hook's output, and the exact request to
// replay with `noVerify: true` ("Commit without hooks" / "Push without hooks").

/** A push the UI started, recorded so a `pre-push` failure can be retried as-is. */
export type PushRequest =
  | { operation: "PushBranch"; remoteName: string }
  | { operation: "ForcePushBranch"; remoteName: string }
  | { operation: "PushTags"; remoteName: string; names: string[] }
  | { operation: "PushBranchTo"; remoteName: string; localBranch: string; remoteBranch: string; setUpstream: boolean }
  | { operation: "DeleteRemoteBranch"; remoteName: string; branchName: string };

export type HookRetry =
  | { kind: "commit"; message: string }
  | { kind: "amend"; message: string }
  | { kind: "push"; push: PushRequest };

export interface HookFailure {
  hook: string;
  output: string;
  retry: HookRetry;
  /** False once the dialog was closed; the error banner's "Show hook output…" reopens it. */
  dialogOpen: boolean;
}

/** What `--no-verify` skips; mirrors `git_core::hooks::SKIPPABLE_HOOKS`. */
export const SKIPPABLE_HOOKS: ReadonlySet<string> = new Set(["pre-commit", "commit-msg", "pre-push"]);

export function hookFailedMessage(hook: string): string {
  return `The ${hook} hook failed.`;
}

export function runningHookLabel(hook: string): string {
  return `Running ${hook} hook…`;
}

/** The dialog's retry button; `null` for a hook `--no-verify` cannot skip (it would fail again). */
export function retryLabel(failure: HookFailure): string | null {
  if (!SKIPPABLE_HOOKS.has(failure.hook)) return null;
  if (failure.retry.kind === "commit") return "Commit without hooks";
  if (failure.retry.kind === "amend") return "Amend without hooks";
  return "Push without hooks";
}

/** The toast after a successful retry: the same one the original action announces. */
export function retrySuccessLabel(retry: HookRetry): string {
  if (retry.kind === "commit") return "Committed";
  if (retry.kind === "amend") return "Amended commit";
  if (retry.push.operation === "ForcePushBranch") return "Force pushed";
  if (retry.push.operation === "DeleteRemoteBranch") return "Remote branch deleted";
  return "Push complete";
}
