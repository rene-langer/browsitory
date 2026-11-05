use super::*;

#[test]
fn remote_failed_event_carries_the_remote_name_and_other_events_omit_it() {
    let (event, failed) = transfer_event_payload(TransferEvent::RemoteFailed {
        operation_id: "fetch-all-7".into(),
        remote_name: "backup".into(),
        error: TransferErrorKind::MissingCredential,
    });
    assert_eq!(event, "transfer-progress");
    let json = serde_json::to_value(failed).unwrap();
    assert_eq!(json["phase"], "RemoteFailed");
    assert_eq!(json["operation"], "FetchAll");
    assert_eq!(json["operationId"], "fetch-all-7");
    assert_eq!(json["remoteName"], "backup");
    assert_eq!(json["errorKind"], "MissingCredential");

    let (_, completed) = transfer_event_payload(TransferEvent::Completed {
        operation_id: "fetch-all-7".into(),
        operation: TransferOperation::FetchAll,
        error: None,
        hook_output: None,
    });
    let json = serde_json::to_value(completed).unwrap();
    assert!(!json.as_object().unwrap().contains_key("remoteName"));
    assert_eq!(json["operation"], "FetchAll");
}

#[test]
fn upstream_info_dto_serializes_the_push_branch_including_null() {
    let upstream = |push_branch: Option<&str>| {
        serde_json::to_value(UpstreamInfoDto::from(git_core::remote::UpstreamInfo {
            local_branch: "fx".into(),
            remote_name: "origin".into(),
            remote_branch: "feature-x".into(),
            ahead: None,
            behind: None,
            push_branch: push_branch.map(str::to_string),
        }))
        .unwrap()
    };
    assert_eq!(upstream(Some("feature-x"))["pushBranch"], "feature-x");
    let json = upstream(None);
    assert!(json.as_object().unwrap().contains_key("pushBranch"));
    assert_eq!(json["pushBranch"], serde_json::Value::Null);
}

#[test]
fn remote_info_dto_serializes_the_prune_default() {
    let json = serde_json::to_value(RemoteInfoDto::from((
        git_core::remote::RemoteInfo {
            name: "origin".into(),
            fetch_url: "https://example.com/repo.git".into(),
            push_url: None,
            prune: true,
        },
        None,
    )))
    .unwrap();
    assert_eq!(json["prune"], true);
    assert_eq!(json["fetchUrl"], "https://example.com/repo.git");
}

#[test]
fn branch_info_dto_serializes_upstream_and_counts_including_nulls() {
    let tracked = serde_json::to_value(BranchInfoDto::from(git_core::branch::BranchInfo {
        name: "main".into(),
        is_current: true,
        upstream: Some("origin/main".into()),
        ahead: Some(2),
        behind: Some(0),
    }))
    .unwrap();
    assert_eq!(tracked["isCurrent"], true);
    assert_eq!(tracked["upstream"], "origin/main");
    assert_eq!(tracked["ahead"], 2);
    assert_eq!(tracked["behind"], 0);

    let untracked = serde_json::to_value(BranchInfoDto::from(git_core::branch::BranchInfo {
        name: "topic".into(),
        is_current: false,
        upstream: None,
        ahead: None,
        behind: None,
    }))
    .unwrap();
    let object = untracked.as_object().unwrap();
    assert_eq!(object["upstream"], serde_json::Value::Null);
    assert!(object.contains_key("ahead") && object.contains_key("behind"));
}

#[test]
fn remote_auth_mode_wire_values_match_the_typescript_union() {
    assert_eq!(
        serde_json::to_value(RemoteAuthModeDto::HttpsToken).unwrap(),
        serde_json::json!("HttpsToken")
    );
    assert_eq!(
        serde_json::to_value(RemoteAuthModeDto::SshAgent).unwrap(),
        serde_json::json!("SshAgent")
    );
}

#[test]
fn forge_provider_wire_values_match_the_typescript_union() {
    assert_eq!(
        serde_json::to_value(ForgeProviderDto::GitHub).unwrap(),
        serde_json::json!("GitHub")
    );
    assert_eq!(
        serde_json::to_value(ForgeProviderDto::Bitbucket).unwrap(),
        serde_json::json!("Bitbucket")
    );
}

#[test]
fn pull_outcome_wire_values_match_the_typescript_union() {
    let fast_forwarded = serde_json::to_value(PullOutcomeDto::FastForwarded {
        upstream_ref: "refs/remotes/origin/main".into(),
    })
    .expect("serialize pull outcome");
    let diverged = serde_json::to_value(PullOutcomeDto::Diverged {
        upstream_ref: "refs/remotes/origin/main".into(),
    })
    .expect("serialize pull outcome");

    assert_eq!(
        serde_json::json!({ "kind": "UpToDate" }),
        serde_json::to_value(PullOutcomeDto::UpToDate).unwrap()
    );
    assert_eq!(
        serde_json::json!({ "kind": "FastForwarded", "upstreamRef": "refs/remotes/origin/main" }),
        fast_forwarded
    );
    assert_eq!(
        serde_json::json!({ "kind": "Diverged", "upstreamRef": "refs/remotes/origin/main" }),
        diverged
    );
}

#[test]
fn transfer_event_bridge_redacts_sideband_and_failure_messages() {
    let (_, progress) = transfer_event_payload(TransferEvent::Progress(TransferProgress {
        operation_id: "fetch-42".into(),
        operation: TransferOperation::Fetch,
        phase: TransferPhase::Receiving,
        current: 2,
        total: 4,
        received_bytes: 1024,
        message: Some("Authorization: Bearer secret-token".into()),
    }));
    let (_, completed) = transfer_event_payload(TransferEvent::Completed {
        operation_id: "fetch-42".into(),
        operation: TransferOperation::Fetch,
        error: Some(TransferErrorKind::TransferFailed),
        hook_output: None,
    });

    assert_eq!(progress.message, None);
    assert_eq!(completed.message, None);
    assert_eq!(
        completed.error_kind,
        Some(TransferErrorKind::TransferFailed)
    );
}

#[test]
fn transfer_failure_is_emitted_as_a_sanitized_failed_terminal_event() {
    let (event_name, failed) = transfer_event_payload(TransferEvent::Completed {
        operation_id: "fetch-42".into(),
        operation: TransferOperation::Fetch,
        error: Some(TransferErrorKind::TransferFailed),
        hook_output: None,
    });

    assert_eq!(event_name, "transfer-complete");
    assert_eq!(failed.phase, TransferProgressPhaseDto::Failed);
    assert_eq!(failed.operation, TransferOperation::Fetch);
    assert_eq!(failed.error_kind, Some(TransferErrorKind::TransferFailed));
    assert_eq!(failed.message, None);
}

#[test]
fn a_cancelled_transfer_is_emitted_with_its_own_terminal_error_kind() {
    // The frontend distinguishes "you cancelled this" from a real transport failure purely
    // by this wire value, so it must not collapse into `TransferFailed`.
    let (event_name, cancelled) = transfer_event_payload(TransferEvent::Completed {
        operation_id: "fetch-42".into(),
        operation: TransferOperation::Fetch,
        error: Some(TransferErrorKind::Cancelled),
        hook_output: None,
    });

    assert_eq!(event_name, "transfer-complete");
    assert_eq!(cancelled.error_kind, Some(TransferErrorKind::Cancelled));
    assert_eq!(cancelled.message, None);
}

#[test]
fn missing_credential_failure_is_emitted_as_a_safe_terminal_kind() {
    let (_, failed) = transfer_event_payload(TransferEvent::Completed {
        operation_id: "fetch-42".into(),
        operation: TransferOperation::Fetch,
        error: Some(TransferErrorKind::MissingCredential),
        hook_output: None,
    });

    assert_eq!(
        failed.error_kind,
        Some(TransferErrorKind::MissingCredential)
    );
    assert_eq!(failed.message, None);
}

#[test]
fn push_failure_payload_preserves_only_safe_operation_and_error_kinds() {
    let (_, failed) = transfer_event_payload(TransferEvent::Completed {
        operation_id: "push-42".into(),
        operation: TransferOperation::PushBranch,
        error: Some(TransferErrorKind::NonFastForward),
        hook_output: None,
    });

    assert_eq!(failed.operation, TransferOperation::PushBranch);
    assert_eq!(failed.error_kind, Some(TransferErrorKind::NonFastForward));
    assert_eq!(failed.message, None);
}
