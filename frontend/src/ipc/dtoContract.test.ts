// `dtoContract.ts` only checks the types it lists. This test makes sure it lists every type
// ts-rs generates (#306), so a new Rust DTO can't skip the contract.
import { describe, expect, it } from "vitest";
import contract from "./dtoContract.ts?raw";
import wireBindings from "./generated/wire-dto.ts?raw";
import tauriBindings from "./generated/tauri-dto.ts?raw";
import sidecarBindings from "./generated/sidecar-dto.ts?raw";

/** Generated types with no hand-written counterpart in `RepoClient.ts`, and why. */
const NOT_COMPARED: Record<string, string> = {
  // Inline in `CloneRequest`, `CommitSignature`, `TerminalRebase` and `TransferProgress`, which
  // are compared as a whole.
  HttpsInput: "inline in CloneRequest",
  SignatureFormat: "inline in CommitSignature",
  TerminalRebaseKind: "inline in TerminalRebase",
  TransferPhase: "part of TransferProgress.phase",
  // `toCloneError` reads the error payload defensively; its `kind` is a plain string in Rust.
  CloneErrorPayload: "read by toCloneError, not typed",
  // Parameter of the host-only `read_file_version`, which the webview never calls.
  FileVersion: "extension host only",
};

function exportedTypes(source: string): string[] {
  return [...source.matchAll(/^export type (\w+)/gm)].map((match) => match[1]!);
}

describe("dtoContract.ts", () => {
  it.each([
    ["Wire", wireBindings],
    ["Tauri", tauriBindings],
    ["Sidecar", sidecarBindings],
  ])("compares every type in the %s bindings", (side, bindings) => {
    const unchecked = exportedTypes(bindings).filter(
      (name) => !(name in NOT_COMPARED) && !new RegExp(`\\b${side}\\.${name}\\b`).test(contract),
    );
    expect(unchecked).toEqual([]);
  });

  it("finds each generated type in one file only", () => {
    // A transport-specific type with a shared type's name would hide which one is compared.
    const all = [wireBindings, tauriBindings, sidecarBindings].flatMap(exportedTypes);
    expect(all.filter((name, index) => all.indexOf(name) !== index)).toEqual([]);
  });
});
