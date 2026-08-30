//! Transport-agnostic git service layer: per-repo worker threads, credential storage, and
//! forge (GitHub/Bitbucket) pull-request API access. Shared by every `RepoClient` transport —
//! the Tauri desktop app today, a JSON-RPC sidecar for the VSCode extension later. See
//! `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`.

pub mod credentials;
pub mod pull_requests;
pub mod worker;
