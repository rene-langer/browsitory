import { createContext } from "react";

/** What a "Check for updates" found (#630). `ready`: an update is downloaded and the banner offers the restart. */
export type UpdateCheckOutcome =
  | { kind: "ready"; version: string }
  | { kind: "none" }
  | { kind: "failed"; message: string };

export type UpdateCheck = () => Promise<UpdateCheckOutcome>;

/**
 * How the update banner (desktop app only) hands the app its check, so "Check for updates" in
 * the palette and in Preferences runs the same check, even with automatic checks off. `register`
 * returns the function that withdraws it.
 */
export interface UpdateCheckRegistry {
  register(check: UpdateCheck): () => void;
}

export const UpdateCheckContext = createContext<UpdateCheckRegistry | null>(null);
