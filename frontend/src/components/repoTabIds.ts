/** DOM ids linking a repo's tab to its workspace panel (`aria-controls` / `aria-labelledby`). */
export function repoTabId(path: string): string {
  return `repo-tab-${encodeURIComponent(path)}`;
}

export function repoPanelId(path: string): string {
  return `repo-panel-${encodeURIComponent(path)}`;
}
