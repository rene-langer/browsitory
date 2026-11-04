// Generated from the VSCode sidecar's own wire DTOs (crates/vscode-sidecar/src/dispatch.rs) by ts-rs.
// The shared ones are in wire-dto.ts.
// Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar dto_bindings_are_current

import type { OpenRepoEntry } from "./wire-dto";

export type FileVersion = { "kind": "Head" } | { "kind": "Index" } | { "kind": "Stage", stage: number, } | { "kind": "Commit", commitId: string, } | { "kind": "FirstParent", commitId: string, };

export type OpenRepos = { entries: Array<OpenRepoEntry>, activePath: string | null, };
