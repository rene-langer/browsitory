import { useCallback, useSyncExternalStore } from "react";
import type { ForgeProvider } from "../ipc/RepoClient";

/**
 * The account a forge token was saved for, per provider (#1012): a section with one is
 * "connected" and lists its pull requests by itself. The token stays in the system's credential
 * store (`save_forge_token` keys it by provider and account); only the account name is kept
 * here, in the web storage, like other display choices. Every section of one provider shares it.
 */
const storageKey = (provider: ForgeProvider) => `forge-account.${provider}`;

const listeners = new Set<() => void>();

function read(provider: ForgeProvider): string | null {
  try {
    const account = localStorage.getItem(storageKey(provider));
    return account === null || account === "" ? null : account;
  } catch {
    return null;
  }
}

function write(provider: ForgeProvider, account: string | null): void {
  try {
    if (account === null) localStorage.removeItem(storageKey(provider));
    else localStorage.setItem(storageKey(provider), account);
  } catch {
    // Storage refused: the token is saved, but the section asks to connect again.
  }
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The connected account of `provider` (`null`: not connected), and a setter that every user sees. */
export function useForgeAccount(provider: ForgeProvider): [string | null, (account: string | null) => void] {
  const account = useSyncExternalStore(subscribe, () => read(provider));
  const setAccount = useCallback((next: string | null) => write(provider, next), [provider]);
  return [account, setAccount];
}
