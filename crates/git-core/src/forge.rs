use git2::Repository;
use thiserror::Error;
use url::Url;

/// A forge (GitHub/Bitbucket Cloud) host that pull-request integration can talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeProvider {
    GitHub,
    Bitbucket,
}

impl ForgeProvider {
    fn from_host(host: &str) -> Option<Self> {
        match host {
            "github.com" => Some(Self::GitHub),
            "bitbucket.org" => Some(Self::Bitbucket),
            _ => None,
        }
    }
}

/// A repository identity resolved from one of the local repo's remotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeRepository {
    pub provider: ForgeProvider,
    pub host: String,
    pub owner: String,
    pub name: String,
    pub remote_name: String,
}

#[derive(Debug, Error)]
pub enum ForgeError {
    #[error("git remote operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("remote URLs must not contain embedded credentials")]
    CredentialBearingUrl,
    #[error(
        "remote '{remote_name}' points at a supported forge host but its repository path is ambiguous"
    )]
    AmbiguousRemote { remote_name: String },
}

/// Enumerates the repo's remotes and resolves the ones that point at a supported forge
/// (GitHub or Bitbucket Cloud) to an explicit `owner`/`name` identity.
///
/// Remotes on unrecognized hosts are silently skipped (no HTTP request is ever implied by
/// this function — it's pure parsing). Remotes on a recognized forge host whose path can't be
/// read as exactly `owner/name`, and remotes whose URL carries embedded `user:pass`
/// credentials, are reported as errors rather than silently dropped or silently trusted.
pub fn detect_forge_repositories(repo: &Repository) -> Result<Vec<ForgeRepository>, ForgeError> {
    let mut repositories = Vec::new();
    for name in repo.remotes()?.iter().flatten().flatten() {
        let remote = repo.find_remote(name)?;
        let url = remote.url()?;
        match classify_remote_url(url) {
            Ok(Some(identity)) => repositories.push(ForgeRepository {
                provider: identity.provider,
                host: identity.host,
                owner: identity.owner,
                name: identity.name,
                remote_name: name.to_string(),
            }),
            Ok(None) => {}
            Err(ClassifyError::CredentialBearingUrl) => {
                return Err(ForgeError::CredentialBearingUrl)
            }
            Err(ClassifyError::AmbiguousPath) => {
                return Err(ForgeError::AmbiguousRemote {
                    remote_name: name.to_string(),
                })
            }
        }
    }
    Ok(repositories)
}

struct ForgeIdentity {
    provider: ForgeProvider,
    host: String,
    owner: String,
    name: String,
}

enum ClassifyError {
    CredentialBearingUrl,
    AmbiguousPath,
}

fn classify_remote_url(url: &str) -> Result<Option<ForgeIdentity>, ClassifyError> {
    if url.contains("://") {
        classify_scheme_url(url)
    } else if let Some((userinfo, host, path)) = split_scp_like(url) {
        if userinfo.is_some_and(|info| info.contains(':')) {
            return Err(ClassifyError::CredentialBearingUrl);
        }
        classify_host_and_path(&host.to_ascii_lowercase(), path)
    } else {
        Ok(None)
    }
}

fn classify_scheme_url(url: &str) -> Result<Option<ForgeIdentity>, ClassifyError> {
    let Ok(parsed) = Url::parse(url) else {
        return Ok(None);
    };
    if matches!(parsed.scheme(), "http" | "https") && parsed.password().is_some() {
        return Err(ClassifyError::CredentialBearingUrl);
    }
    let Some(host) = parsed.host_str() else {
        return Ok(None);
    };
    classify_host_and_path(&host.to_ascii_lowercase(), parsed.path())
}

/// Splits owner/name out of `path`, stripping only a terminal `.git` off the last segment,
/// and requires exactly two non-empty segments once `host` is a recognized forge host.
fn classify_host_and_path(host: &str, path: &str) -> Result<Option<ForgeIdentity>, ClassifyError> {
    let Some(provider) = ForgeProvider::from_host(host) else {
        return Ok(None);
    };
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    let (Some(owner), Some(name), None) = (segments.next(), segments.next(), segments.next())
    else {
        return Err(ClassifyError::AmbiguousPath);
    };
    let name = name.strip_suffix(".git").unwrap_or(name);
    if owner.is_empty() || name.is_empty() {
        return Err(ClassifyError::AmbiguousPath);
    }
    Ok(Some(ForgeIdentity {
        provider,
        host: host.to_string(),
        owner: owner.to_string(),
        name: name.to_string(),
    }))
}

/// Parses git's scp-like syntax (`[user@]host:path`, as opposed to a schemed URL). Returns
/// `None` for anything that doesn't fit the shape (e.g. a plain local filesystem path), since
/// that's simply not a forge remote rather than a parse error.
fn split_scp_like(url: &str) -> Option<(Option<&str>, &str, &str)> {
    let (userinfo, rest) = match url.rsplit_once('@') {
        Some((user, rest)) => (Some(user), rest),
        None => (None, url),
    };
    let (host, path) = rest.split_once(':')?;
    if host.is_empty() || path.is_empty() || host.contains('/') {
        return None;
    }
    Some((userinfo, host, path))
}
