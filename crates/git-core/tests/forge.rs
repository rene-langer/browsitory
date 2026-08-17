mod common;

use git_core::forge::{detect_forge_repositories, ForgeError, ForgeProvider, ForgeRepository};

fn add_remote(repo: &git2::Repository, name: &str, url: &str) {
    // Bypass git_core::remote::add_remote's own credential validation here: these tests
    // need a credential-bearing URL to actually land on the remote so forge::detect can be
    // proven to reject it itself.
    repo.remote(name, url).expect("add remote");
}

#[test]
fn detects_a_github_https_remote() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "https://github.com/acme/widget.git");

    let repositories = detect_forge_repositories(&repo).expect("detect forge repositories");

    assert_eq!(
        repositories,
        vec![ForgeRepository {
            provider: ForgeProvider::GitHub,
            host: "github.com".to_string(),
            owner: "acme".to_string(),
            name: "widget".to_string(),
            remote_name: "origin".to_string(),
        }]
    );
}

#[test]
fn detects_a_github_ssh_remote() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "git@github.com:acme/widget.git");

    let repositories = detect_forge_repositories(&repo).expect("detect forge repositories");

    assert_eq!(
        repositories,
        vec![ForgeRepository {
            provider: ForgeProvider::GitHub,
            host: "github.com".to_string(),
            owner: "acme".to_string(),
            name: "widget".to_string(),
            remote_name: "origin".to_string(),
        }]
    );
}

#[test]
fn detects_a_bitbucket_https_remote() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "https://bitbucket.org/team/widget.git");

    let repositories = detect_forge_repositories(&repo).expect("detect forge repositories");

    assert_eq!(
        repositories,
        vec![ForgeRepository {
            provider: ForgeProvider::Bitbucket,
            host: "bitbucket.org".to_string(),
            owner: "team".to_string(),
            name: "widget".to_string(),
            remote_name: "origin".to_string(),
        }]
    );
}

#[test]
fn does_not_classify_an_unsupported_host() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "https://gitlab.com/acme/widget.git");

    let repositories = detect_forge_repositories(&repo).expect("detect forge repositories");

    assert!(repositories.is_empty());
}

#[test]
fn does_not_classify_a_malformed_url() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "not a url at all");

    let repositories = detect_forge_repositories(&repo).expect("detect forge repositories");

    assert!(repositories.is_empty());
}

#[test]
fn detects_two_remotes_resolving_to_different_supported_repositories() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "https://github.com/acme/widget.git");
    add_remote(&repo, "upstream", "https://bitbucket.org/team/widget.git");

    let mut repositories = detect_forge_repositories(&repo).expect("detect forge repositories");
    repositories.sort_by(|a, b| a.remote_name.cmp(&b.remote_name));

    assert_eq!(
        repositories,
        vec![
            ForgeRepository {
                provider: ForgeProvider::GitHub,
                host: "github.com".to_string(),
                owner: "acme".to_string(),
                name: "widget".to_string(),
                remote_name: "origin".to_string(),
            },
            ForgeRepository {
                provider: ForgeProvider::Bitbucket,
                host: "bitbucket.org".to_string(),
                owner: "team".to_string(),
                name: "widget".to_string(),
                remote_name: "upstream".to_string(),
            },
        ]
    );
}

#[test]
fn rejects_a_remote_with_embedded_username_and_password() {
    let (_dir, repo) = common::init_repo();
    add_remote(
        &repo,
        "origin",
        "https://alice:secret-token@github.com/acme/widget.git",
    );

    let error = detect_forge_repositories(&repo).expect_err("expected credential rejection");

    assert!(matches!(error, ForgeError::CredentialBearingUrl));
    let message = error.to_string();
    assert!(!message.contains("secret-token"));
    assert!(!message.contains("alice"));
    assert!(!message.contains("github.com"));
}

#[test]
fn rejects_an_ssh_style_remote_with_embedded_username_and_password() {
    let (_dir, repo) = common::init_repo();
    add_remote(
        &repo,
        "origin",
        "alice:secret-token@github.com:acme/widget.git",
    );

    let error = detect_forge_repositories(&repo).expect_err("expected credential rejection");

    assert!(matches!(error, ForgeError::CredentialBearingUrl));
    let message = error.to_string();
    assert!(!message.contains("secret-token"));
    assert!(!message.contains("alice"));
}

#[test]
fn rejects_a_supported_host_url_with_too_few_path_segments() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "https://github.com/acme.git");

    let error = detect_forge_repositories(&repo).expect_err("expected ambiguity error");

    assert!(matches!(error, ForgeError::AmbiguousRemote { .. }));
    let message = error.to_string();
    assert!(!message.contains("acme.git"));
    assert!(message.contains("origin"));
}

#[test]
fn rejects_a_supported_host_url_with_too_many_path_segments() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "https://github.com/acme/widget/extra.git");

    let error = detect_forge_repositories(&repo).expect_err("expected ambiguity error");

    assert!(matches!(error, ForgeError::AmbiguousRemote { .. }));
}

#[test]
fn returns_no_repositories_for_a_remote_less_repo() {
    let (_dir, repo) = common::init_repo();

    let repositories = detect_forge_repositories(&repo).expect("detect forge repositories");

    assert!(repositories.is_empty());
}
