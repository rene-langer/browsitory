use super::*;

#[test]
fn tag_info_dto_serializes_the_peeled_commit_id_in_camel_case() {
    let tag = |commit_id: Option<&str>| git_core::remote::TagInfo {
        name: "v1".into(),
        target_id: "tagobj".into(),
        commit_id: commit_id.map(str::to_string),
        annotated: true,
        message: Some("m".into()),
        tagger_name: None,
        timestamp: None,
    };
    let json = serde_json::to_value(TagInfoDto::from(tag(Some("abc")))).unwrap();
    assert_eq!(json["targetId"], "tagobj");
    assert_eq!(json["commitId"], "abc");
    let json = serde_json::to_value(TagInfoDto::from(tag(None))).unwrap();
    assert_eq!(json["commitId"], serde_json::Value::Null);
}

#[test]
fn file_sides_dto_encodes_base64_sniffs_the_type_and_tags_each_side() {
    use git_core::diff::{FileSide, FileSides};
    let png = b"\x89PNG\r\n\x1a\n".to_vec();
    let dto = serde_json::to_value(FileSidesDto::from(FileSides {
        old: FileSide::Loaded {
            bytes: b"foo".to_vec(),
        },
        new: FileSide::Loaded { bytes: png },
    }))
    .unwrap();
    assert_eq!(
        dto,
        serde_json::json!({
            "old": { "kind": "Loaded", "size": 3, "base64": "Zm9v", "mime": null },
            "new": { "kind": "Loaded", "size": 8, "base64": "iVBORw0KGgo=", "mime": "image/png" },
        })
    );
    let big = serde_json::to_value(FileSidesDto::from(FileSides {
        old: FileSide::TooLarge { size: 25_000_000 },
        new: FileSide::Missing,
    }))
    .unwrap();
    assert_eq!(
        big,
        serde_json::json!({
            "old": { "kind": "TooLarge", "size": 25_000_000 },
            "new": { "kind": "Missing" },
        })
    );
}

#[test]
fn commit_inputs_and_settings_dtos_are_camel_case() {
    let trailer: TrailerDto =
        serde_json::from_value(serde_json::json!({ "key": "Co-authored-by", "value": "A <a@x>" }))
            .unwrap();
    assert_eq!(
        git_core::commit::Trailer::from(trailer),
        git_core::commit::Trailer {
            key: "Co-authored-by".into(),
            value: "A <a@x>".into()
        }
    );
    let author: SignatureInputDto =
        serde_json::from_value(serde_json::json!({ "name": "Ada", "email": "a@x" })).unwrap();
    assert_eq!(git_core::commit::SignatureInput::from(author).email, "a@x");
    assert_eq!(
        serde_json::to_value(SignatureInputDto::from(git_core::commit::SignatureInput {
            name: "Bob".into(),
            email: "b@x".into()
        }))
        .unwrap(),
        serde_json::json!({ "name": "Bob", "email": "b@x" })
    );
    let settings = serde_json::to_value(CommitMessageSettingsDto::from(
        config::CommitMessageSettings::default(),
    ))
    .unwrap();
    assert_eq!(
        settings,
        serde_json::json!({ "summaryLimit": 50, "bodyRuler": 72, "showRuler": true })
    );
    let back: CommitMessageSettingsDto = serde_json::from_value(
        serde_json::json!({ "summaryLimit": 60, "bodyRuler": 80, "showRuler": false }),
    )
    .unwrap();
    assert_eq!(
        config::CommitMessageSettings::from(back),
        config::CommitMessageSettings {
            summary_limit: 60,
            body_ruler: 80,
            show_ruler: false
        }
    );
}

#[test]
fn diff_options_dto_parses_camel_case() {
    let dto: DiffOptionsDto =
        serde_json::from_value(serde_json::json!({"ignoreWhitespace": true, "contextLines": 5}))
            .unwrap();
    assert_eq!(
        git_core::diff::DiffOptions::from(dto),
        git_core::diff::DiffOptions {
            ignore_whitespace: true,
            context_lines: 5
        }
    );
}

#[test]
fn status_entry_dto_always_serializes_old_path() {
    use git_core::status::{StatusEntry, StatusKind};
    let plain = StatusEntryDto::from(StatusEntry {
        path: "a.txt".into(),
        old_path: None,
        staged: false,
        kind: StatusKind::Modified,
    });
    assert_eq!(
        serde_json::to_value(plain).unwrap(),
        serde_json::json!({"path": "a.txt", "oldPath": null, "staged": false, "kind": "Modified"})
    );
    let renamed = StatusEntryDto::from(StatusEntry {
        path: "new.txt".into(),
        old_path: Some("old.txt".into()),
        staged: true,
        kind: StatusKind::Renamed,
    });
    assert_eq!(
        serde_json::to_value(renamed).unwrap()["oldPath"],
        serde_json::json!("old.txt")
    );
}

#[test]
fn diff_source_dto_parses_every_variant() {
    use git_core::tools::DiffSource;
    use repo_service::wire::DiffSourceDto;
    let parse = |v: serde_json::Value| {
        DiffSource::from(serde_json::from_value::<DiffSourceDto>(v).unwrap())
    };
    assert_eq!(
        parse(serde_json::json!({"kind": "Unstaged"})),
        DiffSource::Unstaged
    );
    assert_eq!(
        parse(serde_json::json!({"kind": "Staged"})),
        DiffSource::Staged { old_path: None }
    );
    assert_eq!(
        parse(serde_json::json!({"kind": "Staged", "oldPath": "a.png"})),
        DiffSource::Staged {
            old_path: Some("a.png".into())
        }
    );
    assert_eq!(
        parse(serde_json::json!({"kind": "Commit", "commitId": "abc", "oldPath": null})),
        DiffSource::Commit {
            commit_id: "abc".into(),
            old_path: None
        }
    );
    assert_eq!(
        parse(
            serde_json::json!({"kind": "Range", "fromId": "a", "toId": "b", "oldPath": "old.rs"})
        ),
        DiffSource::Range {
            from_id: "a".into(),
            to_id: Some("b".into()),
            old_path: Some("old.rs".into())
        }
    );
    assert_eq!(
        parse(serde_json::json!({"kind": "Range", "fromId": "a", "toId": null, "oldPath": null})),
        DiffSource::Range {
            from_id: "a".into(),
            to_id: None,
            old_path: None
        }
    );
    assert!(serde_json::from_value::<DiffSourceDto>(serde_json::json!({"kind": "Nope"})).is_err());
}

#[test]
fn tool_outcome_and_settings_dtos_serialize_for_the_frontend() {
    use super::super::MergeToolOutcomeDto;
    use repo_service::wire::{ExternalToolSettingsDto, GitToolsDto, MarkResolvedOutcomeDto};
    assert_eq!(
        serde_json::to_value(MergeToolOutcomeDto::from(
            git_core::tools::MergeToolOutcome::Resolved
        ))
        .unwrap(),
        "Resolved"
    );
    assert_eq!(
        serde_json::to_value(MergeToolOutcomeDto::from(
            git_core::tools::MergeToolOutcome::Unresolved
        ))
        .unwrap(),
        "Unresolved"
    );
    assert_eq!(
        serde_json::to_value(MarkResolvedOutcomeDto::from(
            git_core::merge::MarkResolvedOutcome::MarkersRemain
        ))
        .unwrap(),
        "MarkersRemain"
    );
    let settings: ExternalToolSettingsDto =
        serde_json::from_value(serde_json::json!({"editor": "code", "terminal": null})).unwrap();
    assert_eq!(
        config::ExternalToolSettings::from(settings),
        config::ExternalToolSettings {
            editor: Some("code".into()),
            terminal: None
        }
    );
    assert_eq!(
        serde_json::to_value(ExternalToolSettingsDto::from(
            config::ExternalToolSettings {
                editor: None,
                terminal: Some("kitty".into())
            }
        ))
        .unwrap(),
        serde_json::json!({"editor": null, "terminal": "kitty"})
    );
    let tools = GitToolsDto::from(git_core::tools::GitTools {
        diff_tool: Some(git_core::tools::GitToolInfo {
            name: "meld".into(),
            command: Some("meld $LOCAL $REMOTE".into()),
            source: git_core::tools::ConfigSource::Global,
            via_merge_tool: true,
            problem: None,
        }),
        merge_tool: None,
    });
    assert_eq!(
        serde_json::to_value(tools).unwrap(),
        serde_json::json!({
            "diffTool": {"name": "meld", "command": "meld $LOCAL $REMOTE", "source": "Global", "viaMergeTool": true, "problem": null},
            "mergeTool": null
        })
    );
}

#[test]
fn diff_settings_dto_round_trips_camel_case() {
    let dto = DiffSettingsDto::from(config::DiffSettings {
        ignore_whitespace: false,
        context_lines: 3,
        syntax_highlight: true,
    });
    let json = serde_json::to_value(&dto).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"ignoreWhitespace": false, "contextLines": 3, "syntaxHighlight": true})
    );
    let back: DiffSettingsDto = serde_json::from_value(json).unwrap();
    assert_eq!(config::DiffSettings::from(back).context_lines, 3);
}

#[test]
fn workspace_dto_serializes_camel_case_paths() {
    let dto = WorkspaceDto::from(Workspace {
        id: "workspace-1".into(),
        name: "Project Suite".into(),
        root_path: PathBuf::from("/repos/suite"),
        member_paths: vec![
            PathBuf::from("/repos/suite/api"),
            PathBuf::from("/repos/suite/web"),
        ],
    });

    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        serde_json::json!({
            "id": "workspace-1",
            "name": "Project Suite",
            "rootPath": "/repos/suite",
            "memberPaths": ["/repos/suite/api", "/repos/suite/web"],
        })
    );
}

#[test]
fn open_repo_entry_dto_serializes_camel_case_workspace_id() {
    let dto = OpenRepoEntryDto::from(OpenRepoEntry {
        path: PathBuf::from("/repos/suite/api"),
        workspace_id: Some("workspace-1".into()),
    });

    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        serde_json::json!({
            "path": "/repos/suite/api",
            "workspaceId": "workspace-1",
        })
    );
}

#[test]
fn open_repo_entry_input_deserializes_camel_case_workspace_id() {
    let input: OpenRepoEntryInput = serde_json::from_value(serde_json::json!({
        "path": "/repos/suite/api",
        "workspaceId": "workspace-1",
    }))
    .unwrap();

    assert_eq!(input.path, "/repos/suite/api");
    assert_eq!(input.workspace_id, Some("workspace-1".into()));
}
