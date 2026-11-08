use super::*;

#[test]
fn graph_commit_dto_serializes_camel_case_fields() {
    let dto = GraphCommitDto::from(GraphCommit {
        id: "abc123".into(),
        short_id: "abc123".into(),
        summary: "Add remote branch badges".into(),
        author_name: "Rene".into(),
        author_email: "rene@example.com".into(),
        timestamp: 1_700_000_000,
        parent_ids: vec!["def456".into()],
        branch_refs: vec!["main".into()],
        remote_branch_refs: vec!["origin/main".into()],
        tags: vec!["v1.0".into()],
    });

    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        serde_json::json!({
            "id": "abc123",
            "shortId": "abc123",
            "summary": "Add remote branch badges",
            "authorName": "Rene",
            "authorEmail": "rene@example.com",
            "timestamp": 1_700_000_000i64,
            "parentIds": ["def456"],
            "branchRefs": ["main"],
            "remoteBranchRefs": ["origin/main"],
            "tags": ["v1.0"],
        })
    );
}

#[test]
fn file_history_entry_dto_serializes_camel_case_and_null_old_path() {
    let commit = GraphCommit {
        id: "abc123".into(),
        short_id: "abc123".into(),
        summary: "Edit".into(),
        author_name: "Rene".into(),
        author_email: "rene@example.com".into(),
        timestamp: 1_700_000_000,
        parent_ids: vec![],
        branch_refs: vec![],
        remote_branch_refs: vec![],
        tags: vec![],
    };
    let plain = FileHistoryEntryDto::from(git_core::history::FileHistoryEntry {
        commit: commit.clone(),
        path: "new.txt".into(),
        old_path: None,
    });
    let renamed = FileHistoryEntryDto::from(git_core::history::FileHistoryEntry {
        commit,
        path: "new.txt".into(),
        old_path: Some("old.txt".into()),
    });

    let plain = serde_json::to_value(plain).unwrap();
    assert_eq!(plain["path"], "new.txt");
    assert_eq!(plain["oldPath"], serde_json::Value::Null);
    assert!(plain.as_object().unwrap().contains_key("oldPath"));
    assert_eq!(plain["commit"]["shortId"], "abc123");
    assert_eq!(serde_json::to_value(renamed).unwrap()["oldPath"], "old.txt");
}

#[test]
fn search_page_dto_serializes_camel_case_and_null_cursor() {
    let commit = GraphCommit {
        id: "abc123".into(),
        short_id: "abc123".into(),
        summary: "Hit".into(),
        author_name: "Rene".into(),
        author_email: "rene@example.com".into(),
        timestamp: 1_700_000_000,
        parent_ids: vec![],
        branch_refs: vec![],
        remote_branch_refs: vec![],
        tags: vec![],
    };
    let done = serde_json::to_value(SearchPageDto::from(git_core::search::SearchPage {
        commits: vec![commit.clone()],
        next_cursor: None,
        scanned: 42,
    }))
    .unwrap();
    assert_eq!(done["nextCursor"], serde_json::Value::Null);
    assert!(done.as_object().unwrap().contains_key("nextCursor"));
    assert_eq!(done["scanned"], 42);
    assert_eq!(done["commits"][0]["shortId"], "abc123");

    let more = serde_json::to_value(SearchPageDto::from(git_core::search::SearchPage {
        commits: vec![],
        next_cursor: Some("20000:0123456789abcdef".into()),
        scanned: 20_000,
    }))
    .unwrap();
    assert_eq!(more["nextCursor"], "20000:0123456789abcdef");
}

#[test]
fn compare_dtos_serialize_camel_case_with_a_null_old_path() {
    let plain = serde_json::to_value(ChangedFileDto::from(git_core::diff::ChangedFile {
        path: "a.txt".into(),
        old_path: None,
    }))
    .unwrap();
    assert_eq!(
        plain,
        serde_json::json!({ "path": "a.txt", "oldPath": null })
    );

    let renamed = serde_json::to_value(ChangedFileDto::from(git_core::diff::ChangedFile {
        path: "new.txt".into(),
        old_path: Some("old.txt".into()),
    }))
    .unwrap();
    assert_eq!(renamed["oldPath"], "old.txt");

    let range = serde_json::to_value(CompareRangeDto::from(git_core::diff::CompareRange {
        from_id: "base".into(),
        to_id: "tip".into(),
    }))
    .unwrap();
    assert_eq!(
        range,
        serde_json::json!({ "fromId": "base", "toId": "tip" })
    );
}

#[test]
fn worktree_info_dto_serializes_camel_case_fields() {
    let dto = WorktreeInfoDto::from(WorktreeInfo {
        name: "feature-tree".into(),
        path: PathBuf::from("/repos/project-feature"),
        head: "refs/heads/feature".into(),
        is_main: false,
        is_locked: true,
        is_prunable: false,
    });

    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        serde_json::json!({
            "name": "feature-tree",
            "path": "/repos/project-feature",
            "head": "refs/heads/feature",
            "isMain": false,
            "isLocked": true,
            "isPrunable": false,
        })
    );
}

#[test]
fn worktree_info_dto_serializes_an_unknown_head_as_null() {
    let dto = WorktreeInfoDto::from(WorktreeInfo {
        name: "stale-tree".into(),
        path: PathBuf::from("/repos/stale-tree"),
        head: String::new(),
        is_main: false,
        is_locked: false,
        is_prunable: true,
    });

    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        serde_json::json!({
            "name": "stale-tree",
            "path": "/repos/stale-tree",
            "head": null,
            "isMain": false,
            "isLocked": false,
            "isPrunable": true,
        })
    );
}

#[test]
fn submodule_info_dto_serializes_camel_case_fields() {
    let dto = SubmoduleInfoDto::from(SubmoduleInfo {
        path: "deps/child".into(),
        url: Some("https://example.com/child.git".into()),
        gitlink_id: Some("0123456789abcdef".into()),
        initialized: true,
        head_id: Some("fedcba9876543210".into()),
        workdir: Some("/repos/app/deps/child".into()),
    });

    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        serde_json::json!({
            "path": "deps/child",
            "url": "https://example.com/child.git",
            "gitlinkId": "0123456789abcdef",
            "initialized": true,
            "headId": "fedcba9876543210",
            "workdir": "/repos/app/deps/child",
        })
    );
}

#[test]
fn reflog_entry_dto_serializes_camel_case_fields_and_optional_summary() {
    let with_summary = ReflogEntryDto::from(ReflogEntry {
        reference: "HEAD".into(),
        old_id: "1111111".into(),
        new_id: "2222222".into(),
        committer_name: "Test User".into(),
        committer_email: "test@example.com".into(),
        timestamp: 1_725_000_000,
        message: "commit: second commit".into(),
        summary: Some("second commit".into()),
    });
    let without_summary = ReflogEntryDto::from(ReflogEntry {
        reference: "refs/heads/main".into(),
        old_id: "2222222".into(),
        new_id: "3333333".into(),
        committer_name: "Test User".into(),
        committer_email: "test@example.com".into(),
        timestamp: 1_725_000_001,
        message: "branch: reset".into(),
        summary: None,
    });

    assert_eq!(
        serde_json::to_value(with_summary).unwrap(),
        serde_json::json!({
            "reference": "HEAD",
            "oldId": "1111111",
            "newId": "2222222",
            "committerName": "Test User",
            "committerEmail": "test@example.com",
            "timestamp": 1_725_000_000,
            "message": "commit: second commit",
            "summary": "second commit",
        })
    );
    assert_eq!(
        serde_json::to_value(without_summary).unwrap()["summary"],
        serde_json::Value::Null
    );
}
