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
