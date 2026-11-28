//! Author and committer names stored in another encoding (#1214). A commit with an `encoding`
//! header (`i18n.commitEncoding=ISO-8859-1`, a repository converted from SVN or CVS) stores its
//! author and committer lines in that encoding too. git reads them re-encoded to UTF-8
//! (`logmsg_reencode` re-encodes the whole commit, headers included) wherever it shows them or
//! writes a new commit from them, and Browsitory does the same. Each case is compared with git.

mod common;

use std::path::Path;

use common::{commit_all, git_ok, init_repo, twin, write_file};
use git_core::hooks::HookOptions;
use git_core::pick::PickOutcome;
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};

/// "Jürgen Müller" and "jürgen@example.com" in ISO-8859-1.
const NAME_LATIN1: &[u8] = b"J\xfcrgen M\xfcller";
const EMAIL_LATIN1: &[u8] = b"j\xfcrgen@example.com";
const NAME: &str = "Jürgen Müller";
const EMAIL: &str = "jürgen@example.com";

/// Commits everything with `git commit` as `name` <`email`> (author and committer), under
/// `i18n.commitEncoding=ISO-8859-1`, so both lines and the `encoding` header hold Latin-1.
fn git_commit_as(dir: &Path, name: &[u8], email: &[u8], message: &str) {
    let identity = dir.join(".git").join("latin1-identity");
    let mut config = b"[user]\n\tname = ".to_vec();
    config.extend_from_slice(name);
    config.extend_from_slice(b"\n\temail = ");
    config.extend_from_slice(email);
    config.push(b'\n');
    std::fs::write(&identity, config).unwrap();
    git_ok(dir, &["add", "-A"]);
    git_ok(
        dir,
        &[
            "-c",
            "i18n.commitEncoding=ISO-8859-1",
            "-c",
            &format!("include.path={}", identity.to_str().unwrap()),
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

/// `master` with a base commit and one more, `topic` from the base with Jürgen's commit adding
/// `t.txt`; `topic` is checked out.
fn jurgen_topic() -> (tempfile::TempDir, git2::Repository) {
    topic_by(NAME_LATIN1, EMAIL_LATIN1)
}

/// [`jurgen_topic`] with `topic`'s commit by `name` <`email`>, in Latin-1.
fn topic_by(name: &[u8], email: &[u8]) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "base\n");
    commit_all(&repo, "base");
    git_ok(dir.path(), &["branch", "-M", "master"]);
    git_ok(dir.path(), &["checkout", "-q", "-b", "topic"]);
    write_file(dir.path(), "t.txt", "t\n");
    git_commit_as(dir.path(), name, email, "add t");
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    write_file(dir.path(), "m.txt", "m\n");
    commit_all(&repo, "master moves on");
    git_ok(dir.path(), &["checkout", "-q", "topic"]);
    {
        let topic = repo
            .revparse_single("topic")
            .unwrap()
            .peel_to_commit()
            .unwrap();
        assert_eq!(topic.author().name_bytes(), name);
        assert_eq!(topic.message_encoding().unwrap(), Some("ISO-8859-1"));
    }
    (dir, repo)
}

/// What git prints for `args` in `dir`, as UTF-8.
fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).expect("git's output is UTF-8")
}

/// git's `%an <%ae>` (or another `--format`) of `rev`, without the newline.
fn git_format(dir: &Path, format: &str, rev: &str) -> String {
    git_out(dir, &["log", "-1", &format!("--format={format}"), rev])
        .trim_end()
        .to_string()
}

fn id_of(repo: &git2::Repository, rev: &str) -> String {
    repo.revparse_single(rev).unwrap().id().to_string()
}

/// The author name and email bytes of the commit `rev` names.
fn author_of(repo: &git2::Repository, rev: &str) -> (Vec<u8>, Vec<u8>) {
    let commit = repo.revparse_single(rev).unwrap().peel_to_commit().unwrap();
    let author = commit.author();
    (author.name_bytes().to_vec(), author.email_bytes().to_vec())
}

fn utf8_author() -> (Vec<u8>, Vec<u8>) {
    (NAME.as_bytes().to_vec(), EMAIL.as_bytes().to_vec())
}

fn all_local() -> git_core::graph::GraphSelection {
    git_core::graph::GraphSelection {
        local: None,
        remote: Vec::new(),
        first_parent: false,
    }
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

// Shown as git shows it.

#[test]
fn the_graph_shows_a_latin1_author_as_git_log_does() {
    let (dir, repo) = jurgen_topic();
    let topic = id_of(&repo, "topic");

    let graph = git_core::graph::graph_log(&repo, 50, &all_local()).unwrap();

    let row = graph.iter().find(|commit| commit.id == topic).unwrap();
    assert_eq!(row.author_name, git_format(dir.path(), "%an", "topic"));
    assert_eq!(row.author_email, git_format(dir.path(), "%ae", "topic"));
    assert_eq!(
        (row.author_name.as_str(), row.author_email.as_str()),
        (NAME, EMAIL)
    );
}

#[test]
fn the_rebase_planner_shows_a_latin1_author_as_git_log_does() {
    let (dir, repo) = jurgen_topic();

    let commits = git_core::rebase::commits_since(&repo, "master").unwrap();

    let authors: Vec<&str> = commits.iter().map(|c| c.author_name.as_str()).collect();
    assert_eq!(authors, [git_format(dir.path(), "%an", "topic")]);
    assert_eq!(authors, [NAME]);
}

#[test]
fn the_commit_details_show_a_latin1_author_and_committer_as_git_log_does() {
    let (dir, repo) = jurgen_topic();

    let details = git_core::commit_details::commit_details(
        &repo,
        &id_of(&repo, "topic"),
        std::time::Duration::from_secs(10),
    )
    .unwrap();

    assert_eq!(
        format!("{} <{}>", details.author.name, details.author.email),
        git_format(dir.path(), "%an <%ae>", "topic")
    );
    assert_eq!(
        format!("{} <{}>", details.committer.name, details.committer.email),
        git_format(dir.path(), "%cn <%ce>", "topic")
    );
    assert_eq!(details.author.name, NAME);
    assert_eq!(details.committer.email, EMAIL);
}

#[test]
fn blame_shows_a_latin1_author_as_git_blame_does() {
    let (dir, repo) = jurgen_topic();
    let porcelain = git_out(
        dir.path(),
        &["blame", "--porcelain", "topic", "--", "t.txt"],
    );
    let git_author = porcelain
        .lines()
        .find_map(|line| line.strip_prefix("author "))
        .unwrap();

    let lines = git_core::blame::blame_file(&repo, &id_of(&repo, "topic"), "t.txt").unwrap();

    assert_eq!(lines[0].author_name, git_author);
    assert_eq!(lines[0].author_name, NAME);
}

#[test]
fn the_reflog_reads_a_latin1_committer() {
    let (dir, repo) = jurgen_topic();
    // git writes the reflog's committer as configured: Latin-1 bytes, no encoding to go by.
    let raw = test_support::git(
        dir.path(),
        &["log", "-g", "-1", "--format=%gn <%ge>", "topic"],
    );
    assert_eq!(raw.stdout, b"J\xfcrgen M\xfcller <j\xfcrgen@example.com>\n");

    let entries = git_core::reflog::read_reflog(&repo, "refs/heads/topic").unwrap();

    assert_eq!(
        (
            entries[0].committer_name.as_str(),
            entries[0].committer_email.as_str()
        ),
        (NAME, EMAIL)
    );
}

#[test]
fn recent_authors_list_a_latin1_author() {
    let (_dir, repo) = jurgen_topic();

    let authors = git_core::commit::recent_authors(&repo, 10).unwrap();

    assert_eq!(
        authors,
        [git_core::commit::SignatureInput {
            name: NAME.to_string(),
            email: EMAIL.to_string(),
        }]
    );
}

#[test]
fn an_author_search_finds_a_latin1_author_as_git_log_does() {
    let (dir, repo) = jurgen_topic();
    let git_ids = git_out(
        dir.path(),
        &["log", "--all", "--author=Müller", "--format=%H"],
    );

    let page = git_core::search::search_commits(
        &repo,
        &git_core::search::parse_query("author:Müller").unwrap(),
        &all_local(),
        None,
        git_core::search::PageLimits {
            time: std::time::Duration::MAX,
            ..git_core::search::PageLimits::DEFAULT
        },
        &|| false,
    )
    .unwrap();

    let ids: Vec<String> = page.commits.iter().map(|c| c.id.clone()).collect();
    assert_eq!(ids, git_ids.lines().collect::<Vec<_>>());
    assert_eq!(ids, [id_of(&repo, "topic")]);
}

#[test]
fn a_squash_merge_names_a_latin1_author_as_git_does() {
    let (dir, repo) = jurgen_topic();
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(twin_dir.path(), &["merge", "-q", "--squash", "topic"]);
    let git_message = std::fs::read_to_string(twin_repo.path().join("SQUASH_MSG")).unwrap();
    let without_dates: String = git_message
        .split_inclusive('\n')
        .filter(|line| !line.starts_with("Date:"))
        .collect();

    git_core::merge::start_merge(
        &repo,
        "topic",
        git_core::merge::MergeOptions {
            fast_forward: git_core::merge::FastForward::Allow,
            squash: true,
        },
    )
    .unwrap();

    let message = git_core::merge::squash_message(&repo).unwrap();
    assert_eq!(message, without_dates);
    assert!(message.contains(&format!("\nAuthor: {NAME} <{EMAIL}>\n")));
}

#[test]
fn a_patch_of_a_latin1_authored_commit_names_the_author_in_utf8() {
    let (_dir, repo) = jurgen_topic();

    let patch = git_core::patch::commits_patch(&repo, &[id_of(&repo, "topic")]).unwrap();

    let text = String::from_utf8(patch).expect("the patch is UTF-8");
    assert!(
        text.contains(&format!("\nFrom: {NAME} <{EMAIL}>\n")),
        "{text}"
    );
}

// Written as git writes it: a commit made from one keeps its author, re-encoded to UTF-8.

#[test]
fn a_rebase_replays_a_latin1_author_in_utf8_as_git_does() {
    let (dir, repo) = jurgen_topic();
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(twin_dir.path(), &["rebase", "-q", "master"]);

    let plan = picks(&repo, "master");
    let (_, result) = git_core::rebase::start_rebase(&repo, "master", plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(author_of(&repo, "topic"), author_of(&twin_repo, "topic"));
    assert_eq!(author_of(&repo, "topic"), utf8_author());
}

#[test]
fn a_fixup_group_keeps_its_latin1_leaders_author_in_utf8() {
    let (dir, repo) = jurgen_topic();
    write_file(dir.path(), "t.txt", "t2\n");
    commit_all(&repo, "fix t");
    let mut plan = picks(&repo, "master");
    plan[1].action = RebaseAction::Fixup;

    let (_, result) = git_core::rebase::start_rebase(&repo, "master", plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(author_of(&repo, "topic"), utf8_author());
}

#[test]
fn a_conflicted_rebase_step_by_a_latin1_author_committed_with_git_is_adopted() {
    let (dir, repo) = jurgen_topic();
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
    // git takes the author from CHERRY_PICK_HEAD, re-encoded.
    git_ok(dir.path(), &["-c", "core.editor=true", "commit", "-q"]);
    let committed = id_of(&repo, "HEAD");
    assert_eq!(author_of(&repo, "HEAD"), utf8_author());

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(id_of(&repo, "topic"), committed);
}

#[test]
fn a_cherry_pick_writes_a_latin1_author_in_utf8_as_git_does() {
    let (dir, repo) = jurgen_topic();
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(twin_dir.path(), &["cherry-pick", "topic"]);
    let topic = id_of(&repo, "topic");

    let outcome = git_core::pick::cherry_pick(&repo, &topic, None, HookOptions::default()).unwrap();

    assert!(matches!(outcome, PickOutcome::Committed { .. }));
    assert_eq!(author_of(&repo, "HEAD"), author_of(&twin_repo, "HEAD"));
    assert_eq!(author_of(&repo, "HEAD"), utf8_author());
}

#[test]
fn a_cherry_pick_writes_an_author_with_only_a_latin1_name_in_utf8_as_git_does() {
    a_cherry_pick_writes_the_author_as_git_does(NAME_LATIN1, b"ju@example.com");
}

#[test]
fn a_cherry_pick_writes_an_author_with_only_a_latin1_email_in_utf8_as_git_does() {
    a_cherry_pick_writes_the_author_as_git_does(b"Juergen", EMAIL_LATIN1);
}

/// `topic`'s commit by `name` <`email`> (Latin-1), picked onto master by Browsitory and by git.
fn a_cherry_pick_writes_the_author_as_git_does(name: &[u8], email: &[u8]) {
    let (dir, repo) = topic_by(name, email);
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(twin_dir.path(), &["cherry-pick", "topic"]);
    let topic = id_of(&repo, "topic");

    let outcome = git_core::pick::cherry_pick(&repo, &topic, None, HookOptions::default()).unwrap();

    assert!(matches!(outcome, PickOutcome::Committed { .. }));
    assert_eq!(author_of(&repo, "HEAD"), author_of(&twin_repo, "HEAD"));
    let as_utf8 = |latin1: &[u8]| -> Vec<u8> {
        let text: String = latin1.iter().map(|&byte| char::from(byte)).collect();
        text.into_bytes()
    };
    assert_eq!(author_of(&repo, "HEAD"), (as_utf8(name), as_utf8(email)));
}

#[test]
fn a_cherry_pick_sequence_continued_after_a_conflict_writes_a_latin1_author_as_git_does() {
    let (dir, repo) = jurgen_topic();
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    write_file(dir.path(), "t.txt", "master's t\n");
    commit_all(&repo, "master adds t");
    let topic = id_of(&repo, "topic");
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
    assert!(run.outcome.is_ok());
    write_file(dir.path(), "t.txt", "resolved\n");
    git_ok(dir.path(), &["add", "t.txt"]);
    let run = git_core::pick::continue_sequence(&repo, run.state.unwrap(), HookOptions::default());

    assert!(run.outcome.is_ok());
    assert_eq!(author_of(&repo, "HEAD"), author_of(&twin_repo, "HEAD"));
    assert_eq!(author_of(&repo, "HEAD"), utf8_author());
}

#[test]
fn committing_a_conflicted_cherry_pick_writes_its_latin1_author_as_git_commit_does() {
    let (dir, mut repo) = jurgen_topic();
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    write_file(dir.path(), "t.txt", "master's t\n");
    commit_all(&repo, "master adds t");
    let topic = id_of(&repo, "topic");
    let outcome = git_core::pick::cherry_pick(&repo, &topic, None, HookOptions::default()).unwrap();
    assert!(matches!(outcome, PickOutcome::Conflicted { .. }));
    write_file(dir.path(), "t.txt", "resolved\n");
    git_ok(dir.path(), &["add", "t.txt"]);
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(twin_dir.path(), &["commit", "-q", "-m", "add t"]);

    git_core::commit::commit(&mut repo, "add t", HookOptions::default()).unwrap();

    assert_eq!(author_of(&repo, "HEAD"), author_of(&twin_repo, "HEAD"));
    assert_eq!(author_of(&repo, "HEAD"), utf8_author());
}

#[test]
fn folding_into_a_latin1_authored_head_keeps_its_author_in_utf8_as_git_does() {
    let (dir, repo) = jurgen_topic();
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
    assert_eq!(author_of(&repo, "HEAD"), author_of(&twin_repo, "HEAD"));
    assert_eq!(author_of(&repo, "HEAD"), utf8_author());
}

#[test]
fn amending_a_latin1_authored_head_keeps_its_author_in_utf8_as_git_does() {
    let (dir, repo) = jurgen_topic();
    let (twin_dir, twin_repo) = twin(dir.path());
    git_ok(
        twin_dir.path(),
        &["commit", "-q", "--amend", "-m", "add t again"],
    );

    git_core::commit::amend_head(&repo, "add t again", None, HookOptions::default()).unwrap();

    assert_eq!(author_of(&repo, "HEAD"), author_of(&twin_repo, "HEAD"));
    assert_eq!(author_of(&repo, "HEAD"), utf8_author());
}
