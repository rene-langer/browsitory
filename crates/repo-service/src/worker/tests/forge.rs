use super::*;

/// In-memory `CredentialStore` fake for worker tests — the design doc's "in-memory trait
/// implementations only at external seams such as the keychain" rule applies here just as
/// it does to `credentials.rs`'s own tests; the real OS keychain is never touched.
#[derive(Default)]
struct FakeCredentialStore {
    tokens: RefCell<HashMap<CredentialKey, String>>,
}

impl CredentialStore for FakeCredentialStore {
    fn get(&self, key: &CredentialKey) -> Result<Option<String>, CredentialStoreError> {
        Ok(self.tokens.borrow().get(key).cloned())
    }

    fn set(&self, key: &CredentialKey, token: &str) -> Result<(), CredentialStoreError> {
        self.tokens
            .borrow_mut()
            .insert(key.clone(), token.to_owned());
        Ok(())
    }

    fn delete(&self, key: &CredentialKey) -> Result<(), CredentialStoreError> {
        self.tokens.borrow_mut().remove(key);
        Ok(())
    }
}

/// Canned-response `ForgeApi` fake — the other external seam the design doc calls out.
/// Records every request it receives so tests can assert on headers (e.g. the injected
/// `Authorization` token) without ever making a real HTTP call. `Arc<Mutex<..>>`-backed
/// (rather than `RefCell`, like `pull_requests.rs`'s own same-thread fake) because this one
/// crosses the worker's background-thread boundary: a clone kept in the test can still
/// inspect requests recorded by the clone moved into `Worker::spawn_with`.
#[derive(Clone, Default)]
struct FakeForgeApi {
    responses: Arc<Mutex<VecDeque<Result<ForgeHttpResponse, ForgeApiError>>>>,
    requests: Arc<Mutex<Vec<ForgeHttpRequest>>>,
}

impl FakeForgeApi {
    fn queue(responses: Vec<Result<ForgeHttpResponse, ForgeApiError>>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn requests(&self) -> Vec<ForgeHttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl crate::pull_requests::ForgeApi for FakeForgeApi {
    fn send(&self, request: ForgeHttpRequest) -> Result<ForgeHttpResponse, ForgeApiError> {
        self.requests.lock().unwrap().push(request);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("test queued an unexpected extra request")
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

#[test]
fn detecting_an_ambiguous_remote_declines_rather_than_erroring_or_requesting() {
    let (dir, repo) = init_repo();
    // Three path segments (owner/name/extra) is ambiguous per git-core's forge parsing.
    repo.remote("origin", "https://github.com/acme/widget/extra.git")
        .unwrap();

    // An empty response queue: any attempted HTTP request panics the test.
    let worker = Worker::spawn_with(
        dir.path().to_path_buf(),
        FakeCredentialStore::default(),
        FakeForgeApi::queue(vec![]),
    )
    .unwrap();
    let handle = worker.handle();

    assert_eq!(handle.detect_forge_repository().unwrap(), vec![]);

    let error = handle
        .list_pull_requests("origin".to_string(), "rene".to_string())
        .expect_err("an ambiguous remote must not be treated as a supported repository");
    assert_eq!(error, "this remote is not a supported forge repository");
}

#[test]
fn saved_forge_token_is_looked_up_inside_rust_and_never_exposed_in_the_result() {
    let (dir, repo) = init_repo();
    repo.remote("origin", "https://github.com/acme/widget.git")
        .unwrap();
    let token = "gh-token-should-never-leak";

    let api = FakeForgeApi::queue(vec![ok(200, GITHUB_LIST_FIXTURE)]);
    let worker = Worker::spawn_with(
        dir.path().to_path_buf(),
        FakeCredentialStore::default(),
        api.clone(),
    )
    .unwrap();
    let handle = worker.handle();

    assert_eq!(
        handle
            .detect_forge_repository()
            .unwrap()
            .into_iter()
            .map(|repository| repository.remote_name)
            .collect::<Vec<_>>(),
        vec!["origin".to_string()]
    );

    // Listing before any token is saved must be rejected without ever calling the
    // transport — `Worker::list_pull_requests`'s signature has no token parameter at all,
    // so the only way this can succeed is a lookup happening inside the worker.
    let before = handle
        .list_pull_requests("origin".to_string(), "rene".to_string())
        .expect_err("a missing token must be rejected before any request");
    assert_eq!(before, "a provider token is required for this action");
    assert!(api.requests().is_empty());

    handle
        .save_forge_token(ForgeProvider::GitHub, "rene".to_string(), token.to_string())
        .unwrap();

    let pull_requests = handle
        .list_pull_requests("origin".to_string(), "rene".to_string())
        .unwrap();

    assert_eq!(pull_requests.pull_requests.len(), 1);
    assert_eq!(pull_requests.pull_requests[0].number, 7);
    assert_eq!(
        pull_requests.pull_requests[0].title,
        "Add pull request support"
    );
    assert!(!pull_requests.truncated);
    // The lookup happened inside the worker: the request the fake transport actually
    // received carries the saved token in its Authorization header.
    let requests = api.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0]
        .headers
        .contains(&("Authorization".to_string(), format!("Bearer {token}"))));
    // Round-tripped result never carries the token.
    assert!(!format!("{pull_requests:?}").contains(token));

    // Forgetting the token makes subsequent listing fail again the same way.
    handle
        .forget_forge_token(ForgeProvider::GitHub, "rene".to_string())
        .unwrap();
    let after_forget = handle
        .list_pull_requests("origin".to_string(), "rene".to_string())
        .expect_err("a forgotten token must not still be usable");
    assert_eq!(after_forget, "a provider token is required for this action");
}

#[test]
fn create_pull_request_round_trips_through_the_worker_without_exposing_the_token() {
    let (dir, repo) = init_repo();
    repo.remote("origin", "https://github.com/acme/widget.git")
        .unwrap();
    let token = "gh-token-should-never-leak";
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

    let worker = Worker::spawn_with(
        dir.path().to_path_buf(),
        FakeCredentialStore::default(),
        FakeForgeApi::queue(vec![ok(201, GITHUB_CREATE_FIXTURE)]),
    )
    .unwrap();
    let handle = worker.handle();
    handle
        .save_forge_token(ForgeProvider::GitHub, "rene".to_string(), token.to_string())
        .unwrap();

    let created = handle
        .create_pull_request(
            "origin".to_string(),
            "rene".to_string(),
            crate::pull_requests::CreatePullRequest {
                title: "Add feature".to_string(),
                description: Some("Implements the thing".to_string()),
                source_branch: "feature/pr".to_string(),
                target_branch: "main".to_string(),
            },
        )
        .unwrap();

    assert_eq!(created.number, 8);
    assert!(!format!("{created:?}").contains(token));
}
