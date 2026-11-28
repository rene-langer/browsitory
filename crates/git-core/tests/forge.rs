mod common;

use git_core::forge::{detect_forge_repositories, ForgeProvider, ForgeRepository};

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

/// The identity of a remote whose https URL holds a user name: host, owner and name only, the
/// user name neither kept nor shown (#1101).
fn detects_without_the_user_name(url: &str, provider: ForgeProvider, host: &str) {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", url);

    let repositories = detect_forge_repositories(&repo).expect("detect forge repositories");

    assert_eq!(
        repositories,
        vec![ForgeRepository {
            provider,
            host: host.to_string(),
            owner: "ws".to_string(),
            name: "r".to_string(),
            remote_name: "origin".to_string(),
        }]
    );
    assert!(!format!("{repositories:?}").contains("alice"));
}

#[test]
fn detects_a_bitbucket_https_remote_with_a_user_name() {
    // Bitbucket's copy URL, as `git clone` stores it.
    detects_without_the_user_name(
        "https://alice@bitbucket.org/ws/r.git",
        ForgeProvider::Bitbucket,
        "bitbucket.org",
    );
}

#[test]
fn detects_a_github_https_remote_with_a_user_name() {
    detects_without_the_user_name(
        "https://alice@github.com/ws/r.git",
        ForgeProvider::GitHub,
        "github.com",
    );
}

#[test]
fn detects_an_http_remote_with_a_user_name_and_an_upper_case_host() {
    detects_without_the_user_name(
        "http://alice@GitHub.com/ws/r",
        ForgeProvider::GitHub,
        "github.com",
    );
}

#[test]
fn excludes_a_remote_whose_user_name_is_a_github_token() {
    for token in [
        "ghp_abcd1234",
        "gho_abcd1234",
        "ghu_abcd1234",
        "ghs_abcd1234",
        "ghr_abcd1234",
        "github_pat_11ABCD_efgh",
    ] {
        let (_dir, repo) = common::init_repo();
        add_remote(
            &repo,
            "origin",
            &format!("https://{token}@github.com/acme/widget.git"),
        );

        let repositories = detect_forge_repositories(&repo).expect("detect forge repositories");

        assert!(repositories.is_empty(), "{token}: {repositories:?}");
    }
}

#[test]
fn excludes_a_remote_with_an_empty_user_name_and_a_password() {
    let (_dir, repo) = common::init_repo();
    add_remote(
        &repo,
        "origin",
        "https://:secret@github.com/acme/widget.git",
    );

    let repositories = detect_forge_repositories(&repo).expect("detect forge repositories");

    assert!(repositories.is_empty());
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
fn excludes_a_remote_with_embedded_username_and_password() {
    let (_dir, repo) = common::init_repo();
    add_remote(
        &repo,
        "origin",
        "https://alice:secret-token@github.com/acme/widget.git",
    );

    let repositories =
        detect_forge_repositories(&repo).expect("a bad remote must not abort detection");

    assert!(repositories.is_empty());
}

#[test]
fn excludes_a_remote_with_bare_username_token_and_no_password() {
    let (_dir, repo) = common::init_repo();
    add_remote(
        &repo,
        "origin",
        "https://ghp_abcd1234@github.com/acme/widget.git",
    );

    let repositories =
        detect_forge_repositories(&repo).expect("a bad remote must not abort detection");

    assert!(repositories.is_empty());
}

#[test]
fn excludes_an_ssh_style_remote_with_embedded_username_and_password() {
    let (_dir, repo) = common::init_repo();
    add_remote(
        &repo,
        "origin",
        "alice:secret-token@github.com:acme/widget.git",
    );

    let repositories =
        detect_forge_repositories(&repo).expect("a bad remote must not abort detection");

    assert!(repositories.is_empty());
}

#[test]
fn excludes_a_supported_host_url_with_too_few_path_segments() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "https://github.com/acme.git");

    let repositories =
        detect_forge_repositories(&repo).expect("an ambiguous remote must not abort detection");

    assert!(repositories.is_empty());
}

#[test]
fn excludes_a_supported_host_url_with_too_many_path_segments() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "https://github.com/acme/widget/extra.git");

    let repositories =
        detect_forge_repositories(&repo).expect("an ambiguous remote must not abort detection");

    assert!(repositories.is_empty());
}

#[test]
fn a_bad_remote_does_not_hide_a_good_remote_on_the_same_repository() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "https://github.com/acme/widget.git");
    add_remote(
        &repo,
        "broken",
        "https://alice:secret-token@github.com/acme/other.git",
    );
    add_remote(
        &repo,
        "ambiguous",
        "https://github.com/acme/widget/extra.git",
    );

    let repositories = detect_forge_repositories(&repo)
        .expect("one bad remote among several must not abort detection");

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
fn returns_no_repositories_for_a_remote_less_repo() {
    let (_dir, repo) = common::init_repo();

    let repositories = detect_forge_repositories(&repo).expect("detect forge repositories");

    assert!(repositories.is_empty());
}
