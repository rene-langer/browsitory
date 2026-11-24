import { readAutoFetchOverride, writeAutoFetchOverride } from "../lib/autoFetch";
import { readCommitMessages, recordCommitMessage } from "../lib/commitMessageHistory";
import { readFetchPrune, writeFetchPrune } from "../lib/fetchPrune";
import { readRecentBranches, recordRecentBranch } from "../lib/recentBranches";
import { readRecentSearches, writeRecentSearches } from "../lib/recentSearches";

// A repository's web storage entries (#1096), written and read through each module's own
// functions, so a renamed storage key fails the tests that use these.
export function rememberRepo(path: string): void {
  writeRecentSearches(path, ["fix"]);
  recordCommitMessage(path, "Fix the build");
  recordRecentBranch(path, "main");
  writeFetchPrune(path, true);
  writeAutoFetchOverride(path, 10);
}

export function remembered(path: string) {
  return {
    searches: readRecentSearches(path),
    messages: readCommitMessages(path),
    branches: readRecentBranches(path),
    prune: readFetchPrune(path),
    autoFetch: readAutoFetchOverride(path),
  };
}

export const NOTHING = { searches: [], messages: [], branches: [], prune: null, autoFetch: null };
export const SOMETHING = { searches: ["fix"], messages: ["Fix the build"], branches: ["main"], prune: true, autoFetch: 10 };
