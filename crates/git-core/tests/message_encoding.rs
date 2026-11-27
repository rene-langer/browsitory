//! Commit messages that aren't UTF-8 (#1149): a rebase, a cherry-pick and a fold replay them in
//! UTF-8, as git does (`logmsg_reencode` by the `encoding` header, then invalid bytes read as
//! Latin-1), and write the new commit without the header. Each case is compared with git on a
//! copy of the same repository.

mod common;

use std::path::Path;

use common::{commit_all, git_ok, init_repo, write_file};
use git_core::hooks::HookOptions;
use git_core::pick::{PickOutcome, SequenceOutcome};
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};

/// "Grüße aus Köln", body "ä, Ã©", in ISO-8859-1, whose last two bytes would also read as UTF-8
/// ("é").
const LATIN1: &[u8] = b"Gr\xfc\xdfe aus K\xf6ln\n\nBody \xe4, \xc3\xa9\n";
const LATIN1_AS_UTF8: &str = "Grüße aus Köln\n\nBody ä, Ã©\n";
/// "Preis € 5" in Windows-1252, where € is 0x80.
const CP1252: &[u8] = b"Preis \x80 5\n";
const CP1252_AS_UTF8: &str = "Preis € 5\n";
/// Valid UTF-8 ("ü") next to a byte that isn't (0xfc, "ü" in Latin-1), without a header.
const MIXED: &[u8] = b"mixed \xc3\xbc and \xfc\n";
const MIXED_AS_UTF8: &str = "mixed ü and ü\n";

/// Commits everything with `git commit -F`, the message's bytes stored as they are under
/// `i18n.commitEncoding=<encoding>` (which writes the `encoding` header).
fn git_commit_encoded(dir: &Path, message: &[u8], encoding: &str) {
    let file = dir.join(".git").join("message-file");
    std::fs::write(&file, message).unwrap();
    git_ok(dir, &["add", "-A"]);
    git_ok(
        dir,
        &[
            "-c",
            &format!("i18n.commitEncoding={encoding}"),
            "commit",
            "-q",
            "-F",
            file.to_str().unwrap(),
        ],
    );
}

/// Commits the index on HEAD with `message`'s raw bytes and no `encoding` header, which neither
/// git (it fixes invalid UTF-8 up) nor git2's API (it takes a `&str`) would write.
fn commit_raw(repo: &git2::Repository, message: &[u8]) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = index.write_tree().unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let mut buffer = format!(
        "tree {tree}\nparent {}\nauthor Test User <test@example.com> 1700000000 +0000\n\
         committer Test User <test@example.com> 1700000000 +0000\n\n",
        head.id()
    )
    .into_bytes();
    buffer.extend_from_slice(message);
    let id = repo
        .odb()
        .unwrap()
        .write(git2::ObjectType::Commit, &buffer)
        .unwrap();
    let branch = repo.head().unwrap().name().unwrap().to_string();
    repo.reference(&branch, id, true, "raw commit").unwrap();
}

/// `master` with a base commit and one more, `topic` from the base with a commit adding `t`
/// whose message `commit` writes; `topic` is checked out.
fn topic_with(commit: impl Fn(&Path, &git2::Repository)) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "base\n");
    commit_all(&repo, "base");
    git_ok(dir.path(), &["branch", "-M", "master"]);
    git_ok(dir.path(), &["checkout", "-q", "-b", "topic"]);
    write_file(dir.path(), "t.txt", "t\n");
    commit(dir.path(), &repo);
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    write_file(dir.path(), "m.txt", "m\n");
    commit_all(&repo, "master moves on");
    git_ok(dir.path(), &["checkout", "-q", "topic"]);
    (dir, repo)
}

fn latin1_topic() -> (tempfile::TempDir, git2::Repository) {
    topic_with(|dir, _| git_commit_encoded(dir, LATIN1, "ISO-8859-1"))
}

/// A copy of the repository at `from`, for git to run the same operation on.
fn twin(from: &Path) -> (tempfile::TempDir, git2::Repository) {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    let dir = tempfile::TempDir::new().unwrap();
    copy(from, dir.path());
    let repo = git2::Repository::open(dir.path()).unwrap();
    (dir, repo)
}

/// The message bytes and `encoding` header of the commit `rev` names.
fn message_of(repo: &git2::Repository, rev: &str) -> (Vec<u8>, Option<String>) {
    let commit = repo.revparse_single(rev).unwrap().peel_to_commit().unwrap();
    let encoding = commit.message_encoding().unwrap().map(str::to_string);
    (commit.message_bytes().to_vec(), encoding)
}

fn utf8(text: &str) -> (Vec<u8>, Option<String>) {
    (text.as_bytes().to_vec(), None)
}

fn picks(repo: &git2::Repository, onto: &str) -> Vec<RebasePlanEntry> {
    git_core::rebase::commits_since(repo, onto)
        .unwrap()
        .into_iter()
        .map(|commit| RebasePlanEntry {
            commit_id: commit.id,
            action: RebaseAction::Pick,
            combined_message: None,
        })
        .collect()
}

fn rebase_onto_master(repo: &git2::Repository) {
    let plan = picks(repo, "master");
    let (_, result) = git_core::rebase::start_rebase(repo, "master", plan).unwrap();
    assert_eq!(result, RebaseStepResult::Done);
}

#[test]
fn a_rebase_replays_a_latin1_message_in_utf8_as_git_does() {
    let (dir, repo) = latin1_topic();
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(twin_dir.path(), &["rebase", "-q", "master"]);

    rebase_onto_master(&repo);

    assert_eq!(message_of(&repo, "topic"), message_of(&twin_repo, "topic"));
    assert_eq!(message_of(&repo, "topic"), utf8(LATIN1_AS_UTF8));
}

#[test]
fn a_rebase_reads_invalid_utf8_without_a_header_as_latin1_as_git_does() {
    let (dir, repo) = topic_with(|_, repo| commit_raw(repo, MIXED));
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(twin_dir.path(), &["rebase", "-q", "master"]);

    rebase_onto_master(&repo);

    assert_eq!(message_of(&repo, "topic"), message_of(&twin_repo, "topic"));
    assert_eq!(message_of(&repo, "topic"), utf8(MIXED_AS_UTF8));
}

#[test]
fn a_fixup_group_keeps_its_latin1_leaders_message_in_utf8() {
    let (dir, repo) = latin1_topic();
    write_file(dir.path(), "t.txt", "t2\n");
    commit_all(&repo, "fix t");
    let mut plan = picks(&repo, "master");
    plan[1].action = RebaseAction::Fixup;

    let (_, result) = git_core::rebase::start_rebase(&repo, "master", plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(message_of(&repo, "topic"), utf8(LATIN1_AS_UTF8));
}

#[test]
fn a_conflicted_latin1_step_committed_with_git_is_adopted() {
    let (dir, repo) = latin1_topic();
    // master changes t.txt too, so the step conflicts.
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    write_file(dir.path(), "t.txt", "master's t\n");
    commit_all(&repo, "master adds t");
    git_ok(dir.path(), &["checkout", "-q", "topic"]);
    let plan = picks(&repo, "master");
    let (mut state, result) = git_core::rebase::start_rebase(&repo, "master", plan).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    write_file(dir.path(), "t.txt", "resolved\n");
    git_ok(dir.path(), &["add", "t.txt"]);
    // As `git status` suggests: the editor opens on MERGE_MSG, whose comment lines git drops.
    git_ok(dir.path(), &["-c", "core.editor=true", "commit", "-q"]);

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(message_of(&repo, "topic"), utf8(LATIN1_AS_UTF8));
}

#[test]
fn a_cherry_pick_writes_a_latin1_message_in_utf8_as_git_does() {
    let (dir, repo) = latin1_topic();
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(twin_dir.path(), &["cherry-pick", "topic"]);
    let topic = repo.revparse_single("topic").unwrap().id().to_string();

    let outcome = git_core::pick::cherry_pick(&repo, &topic, None, HookOptions::default()).unwrap();

    assert!(matches!(outcome, PickOutcome::Committed { .. }));
    assert_eq!(message_of(&repo, "HEAD"), message_of(&twin_repo, "HEAD"));
    assert_eq!(message_of(&repo, "HEAD"), utf8(LATIN1_AS_UTF8));
}

#[test]
fn a_cherry_pick_writes_a_windows_1252_message_in_utf8_as_git_does() {
    let (dir, repo) = topic_with(|dir, _| git_commit_encoded(dir, CP1252, "windows-1252"));
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(twin_dir.path(), &["cherry-pick", "topic"]);
    let topic = repo.revparse_single("topic").unwrap().id().to_string();

    let outcome = git_core::pick::cherry_pick(&repo, &topic, None, HookOptions::default()).unwrap();

    assert!(matches!(outcome, PickOutcome::Committed { .. }));
    assert_eq!(message_of(&repo, "HEAD"), message_of(&twin_repo, "HEAD"));
    assert_eq!(message_of(&repo, "HEAD"), utf8(CP1252_AS_UTF8));
}

/// `topic`'s Latin-1 commit adds `t.txt`, which master (checked out) adds too: picking it
/// conflicts.
fn conflicting_latin1_pick() -> (tempfile::TempDir, git2::Repository, String) {
    let (dir, repo) = latin1_topic();
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    write_file(dir.path(), "t.txt", "master's t\n");
    commit_all(&repo, "master adds t");
    let topic = repo.revparse_single("topic").unwrap().id().to_string();
    (dir, repo, topic)
}

#[test]
fn a_cherry_pick_sequence_continued_after_a_conflict_keeps_a_latin1_message_as_git_does() {
    let (dir, repo, topic) = conflicting_latin1_pick();
    let (twin_dir, twin_repo) = twin(dir.path());
    let picked = test_support::git(twin_dir.path(), &["cherry-pick", "topic"]);
    assert!(!picked.status.success(), "git's pick should conflict");
    write_file(twin_dir.path(), "t.txt", "resolved\n");
    git_ok(twin_dir.path(), &["add", "t.txt"]);
    git_ok(
        twin_dir.path(),
        &["-c", "core.editor=true", "cherry-pick", "--continue"],
    );

    let run = git_core::pick::cherry_pick_many(&repo, &[topic], HookOptions::default());
    assert!(matches!(
        run.outcome,
        Ok(SequenceOutcome::Conflicted { .. })
    ));
    write_file(dir.path(), "t.txt", "resolved\n");
    git_ok(dir.path(), &["add", "t.txt"]);
    let run = git_core::pick::continue_sequence(&repo, run.state.unwrap(), HookOptions::default());

    assert!(matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })));
    assert_eq!(message_of(&repo, "HEAD"), message_of(&twin_repo, "HEAD"));
    assert_eq!(message_of(&repo, "HEAD"), utf8(LATIN1_AS_UTF8));
}

#[test]
fn a_conflicted_cherry_pick_offers_its_latin1_message_in_utf8() {
    let (_dir, repo, topic) = conflicting_latin1_pick();

    let outcome = git_core::pick::cherry_pick(&repo, &topic, None, HookOptions::default()).unwrap();

    assert!(matches!(outcome, PickOutcome::Conflicted { .. }));
    let message = git_core::merge::merge_message(&repo).expect("MERGE_MSG is readable");
    assert!(
        message.starts_with(LATIN1_AS_UTF8),
        "MERGE_MSG: {message:?}"
    );
}

#[test]
fn a_revert_names_a_latin1_subject_in_utf8_as_git_does() {
    let (dir, repo) = latin1_topic();
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(twin_dir.path(), &["revert", "--no-edit", "HEAD"]);
    let head = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    let outcome = git_core::pick::revert(&repo, &head, None, HookOptions::default()).unwrap();

    assert!(matches!(outcome, PickOutcome::Committed { .. }));
    assert_eq!(message_of(&repo, "HEAD"), message_of(&twin_repo, "HEAD"));
}

#[test]
fn folding_into_the_latest_latin1_commit_keeps_its_message_in_utf8_as_git_does() {
    let (dir, repo) = latin1_topic();
    write_file(dir.path(), "t.txt", "t2\n");
    git_ok(dir.path(), &["add", "t.txt"]);
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(
        twin_dir.path(),
        &[
            "-c",
            "core.editor=true",
            "commit",
            "-q",
            "--amend",
            "--no-edit",
        ],
    );

    let (state, result) =
        git_core::fold::fold_staged_into(&repo, "HEAD", HookOptions::default()).unwrap();

    assert_eq!((state, result), (None, RebaseStepResult::Done));
    assert_eq!(message_of(&repo, "HEAD"), message_of(&twin_repo, "HEAD"));
    assert_eq!(message_of(&repo, "HEAD"), utf8(LATIN1_AS_UTF8));
}

#[test]
fn folding_into_an_older_latin1_commit_keeps_its_message_in_utf8() {
    let (dir, repo) = latin1_topic();
    let target = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    write_file(dir.path(), "later.txt", "later\n");
    commit_all(&repo, "later");
    write_file(dir.path(), "t.txt", "t2\n");
    git_ok(dir.path(), &["add", "t.txt"]);

    let (state, result) =
        git_core::fold::fold_staged_into(&repo, &target, HookOptions::default()).unwrap();

    assert_eq!((state, result), (None, RebaseStepResult::Done));
    assert_eq!(message_of(&repo, "HEAD~1"), utf8(LATIN1_AS_UTF8));
}
