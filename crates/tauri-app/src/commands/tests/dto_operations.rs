use super::*;

#[test]
fn commit_outcome_dto_is_kind_tagged() {
    assert_eq!(
        serde_json::to_value(CommitOutcomeDto::from(
            git_core::commit::CommitOutcome::Committed {
                commit_id: "c1".into()
            }
        ))
        .unwrap(),
        serde_json::json!({"kind": "Committed", "commitId": "c1"})
    );
    assert_eq!(
        serde_json::to_value(CommitOutcomeDto::from(
            git_core::commit::CommitOutcome::HookFailed {
                hook: "pre-commit".into(),
                output: "no\n".into()
            }
        ))
        .unwrap(),
        serde_json::json!({"kind": "HookFailed", "hook": "pre-commit", "output": "no\n"})
    );
}

#[test]
fn hook_progress_dto_carries_the_repo_and_phase() {
    let dto = hook_progress_dto(
        "/repo",
        HookEvent::Started {
            operation_id: "commit-1".into(),
            hook: "pre-commit".into(),
            source: HookSource::Commit,
        },
    );
    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        serde_json::json!({"repoPath": "/repo", "operationId": "commit-1", "hook": "pre-commit", "phase": "Started", "source": "Commit"})
    );
    let dto = hook_progress_dto(
        "/repo",
        HookEvent::Finished {
            operation_id: "commit-1".into(),
            hook: "pre-commit".into(),
            source: HookSource::Commit,
        },
    );
    assert_eq!(serde_json::to_value(dto).unwrap()["phase"], "Finished");
}

#[test]
fn hook_progress_dto_serializes_its_source() {
    for (source, name) in [
        (HookSource::Commit, "Commit"),
        (HookSource::Checkout, "Checkout"),
        (HookSource::Merge, "Merge"),
        (HookSource::Pick, "Pick"),
    ] {
        let dto = hook_progress_dto(
            "/repo",
            HookEvent::Started {
                operation_id: "switch-1".into(),
                hook: "post-checkout".into(),
                source,
            },
        );
        assert_eq!(serde_json::to_value(dto).unwrap()["source"], name);
    }
}

#[test]
fn pick_outcome_dto_serializes_a_hook_failure() {
    assert_eq!(
        serde_json::to_value(PickOutcomeDto::from(
            git_core::pick::PickOutcome::HookFailed {
                hook: "prepare-commit-msg".into(),
                output: "no ticket\n".into()
            }
        ))
        .unwrap(),
        serde_json::json!({"kind": "HookFailed", "hook": "prepare-commit-msg", "output": "no ticket\n"})
    );
}

#[test]
fn a_hook_failed_transfer_carries_its_output_and_others_do_not() {
    let (_, failed) = transfer_event_payload(TransferEvent::Completed {
        operation_id: "push-1".into(),
        operation: TransferOperation::PushBranch,
        error: Some(TransferErrorKind::HookFailed),
        hook_output: Some("tests failed\n".into()),
    });
    let value = serde_json::to_value(failed).unwrap();
    assert_eq!(value["errorKind"], "HookFailed");
    assert_eq!(value["hookOutput"], "tests failed\n");
    assert_eq!(value["message"], serde_json::Value::Null);
    let (_, started) = transfer_event_payload(TransferEvent::Started {
        operation_id: "push-1".into(),
        operation: TransferOperation::PushBranch,
    });
    assert_eq!(
        serde_json::to_value(started).unwrap()["hookOutput"],
        serde_json::Value::Null
    );
}

#[test]
fn head_info_dto_serializes_camel_case_with_operation_name() {
    let dto = HeadInfoDto::from(git_core::repo::HeadInfo {
        head_id: Some("abc".into()),
        detached_at: None,
        head_pushed: true,
        operation: Some(git_core::repo::OperationKind::CherryPick),
        operation_leftover: true,
        squash_message: None,
        commit_hooks: vec!["pre-commit".into()],
        sequence: None,
        rebase: None,
        bisect: None,
        terminal_sequence: None,
        terminal_rebase: None,
    });
    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        serde_json::json!({"headId": "abc", "detachedAt": null, "headPushed": true, "operation": "CherryPick", "operationLeftover": true, "squashMessage": null, "commitHooks": ["pre-commit"], "sequence": null, "rebase": null, "bisect": null, "terminalSequence": null, "terminalRebase": null})
    );
    let dto = HeadInfoDto::from(git_core::repo::HeadInfo {
        head_id: Some("abc".into()),
        detached_at: None,
        head_pushed: false,
        operation: None,
        operation_leftover: false,
        squash_message: Some("Squashed commit of the following:\n".into()),
        commit_hooks: vec![],
        sequence: None,
        rebase: None,
        bisect: None,
        terminal_sequence: None,
        terminal_rebase: None,
    });
    assert_eq!(
        serde_json::to_value(dto).unwrap()["squashMessage"],
        serde_json::json!("Squashed commit of the following:\n")
    );
}

#[test]
fn sequence_outcome_stopped_serializes_the_kept_commit_and_progress() {
    use git_core::pick::{SequenceKind, SequenceOutcome, SequenceProgress};
    let stopped = serde_json::to_value(SequenceOutcomeDto::from(SequenceOutcome::Stopped {
        kept: "f one".into(),
        progress: SequenceProgress {
            kind: SequenceKind::CherryPick,
            current_step: 2,
            total_steps: 3,
            current_commit: "c2".into(),
        },
    }))
    .unwrap();
    assert_eq!(stopped["kind"], "Stopped");
    assert_eq!(stopped["kept"], "f one");
    assert_eq!(stopped["progress"]["currentStep"], 2);
    assert_eq!(stopped["progress"]["currentCommit"], "c2");
}

#[test]
fn sequence_outcome_hook_failed_serializes_with_hook_output_and_progress() {
    use git_core::pick::{SequenceKind, SequenceOutcome, SequenceProgress};
    let failed = serde_json::to_value(SequenceOutcomeDto::from(SequenceOutcome::HookFailed {
        hook: "commit-msg".into(),
        output: "x".into(),
        progress: SequenceProgress {
            kind: SequenceKind::CherryPick,
            current_step: 2,
            total_steps: 3,
            current_commit: "abc".into(),
        },
    }))
    .unwrap();
    assert_eq!(
        failed,
        serde_json::json!({
            "kind": "HookFailed",
            "hook": "commit-msg",
            "output": "x",
            "progress": { "kind": "CherryPick", "currentStep": 2, "totalSteps": 3, "currentCommit": "abc" }
        })
    );
}

#[test]
fn sequence_outcome_and_progress_dtos_serialize_tagged_camel_case() {
    use git_core::pick::{SequenceKind, SequenceOutcome, SequenceProgress};
    let progress = SequenceProgress {
        kind: SequenceKind::CherryPick,
        current_step: 2,
        total_steps: 5,
        current_commit: "abc".into(),
    };
    let conflicted = serde_json::to_value(SequenceOutcomeDto::from(SequenceOutcome::Conflicted {
        files: vec!["a.txt".into()],
        progress: progress.clone(),
    }))
    .unwrap();
    assert_eq!(
        conflicted,
        serde_json::json!({
            "kind": "Conflicted",
            "files": ["a.txt"],
            "progress": { "kind": "CherryPick", "currentStep": 2, "totalSteps": 5, "currentCommit": "abc" }
        })
    );
    let completed = serde_json::to_value(SequenceOutcomeDto::from(SequenceOutcome::Completed {
        applied: 2,
        skipped: vec!["s".into()],
    }))
    .unwrap();
    assert_eq!(
        completed,
        serde_json::json!({ "kind": "Completed", "applied": 2, "skipped": ["s"] })
    );

    let head = |sequence| {
        serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
            head_id: None,
            detached_at: None,
            head_pushed: false,
            operation: None,
            operation_leftover: false,
            squash_message: None,
            commit_hooks: vec![],
            sequence,
            rebase: None,
            bisect: None,
            terminal_sequence: None,
            terminal_rebase: None,
        }))
        .unwrap()
    };
    let reverting = head(Some(SequenceProgress {
        kind: SequenceKind::Revert,
        ..progress
    }));
    assert_eq!(reverting["sequence"]["kind"], "Revert");
    assert_eq!(reverting["sequence"]["currentStep"], 2);
    let none = head(None);
    assert!(none.as_object().unwrap().contains_key("sequence"));
    assert_eq!(none["sequence"], serde_json::Value::Null);
}

#[test]
fn sequence_abort_dto_and_conflict_side_input() {
    assert_eq!(
        serde_json::to_value(SequenceAbortDto::from(git_core::pick::SequenceAbort {
            rewound: false
        }))
        .unwrap(),
        serde_json::json!({ "rewound": false })
    );
    let side: ConflictSideDto = serde_json::from_value(serde_json::json!("Theirs")).unwrap();
    assert_eq!(
        git_core::merge::ConflictSide::from(side),
        git_core::merge::ConflictSide::Theirs
    );
    assert!(serde_json::from_value::<ConflictSideDto>(serde_json::json!("Base")).is_err());
}

#[test]
fn merge_options_dto_round_trips_camel_case() {
    let dto: MergeOptionsDto =
        serde_json::from_value(serde_json::json!({"fastForward": "Only", "squash": true})).unwrap();
    assert_eq!(
        git_core::merge::MergeOptions::from(dto),
        git_core::merge::MergeOptions {
            fast_forward: git_core::merge::FastForward::Only,
            squash: true
        }
    );
    assert_eq!(
        serde_json::to_value(MergeOptionsDto::from(
            git_core::merge::MergeOptions::default()
        ))
        .unwrap(),
        serde_json::json!({"fastForward": "Allow", "squash": false})
    );
    let never: MergeOptionsDto =
        serde_json::from_value(serde_json::json!({"fastForward": "Never", "squash": false}))
            .unwrap();
    let preference = config::MergePreference {
        repo_path: "/r".into(),
        fast_forward: config::MergeFastForward::Never,
        squash: false,
    };
    assert_eq!(
        serde_json::to_value(MergeOptionsDto::from(preference)).unwrap(),
        serde_json::to_value(never).unwrap()
    );
}

#[test]
fn merge_outcome_dto_has_squashed() {
    assert_eq!(
        serde_json::to_value(MergeOutcomeDto::from(
            git_core::merge::MergeOutcome::Squashed
        ))
        .unwrap(),
        serde_json::json!({"kind": "Squashed"})
    );
}

#[test]
fn stash_dtos_serialize_camel_case() {
    let opts: StashOptionsDto = serde_json::from_value(serde_json::json!(
        {"message": null, "keepIndex": true, "includeUntracked": false, "paths": ["a.txt"]}
    ))
    .unwrap();
    assert_eq!(
        git_core::stash::StashOptions::from(opts),
        git_core::stash::StashOptions {
            message: None,
            keep_index: true,
            include_untracked: false,
            paths: vec!["a.txt".into()]
        }
    );
    assert_eq!(
        serde_json::to_value(StashApplyOutcomeDto::from(
            git_core::stash::StashApplyOutcome::Conflicted {
                files: vec!["f".into()]
            }
        ))
        .unwrap(),
        serde_json::json!({"kind": "Conflicted", "files": ["f"]})
    );
    assert_eq!(
        serde_json::to_value(StashApplyOutcomeDto::from(
            git_core::stash::StashApplyOutcome::Applied
        ))
        .unwrap(),
        serde_json::json!({"kind": "Applied"})
    );
    assert_eq!(
        serde_json::to_value(StashFilesDto::from(git_core::stash::StashFiles {
            tracked: vec!["t".into()],
            untracked: vec![],
            untracked_commit_id: None
        }))
        .unwrap(),
        serde_json::json!({"tracked": ["t"], "untracked": [], "untrackedCommitId": null})
    );
}

#[test]
fn pick_outcome_dto_is_kind_tagged() {
    assert_eq!(
        serde_json::to_value(PickOutcomeDto::from(
            git_core::pick::PickOutcome::Committed {
                commit_id: "c1".into()
            }
        ))
        .unwrap(),
        serde_json::json!({"kind": "Committed", "commitId": "c1"})
    );
    assert_eq!(
        serde_json::to_value(PickOutcomeDto::from(
            git_core::pick::PickOutcome::Conflicted {
                files: vec!["a".into()]
            }
        ))
        .unwrap(),
        serde_json::json!({"kind": "Conflicted", "files": ["a"]})
    );
}

#[test]
fn commit_signature_dto_serializes_for_the_frontend() {
    use git_core::signing::{CommitSignature, SignatureFormat, SignatureStatus};
    let dto = CommitSignatureDto::from(CommitSignature {
        status: SignatureStatus::GoodUntrusted,
        format: Some(SignatureFormat::OpenPgp),
        signer: Some("Jane <j@e>".into()),
        key: Some("ABCD".into()),
        detail: None,
    });
    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        serde_json::json!({
            "status": "goodUntrusted", "format": "openpgp",
            "signer": "Jane <j@e>", "key": "ABCD", "detail": null
        })
    );
    let unsigned = CommitSignatureDto::from(CommitSignature {
        status: SignatureStatus::Unsigned,
        format: None,
        signer: None,
        key: None,
        detail: None,
    });
    assert_eq!(
        serde_json::to_value(unsigned).unwrap()["status"],
        "unsigned"
    );
    for (status, wire) in [
        (SignatureStatus::Good, "good"),
        (SignatureStatus::Bad, "bad"),
        (SignatureStatus::UnknownKey, "unknownKey"),
        (SignatureStatus::Expired, "expired"),
        (SignatureStatus::ExpiredKey, "expiredKey"),
        (SignatureStatus::RevokedKey, "revokedKey"),
        (SignatureStatus::Error, "error"),
    ] {
        let dto = CommitSignatureDto::from(CommitSignature {
            status,
            format: Some(SignatureFormat::Ssh),
            signer: None,
            key: None,
            detail: None,
        });
        let value = serde_json::to_value(dto).unwrap();
        assert_eq!(value["status"], wire);
        assert_eq!(value["format"], "ssh");
    }
}

#[test]
fn undo_dtos_serialize_for_the_frontend() {
    use repo_service::worker::{UndoOutcome, UndoState};
    let state = UndoStateDto::from(UndoState {
        undo: Some("Stage a.txt".into()),
        redo: None,
    });
    assert_eq!(
        serde_json::to_value(state).unwrap(),
        serde_json::json!({ "undo": "Stage a.txt", "redo": null })
    );
    let cases = [
        (
            UndoOutcome::Done {
                label: "Stage a.txt".into(),
            },
            serde_json::json!({ "kind": "Done", "label": "Stage a.txt" }),
        ),
        (
            UndoOutcome::Nothing,
            serde_json::json!({ "kind": "Nothing" }),
        ),
        (
            UndoOutcome::RepositoryChanged { label: "x".into() },
            serde_json::json!({ "kind": "RepositoryChanged", "label": "x" }),
        ),
        (
            UndoOutcome::BranchHeld {
                label: "x".into(),
                message: "branch 'b' is checked out in another worktree at '/w'".into(),
            },
            serde_json::json!({
                "kind": "BranchHeld",
                "label": "x",
                "message": "branch 'b' is checked out in another worktree at '/w'",
            }),
        ),
        (
            UndoOutcome::OperationInProgress,
            serde_json::json!({ "kind": "OperationInProgress" }),
        ),
    ];
    for (outcome, expected) in cases {
        assert_eq!(
            serde_json::to_value(UndoOutcomeDto::from(outcome)).unwrap(),
            expected
        );
    }
}

#[test]
fn reset_mode_dto_parses_lowercase() {
    let mode: ResetModeDto = serde_json::from_value(serde_json::json!("hard")).unwrap();
    assert_eq!(
        git_core::reset::ResetMode::from(mode),
        git_core::reset::ResetMode::Hard
    );
}

fn head_info_with_rebase(health: git_core::rebase::RebaseHealth) -> serde_json::Value {
    serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
        head_id: None,
        detached_at: None,
        head_pushed: false,
        operation: Some(git_core::repo::OperationKind::CherryPick),
        operation_leftover: false,
        squash_message: None,
        commit_hooks: vec![],
        sequence: None,
        rebase: Some(git_core::repo::RebaseInfo {
            branch: "feature/x".into(),
            onto: "c0ffee".into(),
            current_step: 1,
            total_steps: 2,
            health,
            editing: None,
            split_parts: 0,
        }),
        bisect: None,
        terminal_sequence: None,
        terminal_rebase: None,
    }))
    .unwrap()
}

#[test]
fn confirmed_abort_dto_deserializes_camel_case() {
    let dto: ConfirmedAbortDto = serde_json::from_value(serde_json::json!({
        "headId": "h",
        "startedAt": "s",
        "rewinds": true,
        "commitsToRemove": 2,
        "pausedOnConflict": true
    }))
    .unwrap();
    assert_eq!(
        git_core::pick::ConfirmedAbort::from(dto),
        git_core::pick::ConfirmedAbort {
            head_id: "h".into(),
            started_at: "s".into(),
            rewinds: true,
            commits_to_remove: 2,
            paused_on_conflict: true,
        }
    );
}

#[test]
fn head_info_dto_serializes_a_terminal_sequence() {
    let head = |terminal_sequence| {
        serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
            head_id: None,
            detached_at: None,
            head_pushed: false,
            operation: None,
            operation_leftover: false,
            squash_message: None,
            commit_hooks: vec![],
            sequence: None,
            rebase: None,
            bisect: None,
            terminal_sequence,
            terminal_rebase: None,
        }))
        .unwrap()
    };
    let paused = head(Some(git_core::pick::TerminalSequence {
        kind: git_core::pick::SequenceKind::CherryPick,
        paused_on_conflict: false,
        started_at: "abc".into(),
        started_at_summary: "X".into(),
        rewinds: true,
        commits_to_remove: 1,
        damage: None,
    }));
    assert_eq!(
        paused["terminalSequence"],
        serde_json::json!({
            "kind": "CherryPick",
            "pausedOnConflict": false,
            "startedAt": "abc",
            "startedAtSummary": "X",
            "rewinds": true,
            "commitsToRemove": 1
        })
    );
    let none = head(None);
    assert!(none.as_object().unwrap().contains_key("terminalSequence"));
    assert_eq!(none["terminalSequence"], serde_json::Value::Null);
}

#[test]
fn head_info_dto_serializes_a_terminal_rebase() {
    let head = |terminal_rebase| {
        serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
            head_id: None,
            detached_at: None,
            head_pushed: false,
            operation: None,
            operation_leftover: false,
            squash_message: None,
            commit_hooks: vec![],
            sequence: None,
            rebase: None,
            bisect: None,
            terminal_sequence: None,
            terminal_rebase,
        }))
        .unwrap()
    };
    let rebasing = head(Some(git_core::repo::TerminalRebase {
        kind: git_core::repo::TerminalRebaseKind::Rebase,
        branch: "topic".into(),
        onto: "abc".into(),
        current_step: 2,
        total_steps: 3,
    }));
    assert_eq!(
        rebasing["terminalRebase"],
        serde_json::json!({
            "kind": "Rebase",
            "branch": "topic",
            "onto": "abc",
            "currentStep": 2,
            "totalSteps": 3
        })
    );
    let am = head(Some(git_core::repo::TerminalRebase {
        kind: git_core::repo::TerminalRebaseKind::Am,
        branch: String::new(),
        onto: String::new(),
        current_step: 1,
        total_steps: 2,
    }));
    assert_eq!(am["terminalRebase"]["kind"], "Am");
    let none = head(None);
    assert!(none.as_object().unwrap().contains_key("terminalRebase"));
    assert_eq!(none["terminalRebase"], serde_json::Value::Null);
}

#[test]
fn head_info_dto_serializes_a_bisect() {
    let head = |bisect| {
        serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
            head_id: None,
            detached_at: None,
            head_pushed: false,
            operation: None,
            operation_leftover: false,
            squash_message: None,
            commit_hooks: vec![],
            sequence: None,
            rebase: None,
            bisect,
            terminal_sequence: None,
            terminal_rebase: None,
        }))
        .unwrap()
    };
    let bisecting = head(Some(git_core::repo::BisectInfo {
        started_from: "main".into(),
    }));
    assert_eq!(
        bisecting["bisect"],
        serde_json::json!({ "startedFrom": "main" })
    );
    let none = head(None);
    assert!(none.as_object().unwrap().contains_key("bisect"));
    assert_eq!(none["bisect"], serde_json::Value::Null);
}

#[test]
fn head_info_dto_serializes_a_paused_rebase() {
    let json = head_info_with_rebase(git_core::rebase::RebaseHealth::Paused);
    assert_eq!(
        json["rebase"],
        serde_json::json!({
            "branch": "feature/x",
            "onto": "c0ffee",
            "currentStep": 1,
            "totalSteps": 2,
            "health": { "kind": "Paused" },
            "editing": null,
            "splitParts": 0,
        })
    );
}

#[test]
fn head_info_dto_serializes_a_diverged_rebase_reason() {
    use git_core::rebase::{RebaseDivergence, RebaseHealth};
    for (reason, name) in [
        (RebaseDivergence::HeadMoved, "HeadMoved"),
        (RebaseDivergence::BranchMoved, "BranchMoved"),
        (RebaseDivergence::BranchMissing, "BranchMissing"),
        (RebaseDivergence::OtherOperation, "OtherOperation"),
    ] {
        let json = head_info_with_rebase(RebaseHealth::Diverged(reason));
        assert_eq!(
            json["rebase"]["health"],
            serde_json::json!({ "kind": "Diverged", "reason": name })
        );
    }
}

#[test]
fn rebase_abort_dto_serializes_restored() {
    assert_eq!(
        serde_json::to_value(RebaseAbortDto::from(git_core::rebase::RebaseAbort {
            restored: false
        }))
        .unwrap(),
        serde_json::json!({ "restored": false })
    );
}
