//! Provider-neutral pull-request listing/creation for GitHub and Bitbucket Cloud.
//!
//! This module owns all forge HTTP traffic. `git-core` stays a pure local-git library with no
//! HTTP; `crates/git-core/src/forge.rs` only classifies a repo's remotes into a
//! `ForgeRepository` (provider/host/owner/name). Nothing here ever returns an API token from a
//! public DTO, logs a token, or embeds one in an error message — the OS keychain lookup that
//! supplies a token lives in `credentials.rs`'s dedicated `forge:<provider>:<account>`
//! namespace, kept separate from Git HTTPS transport credentials.
//!
//! HTTP access is behind the `ForgeApi` trait so tests can supply deterministic canned
//! responses (or simulated transport failures) instead of hitting a live GitHub/Bitbucket
//! account or standing up a mock HTTP server.

use std::time::Duration;

use git_core::forge::{ForgeProvider, ForgeRepository};
use serde::Deserialize;

/// A pull request normalized to the same shape regardless of provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub id: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: String,
    pub source_branch: String,
    pub target_branch: String,
    pub state: String,
}

/// The fields a caller supplies to open a new pull request. Never includes a token or any
/// provider-specific field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePullRequest {
    pub title: String,
    pub description: Option<String>,
    pub source_branch: String,
    pub target_branch: String,
}

/// Errors `PullRequestService` returns. Every variant's message is safe to show verbatim in
/// the UI: none of them can contain the provider token, since the service never interpolates
/// the token, the request URL, or the raw response body into an error string (validation
/// messages are extracted from a known, narrow JSON field, never the whole body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullRequestError {
    /// No forge token was supplied for this provider/account. Never makes a request.
    MissingToken,
    /// The provider rejected the saved token (HTTP 401/403).
    Unauthorized,
    /// The provider rejected the request's content (HTTP 400/422); carries the provider's own
    /// human-readable validation message, never the raw response body.
    Validation(String),
    /// The request to the provider timed out.
    Timeout,
    /// The request to the provider failed at the transport level (DNS, TLS, connection reset).
    Transport,
    /// The provider returned a response this client doesn't know how to interpret.
    UnexpectedResponse,
}

impl std::fmt::Display for PullRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingToken => {
                formatter.write_str("a provider token is required for this action")
            }
            Self::Unauthorized => {
                formatter.write_str("the saved token was rejected by the provider")
            }
            Self::Validation(message) => {
                write!(formatter, "the provider rejected the request: {message}")
            }
            Self::Timeout => formatter.write_str("the request to the provider timed out"),
            Self::Transport => formatter.write_str("the request to the provider failed"),
            Self::UnexpectedResponse => {
                formatter.write_str("the provider returned an unexpected response")
            }
        }
    }
}

impl std::error::Error for PullRequestError {}

/// A provider-neutral HTTP request. Built by this module's GitHub/Bitbucket adapters, sent
/// through `ForgeApi`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeHttpRequest {
    pub method: ForgeHttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub json_body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeHttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeHttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeApiError {
    Timeout,
    Transport,
}

/// The HTTP transport seam. Production code uses `ReqwestForgeApi`; tests supply a fake that
/// returns canned responses/errors, so PR tests never depend on a live forge account or a mock
/// HTTP server.
pub trait ForgeApi {
    fn send(&self, request: ForgeHttpRequest) -> Result<ForgeHttpResponse, ForgeApiError>;
}

impl<T: ForgeApi + ?Sized> ForgeApi for &T {
    fn send(&self, request: ForgeHttpRequest) -> Result<ForgeHttpResponse, ForgeApiError> {
        (**self).send(request)
    }
}

/// `reqwest`'s blocking client, matching this codebase's synchronous worker-thread model (see
/// `crates/tauri-app/src/worker.rs` — one `thread::spawn`-owned thread per open repository, no
/// tokio runtime anywhere in this crate).
pub struct ReqwestForgeApi {
    client: reqwest::blocking::Client,
}

impl ReqwestForgeApi {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("the bundled rustls TLS backend must build a reqwest client");
        Self { client }
    }
}

impl Default for ReqwestForgeApi {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeApi for ReqwestForgeApi {
    fn send(&self, request: ForgeHttpRequest) -> Result<ForgeHttpResponse, ForgeApiError> {
        let mut builder = match request.method {
            ForgeHttpMethod::Get => self.client.get(&request.url),
            ForgeHttpMethod::Post => self.client.post(&request.url),
        };
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = &request.json_body {
            builder = builder.json(body);
        }
        let response = builder.send().map_err(|error| {
            if error.is_timeout() {
                ForgeApiError::Timeout
            } else {
                ForgeApiError::Transport
            }
        })?;
        let status = response.status().as_u16();
        let body = response.text().map_err(|_| ForgeApiError::Transport)?;
        Ok(ForgeHttpResponse { status, body })
    }
}

/// Provider-neutral pull-request listing/creation, generic over the HTTP transport seam.
pub struct PullRequestService<A: ForgeApi> {
    api: A,
}

impl<A: ForgeApi> PullRequestService<A> {
    pub fn new(api: A) -> Self {
        Self { api }
    }

    /// Lists open pull requests for `repository`. `token` is the already-looked-up forge token
    /// for this provider/account (see `credentials.rs::CredentialService::lookup_forge_token`);
    /// this service never looks up or caches a token itself. `None` is rejected before any
    /// request is sent.
    pub fn list_pull_requests(
        &self,
        repository: &ForgeRepository,
        token: Option<&str>,
    ) -> Result<Vec<PullRequest>, PullRequestError> {
        let token = token.ok_or(PullRequestError::MissingToken)?;
        let request = build_list_request(repository, token);
        let body = self.execute(request)?;
        parse_list_body(repository.provider, &body)
    }

    /// Creates a pull request on `repository`. Same token-injection contract as
    /// `list_pull_requests`.
    pub fn create_pull_request(
        &self,
        repository: &ForgeRepository,
        token: Option<&str>,
        create: &CreatePullRequest,
    ) -> Result<PullRequest, PullRequestError> {
        let token = token.ok_or(PullRequestError::MissingToken)?;
        let request = build_create_request(repository, token, create);
        let body = self.execute(request)?;
        parse_single_body(repository.provider, &body)
    }

    fn execute(&self, request: ForgeHttpRequest) -> Result<String, PullRequestError> {
        let response = self.api.send(request).map_err(|error| match error {
            ForgeApiError::Timeout => PullRequestError::Timeout,
            ForgeApiError::Transport => PullRequestError::Transport,
        })?;
        match response.status {
            200..=299 => Ok(response.body),
            401 | 403 => Err(PullRequestError::Unauthorized),
            400 | 422 => Err(PullRequestError::Validation(extract_validation_message(
                &response.body,
            ))),
            _ => Err(PullRequestError::UnexpectedResponse),
        }
    }
}

/// Pulls a narrow, known-safe `message` field out of a provider's error body (GitHub's
/// top-level `message`, or Bitbucket's nested `error.message`). Never echoes the raw response
/// body verbatim, so an unexpected/malformed error payload can't leak anything through this
/// path.
fn extract_validation_message(body: &str) -> String {
    const FALLBACK: &str = "the provider rejected the request";
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return FALLBACK.to_string();
    };
    value
        .get("message")
        .and_then(|message| message.as_str())
        .or_else(|| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(|message| message.as_str())
        })
        .unwrap_or(FALLBACK)
        .to_string()
}

fn build_list_request(repository: &ForgeRepository, token: &str) -> ForgeHttpRequest {
    match repository.provider {
        ForgeProvider::GitHub => ForgeHttpRequest {
            method: ForgeHttpMethod::Get,
            url: format!(
                "https://api.github.com/repos/{}/{}/pulls?state=open",
                repository.owner, repository.name
            ),
            headers: github_headers(token),
            json_body: None,
        },
        ForgeProvider::Bitbucket => ForgeHttpRequest {
            method: ForgeHttpMethod::Get,
            url: format!(
                "https://api.bitbucket.org/2.0/repositories/{}/{}/pullrequests?state=OPEN",
                repository.owner, repository.name
            ),
            headers: bitbucket_headers(token),
            json_body: None,
        },
    }
}

fn build_create_request(
    repository: &ForgeRepository,
    token: &str,
    create: &CreatePullRequest,
) -> ForgeHttpRequest {
    let description = create.description.clone().unwrap_or_default();
    match repository.provider {
        ForgeProvider::GitHub => ForgeHttpRequest {
            method: ForgeHttpMethod::Post,
            url: format!(
                "https://api.github.com/repos/{}/{}/pulls",
                repository.owner, repository.name
            ),
            headers: github_headers(token),
            json_body: Some(serde_json::json!({
                "title": create.title,
                "body": description,
                "head": create.source_branch,
                "base": create.target_branch,
            })),
        },
        ForgeProvider::Bitbucket => ForgeHttpRequest {
            method: ForgeHttpMethod::Post,
            url: format!(
                "https://api.bitbucket.org/2.0/repositories/{}/{}/pullrequests",
                repository.owner, repository.name
            ),
            headers: bitbucket_headers(token),
            json_body: Some(serde_json::json!({
                "title": create.title,
                "description": description,
                "source": { "branch": { "name": create.source_branch } },
                "destination": { "branch": { "name": create.target_branch } },
            })),
        },
    }
}

// GitHub's documented token authentication for the REST API.
fn github_headers(token: &str) -> Vec<(String, String)> {
    vec![
        ("Authorization".to_string(), format!("Bearer {token}")),
        (
            "Accept".to_string(),
            "application/vnd.github+json".to_string(),
        ),
        // Required by GitHub's REST API for every request, unrelated to authentication.
        ("User-Agent".to_string(), "Browsitory".to_string()),
    ]
}

// Bitbucket Cloud's documented token authentication for repository/workspace access tokens.
fn bitbucket_headers(token: &str) -> Vec<(String, String)> {
    vec![
        ("Authorization".to_string(), format!("Bearer {token}")),
        ("Accept".to_string(), "application/json".to_string()),
    ]
}

fn parse_list_body(
    provider: ForgeProvider,
    body: &str,
) -> Result<Vec<PullRequest>, PullRequestError> {
    match provider {
        ForgeProvider::GitHub => {
            let items: Vec<GitHubPullRequest> =
                serde_json::from_str(body).map_err(|_| PullRequestError::UnexpectedResponse)?;
            Ok(items.into_iter().map(PullRequest::from).collect())
        }
        ForgeProvider::Bitbucket => {
            let page: BitbucketPullRequestPage =
                serde_json::from_str(body).map_err(|_| PullRequestError::UnexpectedResponse)?;
            Ok(page.values.into_iter().map(PullRequest::from).collect())
        }
    }
}

fn parse_single_body(provider: ForgeProvider, body: &str) -> Result<PullRequest, PullRequestError> {
    match provider {
        ForgeProvider::GitHub => {
            let item: GitHubPullRequest =
                serde_json::from_str(body).map_err(|_| PullRequestError::UnexpectedResponse)?;
            Ok(item.into())
        }
        ForgeProvider::Bitbucket => {
            let item: BitbucketPullRequest =
                serde_json::from_str(body).map_err(|_| PullRequestError::UnexpectedResponse)?;
            Ok(item.into())
        }
    }
}

#[derive(Deserialize)]
struct GitHubUser {
    login: String,
}

#[derive(Deserialize)]
struct GitHubBranchRef {
    #[serde(rename = "ref")]
    name: String,
}

#[derive(Deserialize)]
struct GitHubPullRequest {
    id: u64,
    number: u64,
    title: String,
    html_url: String,
    user: GitHubUser,
    head: GitHubBranchRef,
    base: GitHubBranchRef,
    state: String,
}

impl From<GitHubPullRequest> for PullRequest {
    fn from(pull_request: GitHubPullRequest) -> Self {
        PullRequest {
            id: pull_request.id.to_string(),
            number: pull_request.number,
            title: pull_request.title,
            url: pull_request.html_url,
            author: pull_request.user.login,
            source_branch: pull_request.head.name,
            target_branch: pull_request.base.name,
            state: pull_request.state.to_lowercase(),
        }
    }
}

#[derive(Deserialize)]
struct BitbucketAuthor {
    display_name: String,
}

#[derive(Deserialize)]
struct BitbucketBranchName {
    name: String,
}

#[derive(Deserialize)]
struct BitbucketBranchRef {
    branch: BitbucketBranchName,
}

#[derive(Deserialize)]
struct BitbucketLink {
    href: String,
}

#[derive(Deserialize)]
struct BitbucketLinks {
    html: BitbucketLink,
}

#[derive(Deserialize)]
struct BitbucketPullRequest {
    id: u64,
    title: String,
    links: BitbucketLinks,
    author: BitbucketAuthor,
    source: BitbucketBranchRef,
    destination: BitbucketBranchRef,
    state: String,
}

#[derive(Deserialize)]
struct BitbucketPullRequestPage {
    values: Vec<BitbucketPullRequest>,
}

impl From<BitbucketPullRequest> for PullRequest {
    fn from(pull_request: BitbucketPullRequest) -> Self {
        PullRequest {
            id: pull_request.id.to_string(),
            number: pull_request.id,
            title: pull_request.title,
            url: pull_request.links.html.href,
            author: pull_request.author.display_name,
            source_branch: pull_request.source.branch.name,
            target_branch: pull_request.destination.branch.name,
            state: pull_request.state.to_lowercase(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;

    #[derive(Default)]
    struct FakeForgeApi {
        responses: RefCell<VecDeque<Result<ForgeHttpResponse, ForgeApiError>>>,
        requests: RefCell<Vec<ForgeHttpRequest>>,
    }

    impl FakeForgeApi {
        fn queue(responses: Vec<Result<ForgeHttpResponse, ForgeApiError>>) -> Self {
            Self {
                responses: RefCell::new(responses.into_iter().collect()),
                requests: RefCell::new(Vec::new()),
            }
        }
    }

    impl ForgeApi for FakeForgeApi {
        fn send(&self, request: ForgeHttpRequest) -> Result<ForgeHttpResponse, ForgeApiError> {
            self.requests.borrow_mut().push(request);
            self.responses
                .borrow_mut()
                .pop_front()
                .expect("test queued an unexpected extra request")
        }
    }

    fn ok(status: u16, body: &str) -> Result<ForgeHttpResponse, ForgeApiError> {
        Ok(ForgeHttpResponse {
            status,
            body: body.to_string(),
        })
    }

    fn github_repo() -> ForgeRepository {
        ForgeRepository {
            provider: ForgeProvider::GitHub,
            host: "github.com".to_string(),
            owner: "acme".to_string(),
            name: "widget".to_string(),
            remote_name: "origin".to_string(),
        }
    }

    fn bitbucket_repo() -> ForgeRepository {
        ForgeRepository {
            provider: ForgeProvider::Bitbucket,
            host: "bitbucket.org".to_string(),
            owner: "acme".to_string(),
            name: "widget".to_string(),
            remote_name: "origin".to_string(),
        }
    }

    const GITHUB_LIST_FIXTURE: &str = r#"[
        {
            "id": 101,
            "number": 7,
            "title": "Add pull request support",
            "html_url": "https://github.com/acme/widget/pull/7",
            "user": {"login": "rene"},
            "head": {"ref": "feature/pr"},
            "base": {"ref": "main"},
            "state": "open"
        }
    ]"#;

    const BITBUCKET_LIST_FIXTURE: &str = r#"{
        "values": [
            {
                "id": 12,
                "title": "Add pull request support",
                "links": {"html": {"href": "https://bitbucket.org/acme/widget/pull-requests/12"}},
                "author": {"display_name": "Rene Langer"},
                "source": {"branch": {"name": "feature/pr"}},
                "destination": {"branch": {"name": "main"}},
                "state": "OPEN"
            }
        ]
    }"#;

    #[test]
    fn listing_without_a_token_is_rejected_before_any_request_is_sent() {
        let api = FakeForgeApi::queue(vec![]);
        let service = PullRequestService::new(&api);

        let error = service
            .list_pull_requests(&github_repo(), None)
            .expect_err("a missing token must not be silently accepted");

        assert_eq!(error, PullRequestError::MissingToken);
        assert!(api.requests.borrow().is_empty());
    }

    #[test]
    fn creating_without_a_token_is_rejected_before_any_request_is_sent() {
        let api = FakeForgeApi::queue(vec![]);
        let service = PullRequestService::new(&api);
        let create = CreatePullRequest {
            title: "Add feature".to_string(),
            description: None,
            source_branch: "feature/pr".to_string(),
            target_branch: "main".to_string(),
        };

        let error = service
            .create_pull_request(&github_repo(), None, &create)
            .expect_err("a missing token must not be silently accepted");

        assert_eq!(error, PullRequestError::MissingToken);
        assert!(api.requests.borrow().is_empty());
    }

    #[test]
    fn lists_and_normalizes_github_pull_requests() {
        let api = FakeForgeApi::queue(vec![ok(200, GITHUB_LIST_FIXTURE)]);
        let service = PullRequestService::new(&api);

        let pull_requests = service
            .list_pull_requests(&github_repo(), Some("gh-token-123"))
            .unwrap();

        assert_eq!(
            pull_requests,
            vec![PullRequest {
                id: "101".to_string(),
                number: 7,
                title: "Add pull request support".to_string(),
                url: "https://github.com/acme/widget/pull/7".to_string(),
                author: "rene".to_string(),
                source_branch: "feature/pr".to_string(),
                target_branch: "main".to_string(),
                state: "open".to_string(),
            }]
        );

        let requests = api.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, ForgeHttpMethod::Get);
        assert_eq!(
            requests[0].url,
            "https://api.github.com/repos/acme/widget/pulls?state=open"
        );
        assert!(requests[0].headers.contains(&(
            "Authorization".to_string(),
            "Bearer gh-token-123".to_string()
        )));
    }

    #[test]
    fn lists_and_normalizes_bitbucket_pull_requests() {
        let api = FakeForgeApi::queue(vec![ok(200, BITBUCKET_LIST_FIXTURE)]);
        let service = PullRequestService::new(&api);

        let pull_requests = service
            .list_pull_requests(&bitbucket_repo(), Some("bb-token-456"))
            .unwrap();

        assert_eq!(
            pull_requests,
            vec![PullRequest {
                id: "12".to_string(),
                number: 12,
                title: "Add pull request support".to_string(),
                url: "https://bitbucket.org/acme/widget/pull-requests/12".to_string(),
                author: "Rene Langer".to_string(),
                source_branch: "feature/pr".to_string(),
                target_branch: "main".to_string(),
                state: "open".to_string(),
            }]
        );

        let requests = api.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, ForgeHttpMethod::Get);
        assert_eq!(
            requests[0].url,
            "https://api.bitbucket.org/2.0/repositories/acme/widget/pullrequests?state=OPEN"
        );
        assert!(requests[0].headers.contains(&(
            "Authorization".to_string(),
            "Bearer bb-token-456".to_string()
        )));
    }

    #[test]
    fn creates_a_github_pull_request_sending_title_body_and_branches() {
        const GITHUB_CREATE_FIXTURE: &str = r#"{
            "id": 202,
            "number": 8,
            "title": "Add feature",
            "html_url": "https://github.com/acme/widget/pull/8",
            "user": {"login": "rene"},
            "head": {"ref": "feature/pr"},
            "base": {"ref": "main"},
            "state": "open"
        }"#;
        let api = FakeForgeApi::queue(vec![ok(201, GITHUB_CREATE_FIXTURE)]);
        let service = PullRequestService::new(&api);
        let create = CreatePullRequest {
            title: "Add feature".to_string(),
            description: Some("Implements the thing".to_string()),
            source_branch: "feature/pr".to_string(),
            target_branch: "main".to_string(),
        };

        let pull_request = service
            .create_pull_request(&github_repo(), Some("gh-token-123"), &create)
            .unwrap();

        assert_eq!(pull_request.number, 8);
        let requests = api.requests.borrow();
        assert_eq!(requests[0].method, ForgeHttpMethod::Post);
        assert_eq!(
            requests[0].url,
            "https://api.github.com/repos/acme/widget/pulls"
        );
        assert_eq!(
            requests[0].json_body,
            Some(serde_json::json!({
                "title": "Add feature",
                "body": "Implements the thing",
                "head": "feature/pr",
                "base": "main",
            }))
        );
    }

    #[test]
    fn creates_a_bitbucket_pull_request_sending_title_description_and_branches() {
        const BITBUCKET_CREATE_FIXTURE: &str = r#"{
            "id": 13,
            "title": "Add feature",
            "links": {"html": {"href": "https://bitbucket.org/acme/widget/pull-requests/13"}},
            "author": {"display_name": "Rene Langer"},
            "source": {"branch": {"name": "feature/pr"}},
            "destination": {"branch": {"name": "main"}},
            "state": "OPEN"
        }"#;
        let api = FakeForgeApi::queue(vec![ok(201, BITBUCKET_CREATE_FIXTURE)]);
        let service = PullRequestService::new(&api);
        let create = CreatePullRequest {
            title: "Add feature".to_string(),
            description: Some("Implements the thing".to_string()),
            source_branch: "feature/pr".to_string(),
            target_branch: "main".to_string(),
        };

        let pull_request = service
            .create_pull_request(&bitbucket_repo(), Some("bb-token-456"), &create)
            .unwrap();

        assert_eq!(pull_request.id, "13");
        let requests = api.requests.borrow();
        assert_eq!(requests[0].method, ForgeHttpMethod::Post);
        assert_eq!(
            requests[0].url,
            "https://api.bitbucket.org/2.0/repositories/acme/widget/pullrequests"
        );
        assert_eq!(
            requests[0].json_body,
            Some(serde_json::json!({
                "title": "Add feature",
                "description": "Implements the thing",
                "source": { "branch": { "name": "feature/pr" } },
                "destination": { "branch": { "name": "main" } },
            }))
        );
    }

    #[test]
    fn a_401_response_becomes_a_secret_free_unauthorized_error() {
        let token = "gh-token-should-never-appear";
        let api = FakeForgeApi::queue(vec![ok(401, r#"{"message": "Bad credentials"}"#)]);
        let service = PullRequestService::new(&api);

        let error = service
            .list_pull_requests(&github_repo(), Some(token))
            .expect_err("a 401 must be rejected");

        assert_eq!(error, PullRequestError::Unauthorized);
        assert!(!error.to_string().contains(token));
    }

    #[test]
    fn a_github_validation_response_surfaces_the_providers_message_without_the_token() {
        let token = "gh-token-should-never-appear";
        let api = FakeForgeApi::queue(vec![ok(422, r#"{"message": "Validation Failed"}"#)]);
        let service = PullRequestService::new(&api);

        let error = service
            .list_pull_requests(&github_repo(), Some(token))
            .expect_err("a 422 must be rejected");

        assert_eq!(
            error,
            PullRequestError::Validation("Validation Failed".to_string())
        );
        assert!(!error.to_string().contains(token));
    }

    #[test]
    fn a_bitbucket_validation_response_surfaces_the_nested_error_message() {
        let token = "bb-token-should-never-appear";
        let api = FakeForgeApi::queue(vec![ok(
            400,
            r#"{"type": "error", "error": {"message": "source and destination branches must be different"}}"#,
        )]);
        let service = PullRequestService::new(&api);

        let error = service
            .list_pull_requests(&bitbucket_repo(), Some(token))
            .expect_err("a 400 must be rejected");

        assert_eq!(
            error,
            PullRequestError::Validation(
                "source and destination branches must be different".to_string()
            )
        );
        assert!(!error.to_string().contains(token));
    }

    #[test]
    fn a_timeout_from_the_transport_becomes_a_secret_free_timeout_error() {
        let token = "gh-token-should-never-appear";
        let api = FakeForgeApi::queue(vec![Err(ForgeApiError::Timeout)]);
        let service = PullRequestService::new(&api);

        let error = service
            .list_pull_requests(&github_repo(), Some(token))
            .expect_err("a timeout must be rejected");

        assert_eq!(error, PullRequestError::Timeout);
        assert!(!error.to_string().contains(token));
    }

    #[test]
    fn a_transport_failure_becomes_a_secret_free_transport_error() {
        let token = "gh-token-should-never-appear";
        let api = FakeForgeApi::queue(vec![Err(ForgeApiError::Transport)]);
        let service = PullRequestService::new(&api);

        let error = service
            .list_pull_requests(&github_repo(), Some(token))
            .expect_err("a transport failure must be rejected");

        assert_eq!(error, PullRequestError::Transport);
        assert!(!error.to_string().contains(token));
    }

    #[test]
    fn an_unparseable_success_body_becomes_an_unexpected_response_error() {
        let api = FakeForgeApi::queue(vec![ok(200, "not json")]);
        let service = PullRequestService::new(&api);

        let error = service
            .list_pull_requests(&github_repo(), Some("gh-token-123"))
            .expect_err("malformed success bodies must not panic or be silently accepted");

        assert_eq!(error, PullRequestError::UnexpectedResponse);
    }

    #[test]
    fn an_unrecognized_error_status_becomes_an_unexpected_response_error() {
        let api = FakeForgeApi::queue(vec![ok(503, "service unavailable")]);
        let service = PullRequestService::new(&api);

        let error = service
            .list_pull_requests(&github_repo(), Some("gh-token-123"))
            .expect_err("an unmapped status must not be treated as success");

        assert_eq!(error, PullRequestError::UnexpectedResponse);
    }
}
