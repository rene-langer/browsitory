import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import type { OpenRepo } from "../state/useOpenRepos";
import { repoNameOf } from "../lib/repoName";
import { repoTabId } from "./repoTabIds";
import styles from "./TabList.module.css";

interface TabListGroup {
  workspaceName: string | null;
  repos: OpenRepo[];
}

/** Tabs by workspace, in the order each workspace first appears; loose tabs form their own group. */
function groupByWorkspace(repos: OpenRepo[], workspaceNames: Record<string, string>): TabListGroup[] {
  const groups = new Map<string | null, TabListGroup>();
  for (const repo of repos) {
    const workspaceName = repo.workspaceId !== null ? workspaceNames[repo.workspaceId] ?? null : null;
    const group = groups.get(workspaceName) ?? { workspaceName, repos: [] };
    group.repos.push(repo);
    groups.set(workspaceName, group);
  }
  return Array.from(groups.values());
}

/** Whether `query` occurs in the tab's name, folder name or path, ignoring case. */
function tabMatches(repo: OpenRepo, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === "") return true;
  return [repo.displayName, repoNameOf(repo.path), repo.path].some((text) => text.toLowerCase().includes(q));
}

/**
 * The list of every open tab (#625), opened from the tab strip's list button: a filter field over
 * a listbox grouped by workspace. Up/Down move through the matches, Enter or a click switches to
 * one, Escape closes and returns focus to the button, Tab closes and moves on, and so does a press
 * outside (like `ContextMenu`).
 */
export function TabList({
  x,
  y,
  openRepos,
  activePath,
  workspaceNames,
  changedPaths,
  onSwitchTo,
  onClose,
  trigger,
}: {
  /** The right edge and top of the popup, in viewport pixels. */
  x: number;
  y: number;
  openRepos: OpenRepo[];
  activePath: string | null;
  workspaceNames: Record<string, string>;
  changedPaths?: ReadonlySet<string>;
  onSwitchTo: (path: string) => void;
  onClose: () => void;
  /** The list button: focus returns to it, and a press on it is left to its own toggle. */
  trigger: RefObject<HTMLElement | null>;
}) {
  const id = useId();
  const popupRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const matches = openRepos.filter((repo) => tabMatches(repo, query));
  const groups = groupByWorkspace(matches, workspaceNames);
  const ordered = groups.flatMap((group) => group.repos);
  const [highlighted, setHighlighted] = useState<string | null>(activePath);
  const highlightedPath = ordered.some((repo) => repo.path === highlighted) ? highlighted : ordered[0]?.path ?? null;
  const optionId = (path: string) => `${id}-option-${encodeURIComponent(path)}`;

  const [position, setPosition] = useState({ left: x, top: y });
  useLayoutEffect(() => {
    const popup = popupRef.current;
    if (popup === null) return;
    const margin = 4;
    const width = popup.getBoundingClientRect().width;
    const left = Math.max(margin, Math.min(x - width, window.innerWidth - width - margin));
    setPosition((current) => (current.left === left && current.top === y ? current : { left, top: y }));
  }, [x, y]);

  useEffect(() => inputRef.current?.focus(), []);

  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });
  useEffect(() => {
    function handleMouseDown(event: MouseEvent) {
      const target = event.target as Node;
      if (popupRef.current?.contains(target) === true) return;
      if (trigger.current?.contains(target) === true) return;
      onCloseRef.current();
    }
    document.addEventListener("mousedown", handleMouseDown);
    return () => document.removeEventListener("mousedown", handleMouseDown);
  }, [trigger]);

  useEffect(() => {
    if (highlightedPath === null) return;
    document.getElementById(optionId(highlightedPath))?.scrollIntoView?.({ block: "nearest" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [highlightedPath]);

  const close = () => {
    if (trigger.current?.isConnected === true) trigger.current.focus({ preventScroll: true });
    onClose();
  };

  // Focus moves to the chosen tab, as Left/Right on the tablist would leave it.
  const choose = (path: string) => {
    document.getElementById(repoTabId(path))?.focus({ preventScroll: true });
    onClose();
    onSwitchTo(path);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    switch (event.key) {
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        close();
        return;
      case "Tab":
        close();
        return;
      case "Enter":
        event.preventDefault();
        if (highlightedPath !== null) choose(highlightedPath);
        return;
      case "ArrowDown":
      case "ArrowUp": {
        event.preventDefault();
        if (ordered.length === 0) return;
        const current = ordered.findIndex((repo) => repo.path === highlightedPath);
        const step = event.key === "ArrowDown" ? 1 : -1;
        setHighlighted(ordered[(current + step + ordered.length) % ordered.length].path);
        return;
      }
    }
  };

  const listboxId = `${id}-listbox`;
  return (
    <div
      ref={popupRef}
      className={styles.popup}
      style={{ position: "fixed", top: position.top, left: position.left }}
      role="dialog"
      aria-label="Open tabs"
    >
      <input
        className={styles.filter}
        type="text"
        role="combobox"
        aria-label="Filter open tabs"
        aria-expanded="true"
        aria-controls={listboxId}
        aria-autocomplete="list"
        aria-activedescendant={highlightedPath === null ? undefined : optionId(highlightedPath)}
        placeholder="Filter tabs…"
        autoComplete="off"
        spellCheck={false}
        ref={inputRef}
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        onKeyDown={handleKeyDown}
      />
      <div id={listboxId} role="listbox" aria-label="Open tabs" className={styles.list}>
        {ordered.length === 0 && <p className={styles.empty}>No tab matches</p>}
        {groups.map((group) => (
          <div
            key={group.workspaceName ?? ""}
            role="group"
            aria-label={group.workspaceName ?? "Not in a workspace"}
            className={styles.group}
          >
            {group.workspaceName !== null && (
              <div role="presentation" className={styles.groupHeading}>
                {group.workspaceName}
              </div>
            )}
            {group.repos.map((repo) => {
              const changed = changedPaths?.has(repo.path) ?? false;
              const current = repo.path === activePath;
              return (
                <div
                  key={repo.path}
                  id={optionId(repo.path)}
                  role="option"
                  aria-selected={repo.path === highlightedPath}
                  aria-current={current ? "page" : undefined}
                  className={styles.option}
                  title={repo.path}
                  // Keeps focus in the filter field, which owns the keyboard.
                  onMouseDown={(event) => event.preventDefault()}
                  onMouseMove={() => setHighlighted(repo.path)}
                  onClick={() => choose(repo.path)}
                >
                  <span className={styles.name}>
                    {repo.displayName}
                    {changed && (
                      <>
                        <span className={styles.changeMarker} aria-hidden="true" title="Uncommitted changes" />
                        <span className={styles.srOnly}>, uncommitted changes</span>
                      </>
                    )}
                    {current && <span className={styles.current}>current</span>}
                  </span>
                  <span className={styles.path}>{repo.path}</span>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
