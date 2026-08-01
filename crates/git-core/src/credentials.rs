use git2::{Cred, CredentialType, RemoteCallbacks};

/// Builds a `RemoteCallbacks` wired for non-interactive authentication —
/// this is a GUI app with no controlling terminal, so credential resolution
/// must never block on a terminal prompt.
///
/// Tries, per libgit2's repeated calls to the `credentials` callback (it
/// retries with a different `allowed_types` bitflag as earlier attempts
/// fail): SSH agent first if the server offers `SSH_KEY` auth, then the
/// platform credential helper (`credential.helper` in gitconfig — this is
/// git2's built-in bridge to osxkeychain/wincred/git-credential-manager/etc,
/// not a new dependency) if the server offers plaintext username/password,
/// then `Cred::default()` (NTLM/Negotiate) as a last resort. Returns `Err`
/// once none of these apply rather than falling through to any interactive
/// path — libgit2 gives up and surfaces the error to the caller when the
/// callback errors, which is exactly what we want here.
pub fn make_callbacks<'a>(repo: &'a git2::Repository) -> RemoteCallbacks<'a> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |url, username_from_url, allowed_types| {
        if allowed_types.contains(CredentialType::SSH_KEY)
            && let Ok(cred) = Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"))
        {
            return Ok(cred);
        }
        if allowed_types.contains(CredentialType::USER_PASS_PLAINTEXT)
            && let Ok(config) = repo.config()
            && let Ok(cred) = Cred::credential_helper(&config, url, username_from_url)
        {
            return Ok(cred);
        }
        Cred::default()
    });
    // Explicitly pass through libgit2's own certificate validation rather
    // than silently accepting invalid certs — `CertificateCheckStatus::
    // CertificatePassthrough` defers to whatever check libgit2 already ran.
    callbacks.certificate_check(|_cert, _valid| {
        Ok(git2::CertificateCheckStatus::CertificatePassthrough)
    });
    callbacks
}
