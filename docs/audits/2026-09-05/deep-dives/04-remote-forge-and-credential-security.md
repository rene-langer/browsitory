# Deep-dive 04: Remote, Forge, and Credential Security

## Purpose, scope, audit questions

Does credential handling for git remotes (HTTPS token, SSH agent) and forge integrations
(GitHub/Bitbucket PR API tokens) actually deliver the guarantees `docs/ARCHITECTURE.md`'s
"Credential release acceptance" checklist documents: no re-prompting on repeat use, no token
leakage into `.git/config`, error text, or logs, and no accidental credential-store cross-talk
between transport auth and forge API auth? Is the `forge-fixture-override` feature (which swaps
the real OS-keychain store for an in-memory one and redirects forge API base URLs) safely
confined to test builds and structurally excluded from anything that ships?

## Evidence reviewed

- `crates/tauri-app/src/credentials.rs` (full read, 696 lines including its 24-test module).
- `crates/git-core/src/remote.rs` (grepped for the `MISSING_CREDENTIAL_ERROR` /
  `CREDENTIAL_STORE_FAILURE_ERROR` / `SSH_AGENT_FAILURE_ERROR` constants and the `auth-mode`
  config-key read/write call sites, lines ~49-51, 434-447, 691-696).
- `frontend/src/state/useMutationRunner.ts:24-34` (`credentialFailureMessage`, via `graft ask`).
- `crates/tauri-app/Cargo.toml`'s `[features]` section (doc comments on `forge-fixture-override`).
- `scripts/build-dist.sh` (confirms no `--features` flag is ever passed to `cargo tauri build`).
- `docs/ARCHITECTURE.md`'s "Credential release acceptance" section (full read).
- `cargo test -p tauri-app --features forge-fixture-override` — actual verified output (not
  inferred): **93 passed, 0 failed, 0 ignored, finished in 0.54s**. This is the feature build
  that exercises `InMemoryCredentialStore` and the API-base-URL override together; it is a
  superset covering every credential/forge test in the plain build plus the fixture-specific
  ones. Two tests beyond those cited above are worth calling out explicitly:
  `pull_requests::tests::forge_http_request_debug_output_redacts_the_authorization_header`
  (the `Debug` formatting used on outgoing forge HTTP requests, which can end up in error/log
  text, redacts the bearer token) and
  `worker::tests::saved_forge_token_is_looked_up_inside_rust_and_never_exposed_in_the_result`
  (a saved forge token is consulted server-side but never echoed back across the `Worker`/IPC
  boundary in a command result). Both directly substantiate the "no secret leaves the process
  boundary" claim in Strengths below, independent of the static trace.

## Architecture traced

`CredentialKey::for_https` derives a **non-secret** keychain lookup key from the URL: host +
non-default port + username only (`credentials.rs:31-52`) — the token itself never appears in
the key, and the key construction rejects any URL that already embeds userinfo/password
(`credentials.rs:33-38`), so a malformed or already-credentialed URL can't smuggle a secret into
what becomes a keychain *account* string (which, depending on OS keychain, can be visible in a
system credential manager UI).

`CredentialKey::for_forge` uses an entirely separate keychain **service** name
(`com.browsitory.forge` vs `com.browsitory.git`) and namespaces the account as
`forge:<provider>:<account>` (`credentials.rs:15,54-59`), so a forge API token and a Git HTTPS
transport token for the same host/account can never collide or be accidentally substituted for
each other. `RemoteCredentialProvider::credential` (`credentials.rs:310-343`) only ever consults
the HTTPS/SSH path relevant to the configured `RemoteAuthMode`; the SSH branch
(`Some(RemoteAuthMode::SshAgent)`) calls `self.ssh_agent.credential(...)` directly and **never**
touches `self.service` (the credential store) at all — confirmed by a dedicated test,
`ssh_provider_uses_the_callback_username_or_git_without_querying_the_store`
(`credentials.rs:672-694`), which wires in a `PanicOnGetStore` that panics if `get()` is ever
called and asserts the SSH path never triggers it.

`git-core/src/remote.rs:434-447` writes only `browsitory.remote.<name>.auth-mode` to
`.git/config` (`"https-token"` or `"ssh-agent"`, both fixed non-secret strings) — no token, no
username-with-secret is written there. Reading it back (line 691-696) drives which
`RemoteCredentialProvider` branch is taken.

`forge-fixture-override` (`Cargo.toml`) is `#[cfg]`-gated at the *type* level, not merely
call-site-gated: without the feature, `InMemoryCredentialStore` (`credentials.rs:152-184`) does
not exist in the compiled binary at all (per its own doc comment, `credentials.rs:137-151`), and
`Worker::spawn` (per the parent architecture doc and confirmed structurally by the `#[cfg]`
split at `credentials.rs:102` / `152`) can only construct the real `KeyringCredentialStore` in a
non-feature build. `scripts/build-dist.sh:27` invokes `cargo tauri build "$@"` with no
`--features` argument, and neither the Cargo manifest nor `tauri.release.conf.json` declares
`forge-fixture-override` as a default feature (no `default = [...]` entry in
`crates/tauri-app/Cargo.toml`'s `[features]` block includes it) — so a standard release build
structurally cannot include the in-memory store or the API-base-URL override.

## Findings

No confirmed defects were found in this area. The implementation matches every guarantee in
`docs/ARCHITECTURE.md`'s acceptance checklist that could be verified statically, and the claims
are independently backed by a dedicated, adversarially-written test module (a `PanicOnGetStore`
and a `FailingCredentialStore` test double specifically built to catch exactly the failure modes
the checklist warns about). See "Strengths" below.

One item is downgraded from a finding to an accepted-risk / open question because it could not
be fully confirmed without live network/keychain access (out of scope for a static/read-only
audit environment):

### Open question — unmatched error messages could pass through libgit2 diagnostic text verbatim to the UI

- **Severity if confirmed:** Low
- **Confidence:** Unconfirmed (plausible, not demonstrated)
- **Affected components:** `frontend/src/state/useMutationRunner.ts:24-34`
  (`credentialFailureMessage`).
- **Evidence:** `credentialFailureMessage` pattern-matches on three known substrings ("missing
  credential", "credential keychain failure", "SSH agent failure") and falls back to returning
  the raw `error.message` verbatim for anything else (`useMutationRunner.ts:33`,
  `return message;`). All three credential-specific error paths in `credentials.rs` are proven
  by test to map to exactly those three constants, so the credential-specific paths are safe.
  However, other libgit2 transport errors (DNS failure, TLS failure, non-credential HTTP 4xx/5xx)
  are not classified by `credentials.rs` at all and would fall through to this same generic
  fallback in the frontend. libgit2 error strings for transport failures sometimes embed the
  remote URL (which is not secret) but, depending on libgit2 version and platform, could in rare
  cases embed proxy or redirect target details. No token embedding was found or is plausible here
  (no code path constructs a git2::Error containing the token literal), so the residual risk is
  informational-to-low, not credential leakage.
- **Trigger / reproduction (not performed):** Would require a live remote with a non-credential
  transport failure (e.g., an unreachable host) to observe the exact string surfaced to the UI.
- **Remediation:** If this risk is worth closing, add an explicit "unrecognized transport error"
  classification in `credentialFailureMessage` (or upstream in `credentials.rs`) that only ever
  surfaces a generic message plus a reference to the failure log (see `09`'s DOC-003), rather
  than forwarding arbitrary libgit2 text to the UI.
- **Verification:** A live-remote reproduction against a disposable non-credential failure mode
  (e.g., pointing at a closed port) would confirm or rule this out; not performed in this audit.

## Coverage gaps

- The manual "Credential release acceptance" procedure in `docs/ARCHITECTURE.md` (live HTTPS
  fetch-twice / forget / SSH-agent fetch-and-push against disposable remotes) was traced
  statically against the code but **not executed live** — this audit environment has no
  disposable git hosting, token, or SSH key provisioned, and creating one was out of scope for a
  read-only code audit. This is the single most important unresolved verification gap in this
  area: the checklist is a *release gate*, and this audit confirms the code satisfies its
  letter, not that a live run currently passes.
- Forge (GitHub/Bitbucket) API-side security — request signing, rate-limit handling, response
  validation in `crates/tauri-app/src/pull_requests.rs` (47.7K, not read in this pass) — was not
  audited in depth; only the credential-storage boundary it depends on was.
- Host validation (whether a forge base-URL override or a malicious/typo'd forge remote URL
  could be misclassified as GitHub/Bitbucket and receive a token meant for a different host) was
  not traced in `git_core::forge`'s classification logic in this pass.

## Strengths and positive controls

- Dedicated, adversarial test doubles (`PanicOnGetStore`, `FailingCredentialStore`) directly
  encode the security invariants the architecture doc claims, rather than leaving them as
  undocumented assumptions — this is a materially stronger control than typical "happy path
  only" credential tests.
- Structural (type-level, `#[cfg]`-gated) rather than merely logical separation between the
  test-fixture in-memory credential store and the production keychain store, verified to be
  unreachable from the documented release build path (`scripts/build-dist.sh`).
- Clear separation of Git-transport and forge-API credential namespaces at both the keychain
  service-name and account-string level, with a test (`a_saved_https_credential_is_never_
  returned_by_the_forge_token_lookup`) that would fail if the two were ever accidentally
  unified.
- `.git/config` write path is minimal and auditable: only a fixed non-secret enum-like string
  value, never a token or password.
