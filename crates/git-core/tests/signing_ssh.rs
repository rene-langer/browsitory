mod common;

use common::{
    commit_all, enable_ssh_signing, init_repo, program_available, ssh_key_fixture, write_file,
};
use git_core::signing::{create_commit, HeadUpdate};

macro_rules! require {
    ($program:expr) => {
        if !program_available($program) {
            eprintln!("skipping: {} not on PATH", $program);
            return;
        }
    };
}

fn is_ssh_signed(repo: &git2::Repository, oid: git2::Oid) -> bool {
    match repo.extract_signature(&oid, None) {
        Ok((signature, _)) => std::str::from_utf8(&signature)
            .unwrap()
            .starts_with("-----BEGIN SSH SIGNATURE-----"),
        Err(_) => false,
    }
}

#[test]
fn create_commit_signs_with_an_ssh_key_file() {
    require!("ssh-keygen");
    let (dir, repo) = init_repo();
    let keys = tempfile::TempDir::new().unwrap();
    let key = ssh_key_fixture(keys.path());
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    enable_ssh_signing(&repo, &key);
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();

    let oid = create_commit(
        &repo,
        Some(HeadUpdate {
            expected: Some(base.id()),
            reflog_operation: "commit",
        }),
        &sig,
        &sig,
        "signed one",
        &base.tree().unwrap(),
        &[&base],
    )
    .unwrap();

    assert!(is_ssh_signed(&repo, oid));
    assert_eq!(repo.head().unwrap().target(), Some(oid));
    let reflog = repo.reflog("HEAD").unwrap();
    assert_eq!(
        reflog.get(0).unwrap().message().unwrap(),
        Some("commit: signed one")
    );
}

#[test]
fn signed_commit_on_a_detached_head_moves_head_only() {
    require!("ssh-keygen");
    let (dir, repo) = init_repo();
    let keys = tempfile::TempDir::new().unwrap();
    let key = ssh_key_fixture(keys.path());
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let branch = repo.head().unwrap().name().unwrap().to_string();
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.set_head_detached(base.id()).unwrap();
    enable_ssh_signing(&repo, &key);
    let sig = repo.signature().unwrap();

    let oid = create_commit(
        &repo,
        Some(HeadUpdate {
            expected: Some(base.id()),
            reflog_operation: "commit",
        }),
        &sig,
        &sig,
        "detached",
        &base.tree().unwrap(),
        &[&base],
    )
    .unwrap();

    assert!(repo.head_detached().unwrap());
    assert_eq!(repo.head().unwrap().target(), Some(oid));
    assert_eq!(
        repo.find_reference(&branch).unwrap().target(),
        Some(base.id())
    );
    assert!(repo.find_reference("refs/heads/HEAD").is_err());
}

#[test]
fn signer_failure_leaves_head_untouched() {
    require!("ssh-keygen");
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_bool("commit.gpgSign", true).unwrap();
        config.set_str("gpg.format", "ssh").unwrap();
        config
            .set_str("user.signingKey", "/nonexistent/key")
            .unwrap();
    }
    let sig = repo.signature().unwrap();

    let err = create_commit(
        &repo,
        Some(HeadUpdate {
            expected: Some(base.id()),
            reflog_operation: "commit",
        }),
        &sig,
        &sig,
        "nope",
        &base.tree().unwrap(),
        &[&base],
    )
    .unwrap_err();

    assert!(err.to_string().starts_with("Signing failed: "), "{err}");
    assert_eq!(repo.head().unwrap().target(), Some(base.id()));
}

#[test]
fn missing_signer_program_is_reported_by_name() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_bool("commit.gpgSign", true).unwrap();
        config
            .set_str("gpg.program", "browsitory-no-such-gpg")
            .unwrap();
    }
    let sig = repo.signature().unwrap();
    let err = create_commit(
        &repo,
        None,
        &sig,
        &sig,
        "x",
        &base.tree().unwrap(),
        &[&base],
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Signing program 'browsitory-no-such-gpg' was not found."
    );
}

#[test]
fn signs_with_a_literal_ssh_public_key_through_the_agent() {
    require!("ssh-keygen");
    require!("ssh-agent");
    require!("ssh-add");
    let (dir, repo) = init_repo();
    let keys = tempfile::TempDir::new().unwrap();
    let key = ssh_key_fixture(keys.path());
    // Private socket for a throwaway agent, killed at the end of the test.
    let socket = keys.path().join("agent.sock");
    let mut agent = std::process::Command::new("ssh-agent")
        .args(["-D", "-a"])
        .arg(&socket)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..50 {
        if socket.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let added = std::process::Command::new("ssh-add")
        .env("SSH_AUTH_SOCK", &socket)
        .arg(&key.private)
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(added.success());
    // ssh-keygen finds the agent through SSH_AUTH_SOCK; this test binary sets it only here
    // and no other test in this file reads it.
    std::env::set_var("SSH_AUTH_SOCK", &socket);

    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    {
        let mut config = repo.config().unwrap();
        config.set_bool("commit.gpgSign", true).unwrap();
        config.set_str("gpg.format", "ssh").unwrap();
        config
            .set_str("user.signingKey", &format!("key::{}", key.public_line))
            .unwrap();
    }
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();
    let result = create_commit(
        &repo,
        None,
        &sig,
        &sig,
        "agent",
        &base.tree().unwrap(),
        &[&base],
    );
    let _ = agent.kill();
    let _ = agent.wait();

    assert!(is_ssh_signed(&repo, result.unwrap()));
}

use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};

fn signing_repo() -> Option<(tempfile::TempDir, tempfile::TempDir, git2::Repository)> {
    if !program_available("ssh-keygen") {
        eprintln!("skipping: ssh-keygen not on PATH");
        return None;
    }
    let (dir, repo) = init_repo();
    let keys = tempfile::TempDir::new().unwrap();
    let key = ssh_key_fixture(keys.path());
    write_file(dir.path(), "base.txt", "base\n");
    commit_all(&repo, "base");
    enable_ssh_signing(&repo, &key);
    Some((dir, keys, repo))
}

fn head_oid(repo: &git2::Repository) -> git2::Oid {
    repo.head().unwrap().target().unwrap()
}

fn stage(repo: &git2::Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(path)).unwrap();
    index.write().unwrap();
}

#[test]
fn commit_and_amend_are_signed_with_unchanged_reflog() {
    let Some((dir, _keys, mut repo)) = signing_repo() else {
        return;
    };
    write_file(dir.path(), "a.txt", "a\n");
    stage(&repo, "a.txt");
    let oid = git_core::commit::commit(&mut repo, "add a").unwrap();
    let oid = git2::Oid::from_str(&oid).unwrap();
    assert!(is_ssh_signed(&repo, oid));
    assert_eq!(
        repo.reflog("HEAD")
            .unwrap()
            .get(0)
            .unwrap()
            .message()
            .ok()
            .flatten(),
        Some("commit: add a")
    );

    let amended = git_core::commit::amend_head(&repo, "add a, amended").unwrap();
    let amended = git2::Oid::from_str(&amended).unwrap();
    assert!(is_ssh_signed(&repo, amended));
    assert_eq!(head_oid(&repo), amended);
    let original = repo.find_commit(oid).unwrap();
    let amended_commit = repo.find_commit(amended).unwrap();
    assert_eq!(
        amended_commit.parent_id(0).unwrap(),
        original.parent_id(0).unwrap()
    );
    assert_eq!(amended_commit.author().name(), original.author().name());
    assert_eq!(
        repo.reflog("HEAD")
            .unwrap()
            .get(0)
            .unwrap()
            .message()
            .ok()
            .flatten(),
        Some("commit (amend): add a, amended")
    );
}

#[test]
fn cherry_pick_and_revert_are_signed() {
    let Some((dir, _keys, mut repo)) = signing_repo() else {
        return;
    };
    let base = head_oid(&repo);
    write_file(dir.path(), "a.txt", "a\n");
    stage(&repo, "a.txt");
    let picked = git_core::commit::commit(&mut repo, "add a").unwrap();

    let reverted = match git_core::pick::revert(&repo, &picked).unwrap() {
        git_core::pick::PickOutcome::Committed { commit_id } => commit_id,
        other => panic!("unexpected {other:?}"),
    };
    assert!(is_ssh_signed(
        &repo,
        git2::Oid::from_str(&reverted).unwrap()
    ));

    // Back to base on a detached HEAD, then cherry-pick "add a" onto it.
    repo.set_head_detached(base).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    let cherry = match git_core::pick::cherry_pick(&repo, &picked).unwrap() {
        git_core::pick::PickOutcome::Committed { commit_id } => commit_id,
        other => panic!("unexpected {other:?}"),
    };
    assert!(is_ssh_signed(&repo, git2::Oid::from_str(&cherry).unwrap()));
}

#[test]
fn rebase_steps_and_squash_collapse_are_signed() {
    let Some((dir, _keys, mut repo)) = signing_repo() else {
        return;
    };
    let onto = head_oid(&repo).to_string();
    let mut ids = Vec::new();
    for name in ["a", "b", "c"] {
        write_file(dir.path(), &format!("{name}.txt"), "x\n");
        stage(&repo, &format!("{name}.txt"));
        ids.push(git_core::commit::commit(&mut repo, &format!("add {name}")).unwrap());
    }
    let plan = vec![
        RebasePlanEntry {
            commit_id: ids[0].clone(),
            action: RebaseAction::Pick,
            combined_message: None,
        },
        RebasePlanEntry {
            commit_id: ids[1].clone(),
            action: RebaseAction::Squash,
            combined_message: None,
        },
        RebasePlanEntry {
            commit_id: ids[2].clone(),
            action: RebaseAction::Reword {
                message: "c reworded".into(),
            },
            combined_message: None,
        },
    ];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert_eq!(result, RebaseStepResult::Done);

    let tip = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(tip.summary().ok().flatten(), Some("c reworded"));
    assert!(is_ssh_signed(&repo, tip.id()));
    let collapsed = tip.parent(0).unwrap();
    assert_eq!(collapsed.summary().ok().flatten(), Some("add a"));
    assert!(is_ssh_signed(&repo, collapsed.id()));
}

#[test]
fn failed_signing_during_a_merge_keeps_the_merge_in_progress() {
    let Some((dir, _keys, mut repo)) = signing_repo() else {
        return;
    };
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &base, false).unwrap();
    }
    write_file(dir.path(), "main.txt", "m\n");
    stage(&repo, "main.txt");
    git_core::commit::commit(&mut repo, "on main").unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "feature.txt", "f\n");
    stage(&repo, "feature.txt");
    git_core::commit::commit(&mut repo, "on feature").unwrap();
    repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();

    let outcome = git_core::merge::start_merge(&repo, "feature").unwrap();
    assert_eq!(outcome, git_core::merge::MergeOutcome::Merged);
    let before = head_oid(&repo);
    repo.config()
        .unwrap()
        .set_str("user.signingKey", "/nonexistent/key")
        .unwrap();

    let err = git_core::commit::commit(&mut repo, "merge feature").unwrap_err();

    assert!(err.to_string().starts_with("Signing failed: "), "{err}");
    assert_eq!(head_oid(&repo), before);
    assert_eq!(repo.state(), git2::RepositoryState::Merge);
}

#[test]
fn signing_off_by_default_keeps_commits_unsigned() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    stage(&repo, "a.txt");
    let oid = git_core::commit::commit(&mut repo, "plain").unwrap();
    assert!(!is_ssh_signed(&repo, git2::Oid::from_str(&oid).unwrap()));
    assert!(repo
        .extract_signature(&git2::Oid::from_str(&oid).unwrap(), None)
        .is_err());
}
