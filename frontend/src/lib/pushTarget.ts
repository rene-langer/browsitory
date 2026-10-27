import type { UpstreamInfo } from "../ipc/RepoClient";

/**
 * The remote branch a push of `upstream.localBranch` to `upstream.remoteName` lands on, as the
 * backend's `push.default` rule resolved it (`UpstreamInfo.pushBranch`), or `null` when the user
 * must choose (the Push to… dialog). An upstream without `pushBranch` is read as
 * `push.default=simple`: only a same-named upstream is pushed to.
 */
export function upstreamPushBranch(upstream: UpstreamInfo): string | null {
  if (upstream.pushBranch !== undefined) return upstream.pushBranch;
  return upstream.remoteBranch === upstream.localBranch ? upstream.remoteBranch : null;
}
