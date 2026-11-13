use std::sync::mpsc;

use super::*;

/// A key no other test uses, so the process-wide map never mixes two tests' entries.
fn unique_key(name: &str) -> String {
    format!("/oplog-test/{name}/{:?}", std::thread::current().id())
}

fn start(name: &str) -> Start {
    Start {
        name: name.to_string(),
        ..Start::default()
    }
}

fn names(page: &OperationPage) -> Vec<&str> {
    page.entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect()
}

#[test]
fn begin_adds_a_running_entry_and_finish_ends_it() {
    let journal = journal(&unique_key("begin"));
    let id = journal.begin(Start {
        name: "Push".into(),
        target: Some("origin".into()),
        params: vec![OperationParam {
            name: "Skip hooks".into(),
            value: "no".into(),
        }],
        rerun: Some(Rerun::Pull),
    });

    let running = journal.page(None, 10);
    assert_eq!(running.repo, journal.key());
    assert_eq!(running.entries.len(), 1);
    let entry = &running.entries[0];
    assert_eq!(entry.id, id);
    assert_eq!(entry.name, "Push");
    assert_eq!(entry.target.as_deref(), Some("origin"));
    assert_eq!(entry.outcome, OperationOutcome::Running);
    assert_eq!(entry.duration_ms, None);
    assert_eq!(entry.rerun, Some(Rerun::Pull));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert!(
        (now - entry.started_at).abs() < 60_000,
        "started_at is the Unix time in milliseconds: {}",
        entry.started_at
    );

    journal.finish(
        id,
        Finish {
            outcome: OperationOutcome::Failed,
            message: Some("rejected".into()),
            error_kind: Some(ErrorKind::Other),
            transfer_error: Some(TransferErrorKind::NonFastForward),
            ..Finish::succeeded()
        },
    );

    let ended = &journal.page(None, 10).entries[0];
    assert_eq!(ended.outcome, OperationOutcome::Failed);
    assert_eq!(ended.message.as_deref(), Some("rejected"));
    assert_eq!(ended.error_kind, Some(ErrorKind::Other));
    assert_eq!(
        ended.transfer_error,
        Some(TransferErrorKind::NonFastForward)
    );
    assert!(ended.duration_ms.is_some());
}

#[test]
fn ids_increase_across_repositories() {
    let first = journal(&unique_key("ids-a")).begin(start("A"));
    let second = journal(&unique_key("ids-b")).begin(start("B"));
    assert!(second > first, "{first} then {second}");
}

#[test]
fn the_same_key_gives_the_same_log() {
    let key = unique_key("same");
    journal(&key).begin(start("Commit"));
    assert_eq!(names(&journal(&key).page(None, 10)), ["Commit"]);
    assert!(journal(&unique_key("other"))
        .page(None, 10)
        .entries
        .is_empty());
}

#[test]
fn pages_are_newest_first_and_continue_before_an_id() {
    let journal = journal(&unique_key("pages"));
    let ids: Vec<u64> = ["one", "two", "three", "four", "five"]
        .into_iter()
        .map(|name| journal.begin(start(name)))
        .collect();

    let first = journal.page(None, 2);
    assert_eq!(names(&first), ["five", "four"]);
    assert!(first.has_more);

    let second = journal.page(Some(ids[3]), 2);
    assert_eq!(names(&second), ["three", "two"]);
    assert!(second.has_more);

    let last = journal.page(Some(ids[1]), 2);
    assert_eq!(names(&last), ["one"]);
    assert!(!last.has_more);

    let exact = journal.page(None, 5);
    assert_eq!(exact.entries.len(), 5);
    assert!(!exact.has_more);
}

#[test]
fn the_log_keeps_only_the_newest_max_entries() {
    let journal = journal(&unique_key("cap"));
    let first = journal.begin(start("first"));
    for n in 0..MAX_ENTRIES {
        journal.begin(start(&format!("op {n}")));
    }

    let page = journal.page(None, MAX_ENTRIES + 10);
    assert_eq!(page.entries.len(), MAX_ENTRIES);
    assert!(!page.has_more);
    assert_eq!(page.entries[0].name, format!("op {}", MAX_ENTRIES - 1));
    assert_eq!(page.entries[MAX_ENTRIES - 1].name, "op 0");
    assert!(page.entries.iter().all(|entry| entry.id != first));

    // Ending a dropped entry changes nothing.
    journal.finish(first, Finish::succeeded());
    assert_eq!(
        journal.page(None, MAX_ENTRIES + 10).entries.len(),
        MAX_ENTRIES
    );
}

#[test]
fn listeners_hear_each_added_and_ended_entry_until_dropped() {
    let key = unique_key("listen");
    let (tx, rx) = mpsc::channel();
    let wanted = key.clone();
    let subscription = subscribe(Arc::new(move |change: &Change| {
        if change.repo == wanted {
            let _ = tx.send(change.entry_id);
        }
    }));
    let journal = journal(&key);

    let id = journal.begin(start("Fetch"));
    journal.finish(id, Finish::succeeded());
    assert_eq!(rx.try_iter().collect::<Vec<_>>(), [id, id]);

    drop(subscription);
    journal.begin(start("Fetch"));
    assert!(rx.try_iter().next().is_none());
}

#[test]
fn entries_keep_no_credentials() {
    let journal = journal(&unique_key("redact"));
    let id = journal.begin(Start {
        name: "Add remote".into(),
        target: Some("https://alice:s3cret@example.test/r.git".into()),
        params: vec![OperationParam {
            name: "Push URL".into(),
            value: "https://token@example.test/r.git".into(),
        }],
        rerun: None,
    });
    journal.finish(
        id,
        Finish {
            outcome: OperationOutcome::Failed,
            message: Some("could not reach https://bob:hunter2@example.test/r.git".into()),
            runs: vec![ProgramRun {
                program: "/repo/.git/hooks/pre-push".into(),
                args: vec![
                    "origin".into(),
                    "https://carol:pw@example.test/r.git".into(),
                ],
                output: "> GET /r.git\n> Authorization: Basic Y2Fyb2w6cHc=\nfailed\n".into(),
                status: ProgramRunStatus::Failed,
            }],
            remote_failures: vec![RemoteFailure {
                remote_name: "backup".into(),
                error: TransferErrorKind::TransferFailed,
                message: "ssh://dave:pw@example.test/r.git refused".into(),
            }],
            ..Finish::succeeded()
        },
    );

    let entry = &journal.page(None, 1).entries[0];
    assert_eq!(
        entry.target.as_deref(),
        Some("https://***@example.test/r.git")
    );
    assert_eq!(entry.params[0].value, "https://***@example.test/r.git");
    assert_eq!(
        entry.message.as_deref(),
        Some("could not reach https://***@example.test/r.git")
    );
    assert_eq!(
        entry.runs[0].args,
        ["origin", "https://***@example.test/r.git"]
    );
    assert_eq!(
        entry.runs[0].output,
        "> GET /r.git\n> Authorization: ***\nfailed\n"
    );
    assert_eq!(
        entry.remote_failures[0].message,
        "ssh://***@example.test/r.git refused"
    );
}

#[test]
fn entries_cap_their_runs_and_each_runs_output() {
    let journal = journal(&unique_key("runs"));
    let id = journal.begin(start("Cherry-pick 40 commits"));
    let long_output = format!("{}tail\n", "x".repeat(MAX_RUN_OUTPUT * 2));
    let runs = (0..MAX_RUNS + 5)
        .map(|n| ProgramRun {
            program: format!("hook-{n}"),
            args: Vec::new(),
            output: long_output.clone(),
            status: ProgramRunStatus::Succeeded,
        })
        .collect();
    journal.finish(
        id,
        Finish {
            runs,
            ..Finish::succeeded()
        },
    );

    let entry = &journal.page(None, 1).entries[0];
    assert_eq!(entry.runs.len(), MAX_RUNS);
    assert_eq!(entry.runs[0].program, "hook-0");
    let output = &entry.runs[0].output;
    assert!(output.starts_with(OUTPUT_OMITTED), "{}", &output[..40]);
    assert!(output.ends_with("tail\n"));
    assert!(output.len() <= OUTPUT_OMITTED.len() + MAX_RUN_OUTPUT);
}

#[test]
fn redact_removes_user_information_and_authorization_values() {
    assert_eq!(
        redact("https://user:pass@host/x and http://tok@h2:8080/y"),
        "https://***@host/x and http://***@h2:8080/y"
    );
    // A password holding an `@` (percent-encoded or not): everything up to the host goes.
    assert_eq!(redact("'https://u:p@ss@host/x'"), "'https://***@host/x'");
    // No user information, a scp-like address, or an `@` past the host: kept.
    assert_eq!(
        redact("https://host/a@b git@github.com:o/r.git"),
        "https://host/a@b git@github.com:o/r.git"
    );
    assert_eq!(
        redact("authorization: Bearer abc\nAUTHORIZATION:x\nnext"),
        "authorization: ***\nAUTHORIZATION: ***\nnext"
    );
    assert_eq!(redact("plain text"), "plain text");
}
