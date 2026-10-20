//! Commit signing (OpenPGP via gpg, SSH via ssh-keygen) and signature verification. See
//! `docs/superpowers/specs/2026-10-19-sm-parity-batch3c-design.md`.

use std::io::Write;
use std::process::{Command, Stdio};

use git2::{Commit, Oid, Repository, Signature, Tree};
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
