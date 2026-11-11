//! #306: byte-exact snapshots of the desktop app's wire output, checked in as
//! `crates/tauri-app/tests/wire/dto-snapshots.txt`. Each line is one DTO built from the shared
//! domain values in `crates/test-support/wire_fixtures.rs` through the `From` impls and helpers
//! the commands use (`repo_service::wire`, plus this crate's desktop-only DTOs and event names),
//! serialized the way Tauri sends it. The sidecar's snapshot test compares
//! the cases both transports share (see `crates/vscode-sidecar/src/dispatch/wire_snapshots.rs`).
//! Regenerate with `UPDATE_TRANSPORT_METHODS=1 cargo test -p tauri-app wire_snapshots`.
//!
//! The cases both transports share are listed once, in `crates/test-support/wire_cases.rs`
//! (#488); this file serializes them and adds its own.

use serde::Serialize;

use super::clone::{CloneErrorDto, CloneOutcomeDto};
use super::*;

#[path = "../../../test-support/wire_fixtures.rs"]
#[macro_use]
mod fixtures;

use fixtures::Cases;

#[path = "../../../test-support/wire_cases.rs"]
mod cases;

use cases::{shared_wire_cases, Slot, WireCases};

/// What Tauri sends for a command's answer (`IpcResponse::body`) and an event's payload
/// (`EmitArgs::new`): `serde_json::to_string` of the value, fields in declaration order.
fn wire<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

/// What Tauri sends for a command's error: `InvokeError::from` makes a `serde_json::Value`
/// first, whose objects sort their keys.
fn wire_error<T: Serialize>(value: &T) -> String {
    serde_json::to_string(&serde_json::to_value(value).unwrap()).unwrap()
}

#[derive(Default)]
struct Snapshots(Vec<(String, String)>);

impl WireCases for Snapshots {
    fn add<T, D: Serialize>(&mut self, dto: &str, cases: Cases<T>, to_dto: impl Fn(T) -> D) {
        for (case, value) in cases {
            self.0.push((format!("{dto}/{case}"), wire(&to_dto(value))));
        }
    }

    fn transport_cases(&mut self, slot: Slot) {
        match slot {
            Slot::AfterRepoWatch => {
                // The Tauri event they are all emitted as.
                self.0.push((
                    "RepoWatchEvent/all".to_string(),
                    wire(&super::watch::REPO_WATCH_EVENT),
                ));
                // The Tauri event they are all emitted as.
                self.0.push((
                    "HookProgressEvent/all".to_string(),
                    wire(&HOOK_PROGRESS_EVENT),
                ));
            }
            Slot::AfterTransferProgress => {
                // What the desktop app emits is the shared `TransferProgress` DTO.
                for (case, event) in fixtures::transfer_events() {
                    assert_eq!(
                        wire(&transfer_event_payload(event.clone()).1),
                        wire(&repo_service::wire::transfer_progress_dto(event)),
                        "TransferProgress/{case}"
                    );
                }
                // The Tauri event each one is emitted as.
                self.add(
                    "TransferProgressEvent",
                    fixtures::transfer_events(),
                    |event| transfer_event_payload(event).0,
                );
            }
        }
    }
}

fn snapshots() -> Vec<(String, String)> {
    let mut s = Snapshots::default();
    shared_wire_cases(&mut s);

    // Desktop only.
    s.add(
        "MergeToolOutcome",
        merge_tool_outcomes(),
        MergeToolOutcomeDto::from,
    );
    s.add("ListOpenRepos", fixtures::open_repo_entries(), |entry| {
        // `list_open_repos` answers a tuple, which Tauri sends as a two-element array.
        let active = Some(entry.path.to_string_lossy().into_owned());
        (vec![OpenRepoEntryDto::from(entry)], active)
    });
    s.0.push((
        "ListOpenRepos/none".to_string(),
        wire(&(Vec::<OpenRepoEntryDto>::new(), None::<String>)),
    ));
    s.add("CloneOutcome", clone_outcomes(), CloneOutcomeDto::from);
    for (case, error) in clone_errors() {
        s.0.push((format!("CloneErrorPayload/{case}"), wire_error(&error)));
    }
    // Every other command's error (#496).
    for (case, error) in fixtures::errors() {
        s.0.push((format!("RepoErrorPayload/{case}"), wire_error(&error)));
    }
    s.0
}

fn merge_tool_outcomes() -> Cases<git_core::tools::MergeToolOutcome> {
    use git_core::tools::MergeToolOutcome;
    let cases: Cases<_> = [MergeToolOutcome::Resolved, MergeToolOutcome::Unresolved]
        .into_iter()
        .map(|outcome| (format!("{outcome:?}"), outcome))
        .collect();
    assert_every_variant!(
        cases.iter().map(|(_, outcome)| outcome),
        MergeToolOutcome::Resolved,
        MergeToolOutcome::Unresolved,
    );
    cases
}

fn clone_outcomes() -> Cases<repo_service::clone::ServiceCloneOutcome> {
    let outcome = |submodule_failures: Vec<String>, credential_save_failed| {
        repo_service::clone::ServiceCloneOutcome {
            outcome: git_core::clone::CloneOutcome {
                path: "/repos/cloned".into(),
                submodule_failures,
            },
            credential_save_failed,
        }
    };
    vec![
        ("clean".to_string(), outcome(Vec::new(), false)),
        (
            "partial".to_string(),
            outcome(vec!["vendor/lib".to_string()], true),
        ),
    ]
}

/// Every `CloneError` and `RepoError` mapping, plus the `Io` error `clone_repo` builds itself
/// when its blocking task fails.
fn clone_errors() -> Vec<(String, CloneErrorDto)> {
    use git_core::clone::CloneError;
    use git_core::repo::RepoError;
    let mut clone_errors: Vec<(String, CloneError)> = vec![
        ("InvalidUrl".into(), CloneError::InvalidUrl),
        (
            "CredentialBearingUrl".into(),
            CloneError::CredentialBearingUrl,
        ),
        ("DestNotEmpty".into(), CloneError::DestNotEmpty),
        ("BranchNotFound".into(), CloneError::BranchNotFound),
        ("Cancelled".into(), CloneError::Cancelled),
        (
            "Io".into(),
            CloneError::Io(std::io::Error::other("disk full")),
        ),
        (
            "CaseCollision".into(),
            CloneError::CaseCollision(git_core::CaseCollision {
                link: "Docs".to_string(),
                folder: "docs".to_string(),
            }),
        ),
        (
            "CasePair".into(),
            CloneError::CasePair(git_core::CasePair {
                path: "A".to_string(),
                other: "a".to_string(),
            }),
        ),
        (
            "CaseConfig".into(),
            CloneError::CaseConfig(git_core::CaseConfigUnreadable(
                "failed to parse 'maybe' as a boolean".to_string(),
            )),
        ),
        (
            "FileInUse".into(),
            CloneError::HeldOpen(git_core::HeldOpen {
                path: "f.txt".to_string(),
                during: git_core::HeldDuring::Clone,
            }),
        ),
    ];
    clone_errors.extend(fixtures::transfer_error_kinds().into_iter().map(|kind| {
        (
            format!("Transfer-{kind:?}"),
            CloneError::Transfer {
                kind,
                message: "The transfer failed.".to_string(),
            },
        )
    }));
    assert_every_variant!(
        clone_errors.iter().map(|(_, error)| error),
        CloneError::InvalidUrl,
        CloneError::CredentialBearingUrl,
        CloneError::DestNotEmpty,
        CloneError::BranchNotFound,
        CloneError::Cancelled,
        CloneError::Transfer { .. },
        CloneError::Io(_),
        CloneError::CaseCollision(_),
        CloneError::CasePair(_),
        CloneError::CaseConfig(_),
        CloneError::HeldOpen(_),
    );
    let init_errors: Vec<(String, RepoError)> = vec![
        (
            "Open".into(),
            RepoError::Open(git2::Error::from_str("could not open")),
        ),
        ("AlreadyRepository".into(), RepoError::AlreadyRepository),
        (
            "Init".into(),
            RepoError::Init(git2::Error::from_str("could not create")),
        ),
        (
            "Io".into(),
            RepoError::Io(std::io::Error::other("read-only")),
        ),
    ];
    assert_every_variant!(
        init_errors.iter().map(|(_, error)| error),
        RepoError::Open(_),
        RepoError::AlreadyRepository,
        RepoError::Init(_),
        RepoError::Io(_),
    );
    let mut out: Vec<(String, CloneErrorDto)> = clone_errors
        .into_iter()
        .map(|(case, error)| (format!("clone-{case}"), CloneErrorDto::from(error)))
        .collect();
    out.extend(
        init_errors
            .into_iter()
            .map(|(case, error)| (format!("init-{case}"), CloneErrorDto::from(error))),
    );
    out.push((
        "task-failed".to_string(),
        CloneErrorDto {
            kind: "Io".into(),
            message: "task panicked".to_string(),
        },
    ));
    out
}

#[test]
fn wire_snapshots_are_current() {
    let file = test_support::wire_snapshot_file(
        "# Generated: the desktop app's wire bytes, one `<dto>/<case> <json>` line per case (#306).\n\
         # Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p tauri-app wire_snapshots\n",
        &snapshots(),
    );
    test_support::assert_generated_file(
        &test_support::repo_root().join("crates/tauri-app/tests/wire/dto-snapshots.txt"),
        &file,
    );
}
