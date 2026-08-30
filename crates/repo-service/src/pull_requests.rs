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

/// Displayed for `PullRequest::author` when a provider's response has no usable author (GitHub
/// returns `user: null` for a deleted account; some Bitbucket responses omit
/// `author.display_name`). Keeps the row visible instead of failing the whole list — see
/// `parse_list_body`/`parse_single_body`.
const UNKNOWN_AUTHOR: &str = "unknown";

/// A page of listed pull requests, plus whether the provider's response indicates more pages
/// exist beyond this one (GitHub: a `Link: rel="next"` response header; Bitbucket: a non-null
/// `next` field in the response body). Both providers cap a single page well below what an
/// active repository can have open, so this is surfaced explicitly rather than silently
/// dropping the rest — see `build_list_request`'s `per_page`/`pagelen` and `detect_truncation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestList {
    pub pull_requests: Vec<PullRequest>,
    pub truncated: bool,
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
///
/// `Debug` is hand-rolled rather than derived: `headers` carries the `Authorization: Bearer
/// <token>` header, and a derived `Debug` would print the token verbatim into any log/panic
/// message/test failure output that happens to `{:?}`-format a request. Every other field
/// derives normally.
#[derive(Clone, PartialEq, Eq)]
pub struct ForgeHttpRequest {
    pub method: ForgeHttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub json_body: Option<serde_json::Value>,
}

impl std::fmt::Debug for ForgeHttpRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redacted_headers: Vec<(&str, &str)> = self
            .headers
            .iter()
            .map(|(name, value)| {
                if name.eq_ignore_ascii_case("authorization") {
                    (name.as_str(), "<redacted>")
                } else {
                    (name.as_str(), value.as_str())
                }
            })
            .collect();
        formatter
            .debug_struct("ForgeHttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &redacted_headers)
            .field("json_body", &self.json_body)
            .finish()
    }
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
    /// Response headers, `(name, value)`, used only to detect pagination truncation today (a
    /// GitHub `Link` header) — see `detect_truncation`. Case is whatever the transport gave us;
    /// always compare names case-insensitively (HTTP header names are case-insensitive).
    pub headers: Vec<(String, String)>,
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
/// `crates/repo-service/src/worker/mod.rs` — one `thread::spawn`-owned thread per open
/// repository, no tokio runtime anywhere in this crate).
///
/// The inner client is built lazily, on first use, rather than in `new()`. `Worker::spawn`
/// constructs a `ReqwestForgeApi` on the *caller's* thread (before handing it off to the
/// dedicated worker OS thread — see that function), and that caller is a Tauri `async fn`
/// command running on Tauri's own tokio runtime.
/// `reqwest::blocking::Client::builder().build()` briefly spins up and tears down its own
/// internal tokio runtime as part of construction, which panics ("Cannot drop a runtime in a
/// context where blocking is not allowed") if it happens while already inside another tokio
/// runtime's async context — exactly `Worker::spawn`'s caller. Deferring the real `build()` call
/// to first `send()` moves it onto the worker OS thread instead (the only place `send()` is ever
/// called), which has no tokio runtime at all, matching this struct's doc comment above.
pub struct ReqwestForgeApi {
    client: std::sync::OnceLock<Result<reqwest::blocking::Client, ()>>,
}

impl ReqwestForgeApi {
    pub fn new() -> Self {
        Self {
            client: std::sync::OnceLock::new(),
        }
    }

    fn client(&self) -> Result<&reqwest::blocking::Client, ForgeApiError> {
        self.client
            .get_or_init(|| {
                reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build()
                    .map_err(|_| ())
            })
            .as_ref()
            .map_err(|()| ForgeApiError::Transport)
    }
}

impl Default for ReqwestForgeApi {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeApi for ReqwestForgeApi {
    fn send(&self, request: ForgeHttpRequest) -> Result<ForgeHttpResponse, ForgeApiError> {
        let client = self.client()?;
        let mut builder = match request.method {
            ForgeHttpMethod::Get => client.get(&request.url),
            ForgeHttpMethod::Post => client.post(&request.url),
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
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_string(), value.to_string()))
            })
            .collect();
        let body = response.text().map_err(|_| ForgeApiError::Transport)?;
        Ok(ForgeHttpResponse {
            status,
            body,
            headers,
        })
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
    ) -> Result<PullRequestList, PullRequestError> {
        let token = token.ok_or(PullRequestError::MissingToken)?;
        let request = build_list_request(repository, token);
        let response = self.execute(request)?;
        let pull_requests = parse_list_body(repository.provider, &response.body)?;
        let truncated = detect_truncation(repository.provider, &response);
        Ok(PullRequestList {
            pull_requests,
            truncated,
        })
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
        let response = self.execute(request)?;
        parse_single_body(repository.provider, &response.body)
    }

    fn execute(&self, request: ForgeHttpRequest) -> Result<ForgeHttpResponse, PullRequestError> {
        let response = self.api.send(request).map_err(|error| match error {
            ForgeApiError::Timeout => PullRequestError::Timeout,
            ForgeApiError::Transport => PullRequestError::Transport,
        })?;
        match response.status {
            200..=299 => Ok(response),
            401 | 403 => Err(PullRequestError::Unauthorized),
            400 | 422 => Err(PullRequestError::Validation(extract_validation_message(
                &response.body,
            ))),
            _ => Err(PullRequestError::UnexpectedResponse),
        }
    }
}

/// Detects whether `response` (a successful list response) indicates more results exist beyond
/// this page: GitHub signals this via a `Link` response header containing `rel="next"`;
/// Bitbucket signals it via a non-null top-level `next` field in the JSON body. Never fails —
/// an unparseable/absent signal is simply treated as "not truncated" (the plain, already-tested
/// `parse_list_body` call is what surfaces a genuinely malformed body as an error).
fn detect_truncation(provider: ForgeProvider, response: &ForgeHttpResponse) -> bool {
    match provider {
        ForgeProvider::GitHub => response.headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("link") && value.contains("rel=\"next\"")
        }),
        ForgeProvider::Bitbucket => serde_json::from_str::<serde_json::Value>(&response.body)
            .ok()
            .and_then(|value| value.get("next").cloned())
            .is_some_and(|next| !next.is_null()),
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

// Test-only seams: `PullRequestService` otherwise always talks to the real GitHub/Bitbucket
// Cloud APIs, by design (see this module's doc comment — unit tests use `ForgeApi` fakes
// instead, precisely so they never need a mock HTTP server). Black-box E2E coverage
// (`e2e/specs/pull-requests.spec.ts`) drives the actual built binary through WebDriver and has
// no seam inside the process to substitute a fake `ForgeApi`, so it needs the HTTP destination
// itself to be redirectable to a loopback fixture server.
//
// SECURITY: the env-var override below is gated behind the `forge-fixture-override` Cargo
// feature, which is NOT enabled by default and must never be enabled for a release build — it
// is only turned on by the E2E build invocation (`cargo build --workspace --features
// tauri-app/custom-protocol,tauri-app/forge-fixture-override`, mirroring how `custom-protocol`
// is already an opt-in flag in this crate; see `Cargo.toml`). Without the feature, these
// functions are the plain hardcoded hosts below and never call `std::env::var` at all — the env
// vars have *no effect* and no code path reads them, so nobody who can set process environment
// variables before the shipped app launches can silently redirect a user's real GitHub/Bitbucket
// traffic (and their saved forge token, sent as `Authorization: Bearer <token>`) to another
// host. `cargo build --workspace --release` (no `--features`) was used to confirm this.
// Pure, env-free precedence rule shared by both accessors below: a present, non-empty override
// wins, otherwise fall back to `default`. Split out so the override-precedence logic (including
// the "empty string doesn't count as set" rule) can be unit-tested with plain values — never by
// mutating the real process environment, which `std::env::set_var`/`remove_var` calls would race
// against every other test in this binary that (under the `forge-fixture-override` feature) also
// calls `github_api_base`/`bitbucket_api_base` concurrently on another test thread. See this
// file's own test module for that unit coverage, and
// `e2e/specs/pull-requests.spec.ts` for genuine end-to-end coverage of the real env var actually
// being read by a running process.
#[cfg(feature = "forge-fixture-override")]
fn resolve_api_base(env_override: Option<String>, default: &str) -> String {
    env_override
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

#[cfg(feature = "forge-fixture-override")]
fn github_api_base() -> String {
    resolve_api_base(
        std::env::var("BROWSITORY_FORGE_GITHUB_API_BASE_URL").ok(),
        "https://api.github.com",
    )
}

#[cfg(not(feature = "forge-fixture-override"))]
fn github_api_base() -> String {
    "https://api.github.com".to_string()
}

#[cfg(feature = "forge-fixture-override")]
fn bitbucket_api_base() -> String {
    resolve_api_base(
        std::env::var("BROWSITORY_FORGE_BITBUCKET_API_BASE_URL").ok(),
        "https://api.bitbucket.org/2.0",
    )
}

#[cfg(not(feature = "forge-fixture-override"))]
fn bitbucket_api_base() -> String {
    "https://api.bitbucket.org/2.0".to_string()
}

fn build_list_request(repository: &ForgeRepository, token: &str) -> ForgeHttpRequest {
    match repository.provider {
        // `per_page`/`pagelen` are each provider's maximum allowed page size (GitHub defaults
        // to 30, Bitbucket to 10 — both silently truncate without this). A repository can still
        // have more open PRs than even this larger page; `detect_truncation` catches that case
        // rather than silently hiding it, since full cursor-following isn't implemented here.
        ForgeProvider::GitHub => ForgeHttpRequest {
            method: ForgeHttpMethod::Get,
            url: format!(
                "{}/repos/{}/{}/pulls?state=open&per_page=100",
                github_api_base(),
                repository.owner,
                repository.name
            ),
            headers: github_headers(token),
            json_body: None,
        },
        ForgeProvider::Bitbucket => ForgeHttpRequest {
            method: ForgeHttpMethod::Get,
            url: format!(
                "{}/repositories/{}/{}/pullrequests?state=OPEN&pagelen=100",
                bitbucket_api_base(),
                repository.owner,
                repository.name
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
                "{}/repos/{}/{}/pulls",
                github_api_base(),
                repository.owner,
                repository.name
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
                "{}/repositories/{}/{}/pullrequests",
                bitbucket_api_base(),
                repository.owner,
                repository.name
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
    // `null` for a pull request opened by a since-deleted GitHub account. `Option` here (rather
    // than requiring the field) means one such row degrades to `UNKNOWN_AUTHOR` instead of
    // failing `serde_json::from_str` for the entire list/response — see `From<GitHubPullRequest>`.
    user: Option<GitHubUser>,
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
            author: pull_request
                .user
                .map(|user| user.login)
                .unwrap_or_else(|| UNKNOWN_AUTHOR.to_string()),
            source_branch: pull_request.head.name,
            target_branch: pull_request.base.name,
            state: pull_request.state.to_lowercase(),
        }
    }
}

#[derive(Deserialize)]
struct BitbucketAuthor {
    // Some Bitbucket responses (e.g. for a deactivated/removed workspace member) omit this
    // field entirely rather than sending an empty string. `Option` here means one such row
    // degrades to `UNKNOWN_AUTHOR` instead of failing the entire list/response — see
    // `From<BitbucketPullRequest>`.
    display_name: Option<String>,
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
            author: pull_request
                .author
                .display_name
                .unwrap_or_else(|| UNKNOWN_AUTHOR.to_string()),
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
            headers: Vec::new(),
        })
    }

    fn ok_with_headers(
        status: u16,
        body: &str,
        headers: Vec<(String, String)>,
    ) -> Result<ForgeHttpResponse, ForgeApiError> {
        Ok(ForgeHttpResponse {
            status,
            body: body.to_string(),
            headers,
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

    // Only compiled (and only meaningful) with the `forge-fixture-override` feature enabled —
    // without it, `github_api_base`/`bitbucket_api_base` are the plain hardcoded-host functions
    // and don't reference an env var at all, and `resolve_api_base` doesn't exist. Deliberately
    // tests the pure `resolve_api_base` helper with plain values rather than mutating the real
    // process environment: this test module runs with the default multi-threaded test harness,
    // and every other test that calls `list_pull_requests`/`create_pull_request` under this same
    // feature also calls `github_api_base`/`bitbucket_api_base` — a `std::env::set_var` here
    // would be visible, mid-test, to those other tests running concurrently on other threads,
    // making them flaky. `e2e/specs/pull-requests.spec.ts` is what actually exercises the real
    // env var being read by a running process end-to-end. Run with `cargo test -p tauri-app
    // --features forge-fixture-override pull_requests::`.
    #[cfg(feature = "forge-fixture-override")]
    #[test]
    fn resolve_api_base_prefers_a_present_non_empty_override_over_the_default() {
        assert_eq!(
            resolve_api_base(
                Some("http://127.0.0.1:9".to_string()),
                "https://api.github.com"
            ),
            "http://127.0.0.1:9"
        );
    }

    #[cfg(feature = "forge-fixture-override")]
    #[test]
    fn resolve_api_base_falls_back_to_the_default_when_unset_or_empty() {
        assert_eq!(
            resolve_api_base(None, "https://api.github.com"),
            "https://api.github.com"
        );
        assert_eq!(
            resolve_api_base(Some(String::new()), "https://api.github.com"),
            "https://api.github.com"
        );
    }

    #[cfg(feature = "forge-fixture-override")]
    #[test]
    fn github_and_bitbucket_api_base_default_to_the_real_hosts_when_unset() {
        // Guards the actual env var *names* `github_api_base`/`bitbucket_api_base` read — the
        // above two tests cover the override-precedence logic in isolation, but not that these
        // two functions are wired to the right names. Reads only (never sets/removes), so this
        // is race-free regardless of what any other concurrently-running test does to the real
        // environment: as long as nothing else in this test binary ever sets
        // `BROWSITORY_FORGE_{GITHUB,BITBUCKET}_API_BASE_URL` (true — this is the only place in
        // the crate that reads them, and no test sets them), both resolve to their defaults.
        assert_eq!(github_api_base(), "https://api.github.com");
        assert_eq!(bitbucket_api_base(), "https://api.bitbucket.org/2.0");
    }

    // The security-relevant half of the above: without the feature (the only way this crate is
    // ever built for release — see `Cargo.toml`'s `forge-fixture-override` doc comment), the env
    // vars have *zero effect*, because these two functions are the plain hardcoded-host bodies
    // below and never call `std::env::var` at all in this cfg branch — there is no code path in
    // this binary that could read them. That's provable statically, with no env var manipulation
    // needed (and `worker.rs`'s tests, in this same `tauri-app` test binary, DO spawn real OS
    // threads that read process environment via libgit2 — mutating the environment here would
    // race against those under cargo's default parallel test harness). Guards against a future
    // edit accidentally moving the `#[cfg]` gate or the env lookup back to always-on: if either
    // function starts reading the environment in this branch, this constant assertion is the
    // only thing that would still catch it.
    #[cfg(not(feature = "forge-fixture-override"))]
    #[test]
    fn the_api_base_url_env_var_has_no_effect_without_the_fixture_override_feature() {
        assert_eq!(github_api_base(), "https://api.github.com");
        assert_eq!(bitbucket_api_base(), "https://api.bitbucket.org/2.0");
        let request = build_list_request(&github_repo(), "token");
        assert_eq!(
            request.url,
            "https://api.github.com/repos/acme/widget/pulls?state=open&per_page=100"
        );
    }

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

        let result = service
            .list_pull_requests(&github_repo(), Some("gh-token-123"))
            .unwrap();

        assert_eq!(
            result.pull_requests,
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
        assert!(!result.truncated);

        let requests = api.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, ForgeHttpMethod::Get);
        assert_eq!(
            requests[0].url,
            "https://api.github.com/repos/acme/widget/pulls?state=open&per_page=100"
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

        let result = service
            .list_pull_requests(&bitbucket_repo(), Some("bb-token-456"))
            .unwrap();

        assert_eq!(
            result.pull_requests,
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
        assert!(!result.truncated);

        let requests = api.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, ForgeHttpMethod::Get);
        assert_eq!(
            requests[0].url,
            "https://api.bitbucket.org/2.0/repositories/acme/widget/pullrequests?state=OPEN&pagelen=100"
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

    // --- Finding 4: pagination / truncation detection ---

    #[test]
    fn detects_truncation_from_a_github_link_header() {
        let api = FakeForgeApi::queue(vec![ok_with_headers(
            200,
            GITHUB_LIST_FIXTURE,
            vec![(
                "Link".to_string(),
                "<https://api.github.com/repos/acme/widget/pulls?page=2>; rel=\"next\"".to_string(),
            )],
        )]);
        let service = PullRequestService::new(&api);

        let result = service
            .list_pull_requests(&github_repo(), Some("gh-token-123"))
            .unwrap();

        assert!(result.truncated);
        assert_eq!(result.pull_requests.len(), 1);
    }

    #[test]
    fn a_github_link_header_without_rel_next_is_not_truncated() {
        let api = FakeForgeApi::queue(vec![ok_with_headers(
            200,
            GITHUB_LIST_FIXTURE,
            vec![(
                "Link".to_string(),
                "<https://api.github.com/repos/acme/widget/pulls?page=1>; rel=\"prev\"".to_string(),
            )],
        )]);
        let service = PullRequestService::new(&api);

        let result = service
            .list_pull_requests(&github_repo(), Some("gh-token-123"))
            .unwrap();

        assert!(!result.truncated);
    }

    #[test]
    fn detects_truncation_from_a_non_null_bitbucket_next_field() {
        const TRUNCATED_FIXTURE: &str = r#"{
            "next": "https://api.bitbucket.org/2.0/repositories/acme/widget/pullrequests?page=2",
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
        let api = FakeForgeApi::queue(vec![ok(200, TRUNCATED_FIXTURE)]);
        let service = PullRequestService::new(&api);

        let result = service
            .list_pull_requests(&bitbucket_repo(), Some("bb-token-456"))
            .unwrap();

        assert!(result.truncated);
    }

    #[test]
    fn build_list_request_asks_for_a_large_explicit_page_size() {
        assert!(build_list_request(&github_repo(), "token")
            .url
            .contains("per_page=100"));
        assert!(build_list_request(&bitbucket_repo(), "token")
            .url
            .contains("pagelen=100"));
    }

    // --- Finding 5: a missing/null author must not fail the whole list ---

    #[test]
    fn a_github_pull_request_with_a_null_user_falls_back_to_an_unknown_author() {
        const FIXTURE: &str = r#"[
            {
                "id": 101,
                "number": 7,
                "title": "Add pull request support",
                "html_url": "https://github.com/acme/widget/pull/7",
                "user": null,
                "head": {"ref": "feature/pr"},
                "base": {"ref": "main"},
                "state": "open"
            }
        ]"#;
        let api = FakeForgeApi::queue(vec![ok(200, FIXTURE)]);
        let service = PullRequestService::new(&api);

        let result = service
            .list_pull_requests(&github_repo(), Some("gh-token-123"))
            .expect("a null user must not fail the whole list");

        assert_eq!(result.pull_requests[0].author, "unknown");
    }

    #[test]
    fn a_bitbucket_pull_request_missing_display_name_falls_back_to_an_unknown_author() {
        const FIXTURE: &str = r#"{
            "values": [
                {
                    "id": 12,
                    "title": "Add pull request support",
                    "links": {"html": {"href": "https://bitbucket.org/acme/widget/pull-requests/12"}},
                    "author": {},
                    "source": {"branch": {"name": "feature/pr"}},
                    "destination": {"branch": {"name": "main"}},
                    "state": "OPEN"
                }
            ]
        }"#;
        let api = FakeForgeApi::queue(vec![ok(200, FIXTURE)]);
        let service = PullRequestService::new(&api);

        let result = service
            .list_pull_requests(&bitbucket_repo(), Some("bb-token-456"))
            .expect("a missing display_name must not fail the whole list");

        assert_eq!(result.pull_requests[0].author, "unknown");
    }

    // --- Minor: `ForgeHttpRequest`'s `Debug` impl must redact the bearer token ---

    #[test]
    fn forge_http_request_debug_output_redacts_the_authorization_header() {
        let request = ForgeHttpRequest {
            method: ForgeHttpMethod::Get,
            url: "https://api.github.com/repos/acme/widget/pulls".to_string(),
            headers: vec![(
                "Authorization".to_string(),
                "Bearer super-secret-token".to_string(),
            )],
            json_body: None,
        };

        let debug_output = format!("{request:?}");

        assert!(!debug_output.contains("super-secret-token"));
        assert!(debug_output.contains("<redacted>"));
    }
}
