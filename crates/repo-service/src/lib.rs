//! Transport-agnostic git service layer: per-repo worker threads, credential storage,
//! forge (GitHub/Bitbucket) pull-request API access, and the wire DTOs (`wire`). Shared by every
//! `RepoClient` transport: the Tauri desktop app and the VSCode extension's JSON-RPC sidecar. See
//! `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`.

pub mod base64;
pub mod clone;
pub mod credentials;
pub mod fonts;
pub mod oplog;
pub mod pull_requests;
pub mod user_commands;
pub mod watch;
pub mod wire;
pub mod worker;

// Hide the developer's / CI runner's global and system git config from every test (#111).
#[cfg(test)]
test_support::hermetic_git!();
