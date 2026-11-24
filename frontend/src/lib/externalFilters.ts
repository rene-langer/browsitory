// Filter drivers Browsitory can't run (git-crypt, Git LFS; #1063, #1087): the backend refuses to
// stage or write files under them, and the Changes pane says so.

const quoted = (name: string) => `'${name}'`;

/** "'a'", "'a' and 'b'", "'a', 'b' and 'c'". */
function listNames(names: readonly string[]): string {
  const all = names.map(quoted);
  return all.length === 1 ? all[0] : `${all.slice(0, -1).join(", ")} and ${all[all.length - 1]}`;
}

/** The Changes pane's notice for a repository whose attributes use `drivers` (not empty). */
export function externalFiltersNotice(drivers: readonly string[]): string {
  const one = drivers.length === 1;
  return (
    `This repository uses the ${listNames(drivers)} ${one ? "filter" : "filters"}, which Browsitory can't run. ` +
    `It won't stage, discard or check out files under ${one ? "it" : "them"}: use git on the command line for those.`
  );
}

/** Why a file under `driver` has no Stage button, and what its row's note means. */
export function filteredFileReason(driver: string): string {
  return (
    `Browsitory can't run the ${quoted(driver)} filter, so it can't tell whether this file really changed ` +
    "and won't stage it. Use git on the command line."
  );
}
