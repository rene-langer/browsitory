# Sublime Merge parity, batch 3c: commit signing

## Goal

Create signed commits when the user's git config asks for it (`commit.gpgSign`), with OpenPGP
(`gpg`) or SSH (`ssh-keygen`) keys, and show each commit's signature status in the commit details
pane. Both frontends (Tauri app, VSCode extension) through `RepoClient`.

Batch 3 is split: 3a clone + init (shipped, #80), 3b undo stack (shipped), 3c commit signing (this
spec).

## Current state

- Every commit is created unsigned through `git2`:
  - `commit.rs`: `commit` (`repo.commit`) and `amend_head` (`Commit::amend`).
  - `pick.rs`: `apply` (cherry-pick/revert, `repo.commit`).
  - `rebase.rs`: `land_current_step`, which calls `repo.commit` twice (per-step commit and the
    squash/fixup group collapse).
- Merges without conflicts leave `RepositoryState::Merge` and are finished by `commit`, so merge
  commits go through `commit` too. Fast-forwards create no commit.
- `repo-service/worker/undo_store.rs` creates an internal anchor commit. It stays unsigned.
- No signature parsing or display anywhere. `CommitHeader` shows subject, body, author, date,
  SHA and parents; only the message body is fetched (`getCommitMessage`).
- Worker command errors cross the IPC boundary as `String`.

## Decisions

- **Sign and verify.** Signing follows git config; verification is shown in the commit details.
- **Formats: `openpgp` and `ssh`.** `gpg.format=x509` is refused with a clear error when signing,
  and reported as an `Error` status when verifying.
- **Shell out to the signer programs; `git-core` still builds the commits.** The same programs and
  arguments git uses, so the user's gpg-agent / pinentry / ssh-agent / smartcard setup just works.
  Rejected: shelling out to the `git` CLI (hard runtime dependency, two engines); pure-Rust
  crypto (no agent integration, sequoia is LGPL).
- **Git config is authoritative.** No in-app toggle or settings UI.
- **Signing failure aborts the operation** with no commit and no ref change, like git.
- **Verification only in the details pane.** Graph rows show nothing (it would cost a process per
  signed commit). No cache.

## Backend (`git-core`)

### New `signing.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureFormat { OpenPgp, Ssh }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningConfig {
    pub format: SignatureFormat,
    pub program: String,     // gpg.openpgp.program > gpg.program > "gpg"; gpg.ssh.program > "ssh-keygen"
    pub key: Option<String>, // user.signingKey
}

#[derive(Debug, Error)]
pub enum SignError {
    #[error("Signing is configured with gpg.format={0}, which Browsitory does not support.")]
    UnsupportedFormat(String),
    #[error("Signing with SSH needs user.signingKey to be set.")]
    MissingSshKey,
    #[error("Signing program '{0}' was not found.")]
    SignerNotFound(String),
    #[error("Signing failed: {0}")]
    SignerFailed(String), // last few lines of the signer's stderr
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// `Ok(None)` when `commit.gpgSign` is false or unset.
pub fn signing_config(repo: &Repository) -> Result<Option<SigningConfig>, SignError>;

/// Detached, ASCII-armored signature over `buffer`.
pub fn sign_buffer(config: &SigningConfig, buffer: &[u8]) -> Result<String, SignError>;

/// Single entry point for every commit `git-core` creates on the user's behalf.
pub fn create_commit(
    repo: &Repository,
    update_ref: Option<&str>,
    reflog_kind: &str, // "commit" or "commit (amend)"
    author: &Signature<'_>,
    committer: &Signature<'_>,
    message: &str,
    tree: &Tree<'_>,
    parents: &[&Commit<'_>],
) -> Result<Oid, SignError>;
```

`create_commit`:

1. Writes the commit object without touching any ref:
   - unsigned (`signing_config` is `None`): `repo.commit(None, …)`
   - signed: `repo.commit_create_buffer` → `sign_buffer` → `repo.commit_signed(buffer, sig, None)`
2. If `update_ref` is set, moves that ref itself. `"HEAD"` resolves to its branch when attached,
   including an unborn branch; a detached HEAD is moved directly.
   - The ref must still point at `parents[0]`, or be unborn when there are no parents. This uses
     `reference_matching` with `parents[0]` as the expected current id; an unborn ref is created
     with `force = false`.
   - The reflog message is `"<reflog_kind>: <summary>"`, or `"commit (initial): <summary>"` for a
     parentless `"commit"`. These are the exact messages libgit2's `repo.commit` /
     `Commit::amend` write today, so the reflog panel and undo labels see identical entries.
     Tests pin this.
3. One ref-update path for signed and unsigned commits. Amend needs it anyway: its first parent is
   not HEAD's current target, so `repo.commit(Some("HEAD"), …)` would refuse.

The signer runs before any ref moves, so a signing failure leaves the repository untouched.

Call sites switch to `create_commit`:

| Site | `update_ref` |
|---|---|
| `commit::commit` | `Some("HEAD")`, `"commit"` |
| `commit::amend_head` | `Some("HEAD")`, `"commit (amend)"`, with HEAD commit's parents and author. Rewritten from `Commit::amend`, which cannot sign. |
| `pick::apply` | `Some("HEAD")`, `"commit"` |
| `rebase::land_current_step` step commit | `Some("HEAD")`, `"commit"` |
| `rebase::land_current_step` group collapse | `None`, then `set_head_detached` as today |

Each op's error enum gets a `Sign(#[from] SignError)` variant. Messages reach the frontend
unchanged through the existing `String` errors.

### Running the signer

- openpgp: `<program> --status-fd=2 -bsau <key>`. `<key>` is `user.signingKey`, or the
  committer's `Name <email>` when unset (git's default). The buffer goes to stdin and the
  signature comes back on stdout. Success requires a `[GNUPG:] SIG_CREATED` line on stderr; anything
  else is `SignerFailed`.
- ssh: `<program> -Y sign -n git -f <keyfile>`, buffer on stdin, signature on stdout. `user.signingKey`
  is either a path to a key file, or a literal public key (`ssh-…` or `key::ssh-…`). A literal
  key is written to a temp file first, as git does, so the agent can sign with the matching
  private key. `gpg.ssh.defaultKeyCommand` is not supported; an unset key is `MissingSshKey`.
- All signer processes:
  - no TTY; stdin/stdout/stderr are piped
  - `CREATE_NO_WINDOW` on Windows
  - passphrases come from gpg-agent pinentry or ssh-agent
  - `ErrorKind::NotFound` on spawn becomes `SignerNotFound`
- The worker thread blocks while the signer runs, including while pinentry is waiting on the
  user. Other commands for that repo queue behind it, the same as for any long operation.
- `tempfile` moves from a dev-dependency to a runtime dependency of `git-core`. It is already
  vetted (MIT OR Apache-2.0); update its `docs/LICENSE_COMPLIANCE.md` row.

### Verification

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    Unsigned, Good, GoodUntrusted, Bad, UnknownKey, Expired, ExpiredKey, RevokedKey, Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSignature {
    pub status: SignatureStatus,
    pub format: Option<SignatureFormat>,
    pub signer: Option<String>, // gpg uid, or ssh principal
    pub key: Option<String>,    // gpg fingerprint, or ssh SHA256 fingerprint
    pub detail: Option<String>, // human-readable reason for Error / untrusted
}

pub fn verify_commit(repo: &Repository, commit_id: &str) -> Result<CommitSignature, SignError>;
```

- `repo.extract_signature(oid, None)` returning `NotFound` gives `Unsigned` with no process
  spawned.
- The format comes from the signature's armor header (`-----BEGIN PGP SIGNATURE-----` /
  `-----BEGIN SSH SIGNATURE-----`), not from config, so mixed history verifies. Anything else is
  `Error` ("unsupported signature type").
- The program comes from config as for signing (`gpg.openpgp.program`/`gpg.program`, `gpg.ssh.program`).
- openpgp: signature to a temp file, `<program> --status-fd=1 --verify <sigfile> -` with the
  signed data on stdin. Status lines:
  - `GOODSIG` + `TRUST_FULLY`/`TRUST_ULTIMATE` → `Good`
  - `GOODSIG` with another or no trust level → `GoodUntrusted`
  - `BADSIG` → `Bad`
  - `ERRSIG` with `NO_PUBKEY` → `UnknownKey`
  - `EXPSIG` → `Expired`, `EXPKEYSIG` → `ExpiredKey`, `REVKEYSIG` → `RevokedKey`
  - `signer` = uid from the `*SIG` line, `key` = fingerprint from `VALIDSIG` (or the key id
    from `ERRSIG`).
- ssh, with `gpg.ssh.allowedSignersFile` set:
  1. `<program> -Y find-principals -s <sigfile> -f <allowed>`.
  2. For a principal found: `-Y verify -n git -f <allowed> -I <principal> -s <sigfile>` → `Good`
     (signer = principal) or `Bad`.
  3. No principal found: fall back to `-Y check-novalidate -n git -s <sigfile>`.
- ssh, without the file, or after the step-3 fallback: `-Y check-novalidate` succeeding →
  `GoodUntrusted` (detail "signer is not in gpg.ssh.allowedSignersFile"), otherwise `Bad`.
  `key` is the `SHA256:` fingerprint parsed from the output.
- A missing verifier program gives `Error` (detail "gpg was not found"), not `Err`. Only
  repository-level failures (bad oid, unreadable object) return `Err`.

## `repo-service`, Tauri, sidecar, `RepoClient`

- New read command `Command::GetCommitSignature { commit_id, reply }` → `verify_commit`.
- Tauri command `get_commit_signature` and sidecar method `getCommitSignature`, each with a DTO.
  Enums serialize as camelCase strings (`"goodUntrusted"`, `"openPgp"` → `"openpgp"`, `"ssh"`).
- `RepoClient`:

```ts
export type SignatureStatus =
  | "unsigned" | "good" | "goodUntrusted" | "bad" | "unknownKey"
  | "expired" | "expiredKey" | "revokedKey" | "error";

export interface CommitSignature {
  status: SignatureStatus;
  format: "openpgp" | "ssh" | null;
  signer: string | null;
  key: string | null;
  detail: string | null;
}

getCommitSignature(repoPath: string, commitId: string): Promise<CommitSignature>;
```

- Signing needs no new IPC: commit, amend, cherry-pick, revert and rebase keep their current
  commands and return the `SignError` text on failure.

## Frontend

- `CommitHeader` fetches `getCommitSignature` alongside `getCommitMessage`, using the same
  ignore-on-unmount pattern. A fetch error hides the row; it is a nicety, like the body.
- "Signature" row in the `<dl>`, after Date:
  - `unsigned`: no row.
  - Loading: "Checking signature…".
  - Otherwise: a pill with an icon and text (never colour alone), then the signer and a short key
    (last 16 hex chars, or `SHA256:` + first 12). The full key and `detail` go in `title`.

| Status | Tone | Text |
|---|---|---|
| `good` | success | Good signature |
| `goodUntrusted` | warning | Valid signature, signer not trusted |
| `unknownKey` | warning | Signed with an unknown key |
| `expired` | warning | Signature expired |
| `expiredKey` | warning | Signed with an expired key |
| `bad` | danger | Bad signature |
| `revokedKey` | danger | Signed with a revoked key |
| `error` | neutral | Signature could not be checked |

- Tones use existing theme tokens. No new colors outside `CommitHeader.module.css`.
- Signing errors use the surfaces each operation already has (CommitBox error text, rebase/pick
  error toasts). Nothing new.

## Testing

- `git-core` (`tests/signing.rs`), real temp-dir repos and throwaway keys:
  - ssh: `ssh-keygen -t ed25519 -N "" -f <tmp>/key`, `user.signingKey=<tmp>/key.pub`, allowed
    signers file in the temp dir.
  - openpgp: one shared keyring per test binary. A `OnceLock` fixture creates a temp dir, sets
    `GNUPGHOME` once before any test spawns gpg, and runs
    `gpg --batch --passphrase '' --quick-gen-key` plus ultimate trust. No production seam is needed.
  - A test whose program is not on PATH prints a skip reason and returns. That is expected on
    Windows CI.
  - Cases:
    - `commit`, `amend_head`, cherry-pick, revert, rebase step and squash collapse each produce a
      signed commit with `commit.gpgSign=true` and an unsigned one by default.
    - The reflog message is identical, signed or unsigned.
    - A signer failure (wrong program path, or a nonexistent key) leaves HEAD, refs and the index
      untouched and returns the right `SignError`.
    - `gpg.format=x509` gives `UnsupportedFormat`; ssh without a key gives `MissingSshKey`.
    - Verify: `Unsigned`; `Good` (gpg ultimately-trusted own key, ssh in allowed signers);
      `GoodUntrusted` (ssh without allowed signers); `Bad` (signature over a tampered buffer
      written with `commit_signed`); `UnknownKey` (buffer signed by the test with
      `gpg --homedir <second tmp keyring>`, then written with `commit_signed`); `Error`
      (`gpg.program` set to a nonexistent path).
- `repo-service`: worker test for `GetCommitSignature` (ssh fixture, skipped without `ssh-keygen`)
  and a test that a signing failure inside `Commit` replies `Err` and records no undo entry.
- `tauri-app`: `CommitSignature` DTO serialization.
- Frontend (mock `RepoClient`): `CommitHeader` renders one case per tone, hides the row for
  `unsigned` and on fetch error, and shows the loading text.
- E2E Tauri: the fixture repo is configured with a throwaway ssh key and allowed signers. The
  flow commits through the UI, selects the commit and sees "Good signature". VSCode e2e: the same
  flow, reusing the fixture helper.

## Docs

- `CHANGELOG.md` entry.
- `docs/USER_GUIDE.md`: "Signing commits" section (which config keys are honored, agent/pinentry
  note, what each status means).
- `docs/ARCHITECTURE.md`: "Commit signing" subsection (single `create_commit` entry point,
  signer subprocess contract, verification mapping).
- `docs/LICENSE_COMPLIANCE.md`: `tempfile` row also lists it as a runtime dependency of `git-core`.

## Out of scope

- x509 / `gpgsm`.
- `gpg.ssh.defaultKeyCommand`, `gpg.ssh.revocationFile`.
- Signing tags (`tag.gpgSign`) and showing tag signatures.
- Per-commit sign toggle, settings UI.
- Signature status in graph rows, or a cache for it.
- Signing pushes (`push.gpgSign`).
