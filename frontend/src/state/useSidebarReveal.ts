import { useCallback, useState } from "react";
import type { SidebarPanelId } from "./useSidebarPanelVisibility";

/** A sidebar section: "Branches" (always shown) or one of the panels that can be hidden. */
export type SidebarSectionId = "branches" | SidebarPanelId;

/**
 * A request to show a place in the sidebar (#494), from the command palette. Each request has a
 * new `nonce`, so asking for the same place twice reveals it again. The owning component handles
 * it: `AccordionSection` expands and scrolls to its section, `BranchTree` to a remote's folder.
 */
export interface SidebarRevealRequest {
  section: SidebarSectionId;
  /** A remote whose folder in the Branches section to expand and scroll to. */
  remote?: string;
  nonce: number;
}

export interface UseSidebarRevealResult {
  request: SidebarRevealRequest | null;
  revealSection(section: SidebarSectionId): void;
  revealRemote(remoteName: string): void;
}

/** One repository workspace's sidebar reveal requests (`RepoWorkspace`). */
export function useSidebarReveal(): UseSidebarRevealResult {
  const [request, setRequest] = useState<SidebarRevealRequest | null>(null);
  const revealSection = useCallback(
    (section: SidebarSectionId) => setRequest((previous) => ({ section, nonce: (previous?.nonce ?? 0) + 1 })),
    [],
  );
  const revealRemote = useCallback(
    (remote: string) =>
      setRequest((previous) => ({ section: "branches", remote, nonce: (previous?.nonce ?? 0) + 1 })),
    [],
  );
  return { request, revealSection, revealRemote };
}

/** The request's nonce when it is for `section`, for that section's `revealRequest` prop. */
export function sectionRevealRequest(
  request: SidebarRevealRequest | null,
  section: SidebarSectionId,
): number | undefined {
  return request?.section === section ? request.nonce : undefined;
}
