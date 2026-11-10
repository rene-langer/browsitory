import { useEffect, useRef, useState } from "react";
import type { RepoClient } from "../ipc/RepoClient";

/** File names a drop treats as patches to apply (#649); anything else is opened as a repository. */
const PATCH_FILE = /\.(patch|diff|mbox)$/i;

export function isPatchPath(path: string): boolean {
  return PATCH_FILE.test(path);
}

/** What one drop holds: folders (or other files) to open, patch files to apply, in drop order. */
export interface FileDrop {
  folders: string[];
  patches: string[];
}

/**
 * Files and folders dragged from the file manager onto the window (#649). `onDrop` gets each drop,
 * split into what to open and what to apply; the result is whether a drag hovers over the window
 * now, for the drop hint.
 */
export function useFileDrops(client: RepoClient, onDrop: (drop: FileDrop) => void): boolean {
  const [dragging, setDragging] = useState(false);
  const onDropRef = useRef(onDrop);
  useEffect(() => {
    onDropRef.current = onDrop;
  });
  useEffect(
    () =>
      client.subscribeFileDrops((event) => {
        if (event.kind === "over") {
          setDragging(true);
          return;
        }
        setDragging(false);
        if (event.kind !== "drop" || event.paths.length === 0) return;
        onDropRef.current({
          folders: event.paths.filter((path) => !isPatchPath(path)),
          patches: event.paths.filter(isPatchPath),
        });
      }),
    [client],
  );
  return dragging;
}
