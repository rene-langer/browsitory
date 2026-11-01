// What a failed git hook leaves behind for the UI: the hook's output, and the exact request to
// replay with `noVerify: true` ("Commit without hooks" / "Push without hooks" / "Continue without
// hooks").

/** A push the UI started, recorded so a `pre-push` failure can be retried as-is. */
export type PushRequest =
  | { operation: "PushBranch"; remoteName: string }
  | { operation: "ForcePushBranch"; remoteName: string }
  | { operation: "PushTags"; remoteName: string; names: string[] }
  | { operation: "PushBranchTo"; remoteName: string; localBranch: string; remoteBranch: string; setUpstream: boolean }
  | { operation: "DeleteRemoteBranch"; remoteName: string; branchName: string };

import type { AmendOptions, Trailer } from "../ipc/RepoClient";

export type HookRetry =
  | { kind: "commit"; message: string; trailers: Trailer[] }
  | { kind: "amend"; message: string; options: AmendOptions }
  | { kind: "push"; push: PushRequest }
  /** A paused cherry-pick/revert sequence's Continue (the step it commits runs the hooks). */
  | { kind: "continueSequence" }
  /**
   * A single cherry-pick/revert whose `prepare-commit-msg` failed (#183). Never retried:
   * `--no-verify` can't skip that hook; the commit box finishes the staged pick instead.
   */
  | { kind: "pick" };

export interface HookFailure {
  hook: string;
  output: string;
  retry: HookRetry;
  /** False once the dialog was closed; the error banner's "Show hook output…" reopens it. */
  dialogOpen: boolean;
}

/** What `--no-verify` skips; mirrors `git_core::hooks::SKIPPABLE_HOOKS`. */
export const SKIPPABLE_HOOKS: ReadonlySet<string> = new Set(["pre-commit", "pre-merge-commit", "commit-msg", "pre-push"]);

export function hookFailedMessage(hook: string): string {
  return `The ${hook} hook failed.`;
}

/** The status line while a hook runs; `"signing"` is the signing program (not a hook), reported
 * the same way so the commit box offers the same Cancel button. */
export function runningHookLabel(hook: string): string {
  if (hook === "signing") return "Waiting for the signing program…";
  return `Running ${hook} hook…`;
}

/** The dialog's retry button; `null` for a hook `--no-verify` cannot skip (it would fail again). */
export function retryLabel(failure: HookFailure): string | null {
  if (!SKIPPABLE_HOOKS.has(failure.hook) || failure.retry.kind === "pick") return null;
  if (failure.retry.kind === "commit") return "Commit without hooks";
  if (failure.retry.kind === "amend") return "Amend without hooks";
  if (failure.retry.kind === "continueSequence") return "Continue without hooks";
  return "Push without hooks";
}

/**
 * The toast after a successful retry: the same one the original action announces. A Continue's
 * toast comes from its outcome (`sequenceToast`), so it is not listed here.
 */
export function retrySuccessLabel(retry: Exclude<HookRetry, { kind: "continueSequence" } | { kind: "pick" }>): string {
  if (retry.kind === "commit") return "Committed";
  if (retry.kind === "amend") return "Amended commit";
  if (retry.push.operation === "ForcePushBranch") return "Force pushed";
  if (retry.push.operation === "DeleteRemoteBranch") return "Remote branch deleted";
  return "Push complete";
}
