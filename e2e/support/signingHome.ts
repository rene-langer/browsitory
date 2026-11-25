import fs from "node:fs";
import os from "node:os";
import path from "node:path";

// The allowed signers file for `commit-signing.spec.ts`. The app trusts
// `gpg.ssh.allowedSignersFile` only from the user's own git config, never from the repository's
// (#1121), and its libgit2 reads that config from $HOME. So the session running that spec gets
// its own HOME (`wdio.conf.ts`'s `beforeSession`), holding a `.gitconfig` that names this file;
// the spec writes the key into it. The developer's own git config is never read or changed.

export const SIGNING_HOME = path.join(os.tmpdir(), "browsitory-e2e-signing-home");
export const ALLOWED_SIGNERS = path.join(SIGNING_HOME, "allowed_signers");

/** Whether a session runs the spec that needs the signing HOME. */
export function needsSigningHome(specs: string[]): boolean {
  return specs.some((spec) => spec.endsWith("commit-signing.spec.ts"));
}

/** A fresh HOME whose git config names `ALLOWED_SIGNERS` (still to be written). */
export function prepareSigningHome(): void {
  fs.rmSync(SIGNING_HOME, { recursive: true, force: true });
  fs.mkdirSync(path.join(SIGNING_HOME, ".config"), { recursive: true });
  fs.writeFileSync(path.join(SIGNING_HOME, ".gitconfig"), `[gpg "ssh"]\n\tallowedSignersFile = ${ALLOWED_SIGNERS}\n`);
}
