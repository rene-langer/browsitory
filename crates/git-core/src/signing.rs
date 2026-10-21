//! Commit signing (OpenPGP via gpg, SSH via ssh-keygen) and signature verification. See
//! `docs/superpowers/specs/2026-10-19-parity-batch3c-design.md`.

use std::io::Write;
use std::process::{Command, Stdio};

use git2::{Commit, ErrorCode, Oid, Repository, Signature, Tree};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureFormat {
    OpenPgp,
    Ssh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningConfig {
    pub format: SignatureFormat,
    pub program: String,
    pub key: Option<String>,
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
    SignerFailed(String),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

fn config_string(config: &git2::Config, key: &str) -> Option<String> {
    config
        .get_string(key)
        .ok()
        .filter(|value| !value.is_empty())
}

/// The program that signs/verifies `format`, honoring the same config precedence as git.
pub(crate) fn program_for(config: &git2::Config, format: SignatureFormat) -> String {
    match format {
        SignatureFormat::OpenPgp => config_string(config, "gpg.openpgp.program")
            .or_else(|| config_string(config, "gpg.program"))
            .unwrap_or_else(|| "gpg".to_string()),
        SignatureFormat::Ssh => {
            config_string(config, "gpg.ssh.program").unwrap_or_else(|| "ssh-keygen".to_string())
        }
    }
}

/// `Ok(None)` when `commit.gpgSign` is false or unset.
pub fn signing_config(repo: &Repository) -> Result<Option<SigningConfig>, SignError> {
    let config = repo.config()?;
    if !config.get_bool("commit.gpgSign").unwrap_or(false) {
        return Ok(None);
    }
    let format = match config_string(&config, "gpg.format").as_deref() {
        None | Some("openpgp") => SignatureFormat::OpenPgp,
        Some("ssh") => SignatureFormat::Ssh,
        Some(other) => return Err(SignError::UnsupportedFormat(other.to_string())),
    };
    let key = config_string(&config, "user.signingKey");
    if format == SignatureFormat::Ssh && key.is_none() {
        return Err(SignError::MissingSshKey);
    }
    Ok(Some(SigningConfig {
        format,
        program: program_for(&config, format),
        key,
    }))
}

pub(crate) struct ProgramOutput {
    pub status_success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `program args…` with `stdin` piped in and stdout/stderr captured; no TTY, no console
/// window on Windows. Stdin is written from a separate thread so a signer that writes a lot to
/// stderr before reading all of stdin cannot deadlock against us.
pub(crate) fn run_program(
    program: &str,
    args: &[&str],
    stdin: &[u8],
) -> Result<ProgramOutput, SignError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            SignError::SignerNotFound(program.to_string())
        } else {
            SignError::Io(error)
        }
    })?;
    let mut child_stdin = child.stdin.take().expect("stdin is piped");
    let input = stdin.to_vec();
    let writer = std::thread::spawn(move || {
        // A signer that exits early closes its end; that is reported through its exit status.
        let _ = child_stdin.write_all(&input);
    });
    let output = child.wait_with_output()?;
    let _ = writer.join();
    Ok(ProgramOutput {
        status_success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Last few non-empty stderr lines, without gpg's `[GNUPG:]` status lines.
fn stderr_tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("[GNUPG:]"))
        .collect();
    let start = lines.len().saturating_sub(3);
    let tail = lines[start..].join("\n");
    if tail.is_empty() {
        "the signing program exited with an error".to_string()
    } else {
        tail
    }
}

fn is_literal_ssh_key(key: &str) -> Option<&str> {
    if let Some(rest) = key.strip_prefix("key::") {
        Some(rest)
    } else if key.starts_with("ssh-") {
        Some(key)
    } else {
        None
    }
}

fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
        if let Some(home) = home {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

/// Detached, ASCII-armored signature over `buffer`. `committer` supplies gpg's default key
/// (`Name <email>`) when `user.signingKey` is unset, as git does.
pub fn sign_buffer(
    config: &SigningConfig,
    committer: &Signature<'_>,
    buffer: &[u8],
) -> Result<String, SignError> {
    match config.format {
        SignatureFormat::OpenPgp => {
            let key = config.key.clone().unwrap_or_else(|| {
                format!(
                    "{} <{}>",
                    committer.name().unwrap_or_default(),
                    committer.email().unwrap_or_default()
                )
            });
            let output = run_program(&config.program, &["--status-fd=2", "-bsau", &key], buffer)?;
            if !output.status_success || !output.stderr.contains("[GNUPG:] SIG_CREATED ") {
                return Err(SignError::SignerFailed(stderr_tail(&output.stderr)));
            }
            Ok(output.stdout)
        }
        SignatureFormat::Ssh => {
            let key = config.key.as_deref().ok_or(SignError::MissingSshKey)?;
            // A literal public key is written to a temp file; ssh-keygen then signs with the
            // matching private key from the agent (`-U`), exactly like git.
            let mut _literal_file: Option<tempfile::NamedTempFile> = None;
            let (key_path, use_agent) = match is_literal_ssh_key(key) {
                Some(literal) => {
                    let mut file = tempfile::NamedTempFile::new()?;
                    writeln!(file, "{literal}")?;
                    let path = file.path().to_path_buf();
                    _literal_file = Some(file);
                    (path, true)
                }
                None => (expand_tilde(key), false),
            };
            let key_arg = key_path.to_string_lossy().into_owned();
            let mut args = vec!["-Y", "sign", "-n", "git", "-f", key_arg.as_str()];
            if use_agent {
                args.push("-U");
            }
            let output = run_program(&config.program, &args, buffer)?;
            if !output.status_success || !output.stdout.contains("-----BEGIN SSH SIGNATURE-----") {
                return Err(SignError::SignerFailed(stderr_tail(&output.stderr)));
            }
            Ok(output.stdout)
        }
    }
}

/// How `create_commit` moves HEAD after writing the commit object.
pub struct HeadUpdate<'a> {
    /// The commit HEAD must currently point at; `None` = HEAD's branch is unborn.
    pub expected: Option<Oid>,
    /// `"commit"` or `"commit (amend)"`; the reflog message is built from it like libgit2 does.
    pub reflog_operation: &'a str,
}

/// libgit2's `git_reference__update_for_commit` message: `<op><type>: <summary>`, where type is
/// ` (initial)` for a parentless commit and ` (merge)` for one with two or more parents.
fn reflog_message(operation: &str, commit: &Commit<'_>) -> String {
    let kind = match commit.parent_count() {
        0 => " (initial)",
        1 => "",
        _ => " (merge)",
    };
    format!(
        "{operation}{kind}: {}",
        commit.summary().ok().flatten().unwrap_or_default()
    )
}

fn move_head(repo: &Repository, oid: Oid, update: &HeadUpdate<'_>) -> Result<(), SignError> {
    let commit = repo.find_commit(oid)?;
    let message = reflog_message(update.reflog_operation, &commit);
    let head = repo.find_reference("HEAD")?;
    // Attached HEAD: move its branch (libgit2 appends HEAD's reflog too). Detached: move HEAD.
    let target_name = match head.symbolic_target() {
        Ok(Some(branch)) => branch.to_string(),
        _ => "HEAD".to_string(),
    };
    match update.expected {
        Some(expected) => {
            repo.reference_matching(&target_name, oid, true, expected, &message)?;
        }
        None => {
            repo.reference(&target_name, oid, false, &message)?;
        }
    }
    Ok(())
}

/// The single entry point for every commit `git-core` creates for the user. Writes the commit
/// object (signed when `commit.gpgSign` asks for it), then, when `head_update` is set, moves HEAD.
/// The signer runs before any ref moves, so a signing failure changes nothing.
#[allow(clippy::too_many_arguments)]
pub fn create_commit(
    repo: &Repository,
    head_update: Option<HeadUpdate<'_>>,
    author: &Signature<'_>,
    committer: &Signature<'_>,
    message: &str,
    tree: &Tree<'_>,
    parents: &[&Commit<'_>],
) -> Result<Oid, SignError> {
    let oid = match signing_config(repo)? {
        None => repo.commit(None, author, committer, message, tree, parents)?,
        Some(config) => {
            let buffer = repo.commit_create_buffer(author, committer, message, tree, parents)?;
            let signature = sign_buffer(&config, committer, &buffer)?;
            let content = std::str::from_utf8(&buffer).map_err(|_| {
                SignError::Git(git2::Error::from_str("commit buffer is not valid UTF-8"))
            })?;
            repo.commit_signed(content, &signature, None)?
        }
    };
    if let Some(update) = head_update {
        move_head(repo, oid, &update)?;
    }
    Ok(oid)
}

fn is_not_found(error: &git2::Error) -> bool {
    error.code() == ErrorCode::NotFound
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    Unsigned,
    Good,
    GoodUntrusted,
    Bad,
    UnknownKey,
    Expired,
    ExpiredKey,
    RevokedKey,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSignature {
    pub status: SignatureStatus,
    pub format: Option<SignatureFormat>,
    pub signer: Option<String>,
    pub key: Option<String>,
    pub detail: Option<String>,
}

impl CommitSignature {
    fn new(status: SignatureStatus, format: Option<SignatureFormat>) -> Self {
        Self {
            status,
            format,
            signer: None,
            key: None,
            detail: None,
        }
    }

    fn error(format: Option<SignatureFormat>, detail: impl Into<String>) -> Self {
        Self {
            detail: Some(detail.into()),
            ..Self::new(SignatureStatus::Error, format)
        }
    }
}

/// Signature status of `commit_id`. Only repository-level failures (bad id, unreadable object)
/// are `Err`; a missing or failing verifier program is an `Error` status.
pub fn verify_commit(repo: &Repository, commit_id: &str) -> Result<CommitSignature, SignError> {
    let oid = Oid::from_str(commit_id)?;
    repo.find_commit(oid)?;
    let (signature, data) = match repo.extract_signature(&oid, None) {
        Ok(pair) => pair,
        Err(error) if is_not_found(&error) => {
            return Ok(CommitSignature::new(SignatureStatus::Unsigned, None))
        }
        Err(error) => return Err(error.into()),
    };
    let signature = String::from_utf8_lossy(&signature).into_owned();
    let config = repo.config()?;
    let format = if signature.starts_with("-----BEGIN PGP SIGNATURE-----") {
        SignatureFormat::OpenPgp
    } else if signature.starts_with("-----BEGIN SSH SIGNATURE-----") {
        SignatureFormat::Ssh
    } else {
        return Ok(CommitSignature::error(None, "unsupported signature type"));
    };
    let program = program_for(&config, format);
    let mut sig_file = tempfile::NamedTempFile::new()?;
    sig_file.write_all(signature.as_bytes())?;
    sig_file.flush()?;
    let sig_path = sig_file.path().to_string_lossy().into_owned();
    let result = match format {
        SignatureFormat::OpenPgp => verify_gpg(&program, &sig_path, &data),
        SignatureFormat::Ssh => {
            let allowed = config_string(&config, "gpg.ssh.allowedSignersFile")
                .map(|path| expand_tilde(&path).to_string_lossy().into_owned());
            verify_ssh(&program, &sig_path, allowed.as_deref(), &data)
        }
    };
    match result {
        Ok(signature) => Ok(signature),
        Err(SignError::SignerNotFound(program)) => Ok(CommitSignature::error(
            Some(format),
            format!("{program} was not found"),
        )),
        Err(SignError::Io(error)) => Ok(CommitSignature::error(Some(format), error.to_string())),
        Err(other) => Err(other),
    }
}

fn verify_gpg(program: &str, sig_path: &str, data: &[u8]) -> Result<CommitSignature, SignError> {
    let output = run_program(program, &["--status-fd=1", "--verify", sig_path, "-"], data)?;
    Ok(parse_gpg_status(&output.stdout, &output.stderr))
}

/// Maps gpg `--status-fd` lines to a status, like git's `parse_gpg_output`.
pub(crate) fn parse_gpg_status(status: &str, stderr: &str) -> CommitSignature {
    let mut result = CommitSignature::new(SignatureStatus::Error, Some(SignatureFormat::OpenPgp));
    let mut trusted = false;
    let mut decided = false;
    for line in status.lines() {
        let Some(rest) = line.strip_prefix("[GNUPG:] ") else {
            continue;
        };
        let mut parts = rest.splitn(3, ' ');
        let keyword = parts.next().unwrap_or_default();
        let first = parts.next().map(str::to_string);
        let remainder = parts.next().map(str::to_string);
        let set = |result: &mut CommitSignature, status: SignatureStatus| {
            result.status = status;
            result.key = result.key.clone().or(first.clone());
            result.signer = remainder.clone();
        };
        match keyword {
            "GOODSIG" => {
                set(&mut result, SignatureStatus::Good);
                decided = true;
            }
            "BADSIG" => {
                set(&mut result, SignatureStatus::Bad);
                decided = true;
            }
            "EXPSIG" => {
                set(&mut result, SignatureStatus::Expired);
                decided = true;
            }
            "EXPKEYSIG" => {
                set(&mut result, SignatureStatus::ExpiredKey);
                decided = true;
            }
            "REVKEYSIG" => {
                set(&mut result, SignatureStatus::RevokedKey);
                decided = true;
            }
            "ERRSIG" if !decided => {
                result.key = first.clone();
            }
            "NO_PUBKEY" if !decided => {
                result.status = SignatureStatus::UnknownKey;
                result.key = result.key.clone().or(first.clone());
                decided = true;
            }
            // VALIDSIG <fingerprint> …: the full fingerprint beats GOODSIG's long key id.
            "VALIDSIG" => result.key = first.clone(),
            "TRUST_FULLY" | "TRUST_ULTIMATE" => trusted = true,
            _ => {}
        }
    }
    if result.status == SignatureStatus::Good && !trusted {
        result.status = SignatureStatus::GoodUntrusted;
        result.detail = Some("the signing key is not trusted in your keyring".to_string());
    }
    if !decided {
        result.detail = Some(stderr_tail(stderr));
    }
    result
}

fn ssh_fingerprint(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|word| word.starts_with("SHA256:"))
        .map(str::to_string)
}

/// A non-zero ssh-keygen exit is `Bad` only when it says the signature itself is wrong; any other
/// failure (old ssh-keygen without `-Y`, unreadable files, restricted entries) is `Error`.
fn ssh_failure(output: &str, stderr: &str, format: Option<SignatureFormat>) -> CommitSignature {
    if output.contains("Signature verification failed") || output.contains("incorrect signature") {
        CommitSignature::new(SignatureStatus::Bad, format)
    } else {
        CommitSignature::error(format, stderr_tail(stderr))
    }
}

fn verify_ssh(
    program: &str,
    sig_path: &str,
    allowed: Option<&str>,
    data: &[u8],
) -> Result<CommitSignature, SignError> {
    let format = Some(SignatureFormat::Ssh);
    if let Some(allowed) = allowed {
        let found = run_program(
            program,
            &["-Y", "find-principals", "-f", allowed, "-s", sig_path],
            &[],
        )?;
        let principal = found
            .stdout
            .lines()
            .next()
            .map(str::trim)
            .unwrap_or_default();
        if found.status_success && !principal.is_empty() {
            let verified = run_program(
                program,
                &[
                    "-Y", "verify", "-n", "git", "-f", allowed, "-I", principal, "-s", sig_path,
                ],
                data,
            )?;
            let text = format!("{}\n{}", verified.stdout, verified.stderr);
            let mut result = if verified.status_success {
                CommitSignature::new(SignatureStatus::Good, format)
            } else {
                ssh_failure(&text, &verified.stderr, format)
            };
            result.signer = Some(principal.to_string());
            result.key = ssh_fingerprint(&text);
            return Ok(result);
        }
    }
    let checked = run_program(
        program,
        &["-Y", "check-novalidate", "-n", "git", "-s", sig_path],
        data,
    )?;
    let text = format!("{}\n{}", checked.stdout, checked.stderr);
    let mut result = if checked.status_success {
        CommitSignature {
            detail: Some("signer is not in gpg.ssh.allowedSignersFile".to_string()),
            ..CommitSignature::new(SignatureStatus::GoodUntrusted, format)
        }
    } else {
        ssh_failure(&text, &checked.stderr, format)
    };
    result.key = ssh_fingerprint(&text);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpg_status_mapping() {
        let good = "[GNUPG:] GOODSIG AAAA Test User <t@e>\n[GNUPG:] VALIDSIG FPR123 2026-10-20\n[GNUPG:] TRUST_ULTIMATE 0 pgp\n";
        let parsed = parse_gpg_status(good, "");
        assert_eq!(parsed.status, SignatureStatus::Good);
        assert_eq!(parsed.signer.as_deref(), Some("Test User <t@e>"));
        assert_eq!(parsed.key.as_deref(), Some("FPR123"));

        let untrusted = "[GNUPG:] GOODSIG AAAA U <u@e>\n[GNUPG:] TRUST_UNDEFINED 0 pgp\n";
        assert_eq!(
            parse_gpg_status(untrusted, "").status,
            SignatureStatus::GoodUntrusted
        );

        let unknown = "[GNUPG:] ERRSIG BBBB 22 10 00 1700000000 9 -\n[GNUPG:] NO_PUBKEY BBBB\n";
        let parsed = parse_gpg_status(unknown, "");
        assert_eq!(parsed.status, SignatureStatus::UnknownKey);
        assert_eq!(parsed.key.as_deref(), Some("BBBB"));

        for (line, status) in [
            ("[GNUPG:] BADSIG A U <u@e>", SignatureStatus::Bad),
            ("[GNUPG:] EXPSIG A U <u@e>", SignatureStatus::Expired),
            ("[GNUPG:] EXPKEYSIG A U <u@e>", SignatureStatus::ExpiredKey),
            ("[GNUPG:] REVKEYSIG A U <u@e>", SignatureStatus::RevokedKey),
        ] {
            assert_eq!(parse_gpg_status(line, "").status, status, "{line}");
        }

        let nothing = parse_gpg_status("", "gpg: no valid OpenPGP data found.\n");
        assert_eq!(nothing.status, SignatureStatus::Error);
        assert_eq!(
            nothing.detail.as_deref(),
            Some("gpg: no valid OpenPGP data found.")
        );
    }
}
