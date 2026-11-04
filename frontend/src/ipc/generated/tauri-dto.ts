// Generated from the desktop app's own wire DTOs (crates/tauri-app/src/commands) by ts-rs.
// The shared ones are in wire-dto.ts.
// Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p tauri-app dto_bindings_are_current

export type CloneErrorPayload = { kind: string, message: string, };

export type CloneOutcome = { path: string, submoduleFailures: Array<string>, credentialSaveFailed: boolean, };

export type CloneRequest = { url: string, dest: string, branch: string | null, recurseSubmodules: boolean, https: HttpsInput | null, };

export type HttpsInput = { username: string, token: string, };

export type MergeToolOutcome = "Resolved" | "Unresolved";
