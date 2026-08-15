use std::fmt;

use git_core::remote::{CredentialProvider, RemoteAuthMode, MISSING_CREDENTIAL_ERROR};
use url::Url;

const SERVICE_NAME: &str = "com.browsitory.git";
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CredentialKey {
    pub service: String,
    pub account: String,
}

impl CredentialKey {
    pub fn for_https(url: &str, username: &str) -> Result<Self, CredentialStoreError> {
        let parsed = Url::parse(url).map_err(|_| CredentialStoreError::InvalidHttpsUrl)?;
        if parsed.scheme() != "https"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(CredentialStoreError::InvalidHttpsUrl);
        }

        let host = parsed
            .host_str()
            .ok_or(CredentialStoreError::InvalidHttpsUrl)?;
        let port = match parsed.port() {
            Some(port) if port != 443 => format!(":{port}"),
            _ => String::new(),
        };

        Ok(Self {
            service: SERVICE_NAME.to_owned(),
            account: format!("https://{host}{port}/{username}"),
        })
    }
}

pub struct HttpsCredential {
    pub username: String,
    pub token: String,
}

#[derive(Debug)]
pub enum CredentialStoreError {
    InvalidHttpsUrl,
    Keychain,
}

impl fmt::Display for CredentialStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHttpsUrl => formatter.write_str("expected an HTTPS URL without user info"),
            Self::Keychain => {
                formatter.write_str("the operating-system credential store is unavailable")
            }
        }
    }
}

impl std::error::Error for CredentialStoreError {}

pub trait CredentialStore {
    fn get(&self, key: &CredentialKey) -> Result<Option<String>, CredentialStoreError>;
    fn set(&self, key: &CredentialKey, token: &str) -> Result<(), CredentialStoreError>;
    fn delete(&self, key: &CredentialKey) -> Result<(), CredentialStoreError>;
}

pub struct KeyringCredentialStore;

impl KeyringCredentialStore {
    fn entry(&self, key: &CredentialKey) -> Result<keyring::Entry, CredentialStoreError> {
        keyring::Entry::new(&key.service, &key.account).map_err(|_| CredentialStoreError::Keychain)
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn get(&self, key: &CredentialKey) -> Result<Option<String>, CredentialStoreError> {
        match self.entry(key)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(CredentialStoreError::Keychain),
        }
    }

    fn set(&self, key: &CredentialKey, token: &str) -> Result<(), CredentialStoreError> {
        self.entry(key)?
            .set_password(token)
            .map_err(|_| CredentialStoreError::Keychain)
    }

    fn delete(&self, key: &CredentialKey) -> Result<(), CredentialStoreError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(CredentialStoreError::Keychain),
        }
    }
}

pub struct CredentialService<S: CredentialStore> {
    store: S,
}

impl<S: CredentialStore> CredentialService<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn save_https(
        &self,
        url: &str,
        username: &str,
        token: &str,
    ) -> Result<(), CredentialStoreError> {
        let key = CredentialKey::for_https(url, username)?;
        self.store.set(&key, token)
    }

    pub fn lookup_https(
        &self,
        url: &str,
        username: Option<&str>,
    ) -> Result<Option<HttpsCredential>, CredentialStoreError> {
        let Some(username) = username else {
            return Ok(None);
        };
        let key = CredentialKey::for_https(url, username)?;
        let Some(token) = self.store.get(&key)? else {
            return Ok(None);
        };

        Ok(Some(HttpsCredential {
            username: username.to_owned(),
            token,
        }))
    }

    pub fn forget_https(&self, url: &str, username: &str) -> Result<(), CredentialStoreError> {
        let key = CredentialKey::for_https(url, username)?;
        self.store.delete(&key)
    }
}

pub(crate) trait SshAgent {
    fn credential(&self, username: &str) -> Result<git2::Cred, git2::Error>;
}

pub(crate) struct Libgit2SshAgent;

impl SshAgent for Libgit2SshAgent {
    fn credential(&self, username: &str) -> Result<git2::Cred, git2::Error> {
        git2::Cred::ssh_key_from_agent(username)
    }
}

impl<T: SshAgent + ?Sized> SshAgent for &T {
    fn credential(&self, username: &str) -> Result<git2::Cred, git2::Error> {
        (*self).credential(username)
    }
}

pub(crate) struct RemoteCredentialProvider<'a, S: CredentialStore, A = Libgit2SshAgent> {
    service: &'a CredentialService<S>,
    profile: Option<RemoteAuthMode>,
    ssh_agent: A,
}

impl<'a, S: CredentialStore> RemoteCredentialProvider<'a, S, Libgit2SshAgent> {
    pub(crate) fn new(service: &'a CredentialService<S>, profile: Option<RemoteAuthMode>) -> Self {
        Self {
            service,
            profile,
            ssh_agent: Libgit2SshAgent,
        }
    }
}

impl<'a, S: CredentialStore, A: SshAgent> RemoteCredentialProvider<'a, S, A> {
    #[cfg(test)]
    fn with_ssh_agent(
        service: &'a CredentialService<S>,
        profile: Option<RemoteAuthMode>,
        ssh_agent: A,
    ) -> Self {
        Self {
            service,
            profile,
            ssh_agent,
        }
    }
}

impl<S: CredentialStore, A: SshAgent> CredentialProvider for RemoteCredentialProvider<'_, S, A> {
    fn credential(
        &mut self,
        url: &str,
        username: Option<&str>,
        _allowed: git2::CredentialType,
    ) -> Result<git2::Cred, git2::Error> {
        match &self.profile {
            Some(RemoteAuthMode::HttpsToken { username }) => {
                let credential = self
                    .service
                    .lookup_https(url, Some(username))
                    .map_err(|_| git2::Error::from_str(MISSING_CREDENTIAL_ERROR))?
                    .ok_or_else(|| git2::Error::from_str(MISSING_CREDENTIAL_ERROR))?;
                git2::Cred::userpass_plaintext(&credential.username, &credential.token)
            }
            Some(RemoteAuthMode::SshAgent) => self.ssh_agent.credential(username.unwrap_or("git")),
            None if _allowed.is_ssh_key() => self.ssh_agent.credential(username.unwrap_or("git")),
            None => Err(git2::Error::from_str(MISSING_CREDENTIAL_ERROR)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use git_core::remote::{CredentialProvider, RemoteAuthMode};

    use super::{
        CredentialKey, CredentialService, CredentialStore, CredentialStoreError,
        RemoteCredentialProvider, MISSING_CREDENTIAL_ERROR,
    };

    #[derive(Default)]
    struct MemoryCredentialStore {
        tokens: RefCell<HashMap<CredentialKey, String>>,
    }

    impl CredentialStore for MemoryCredentialStore {
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

    #[derive(Default)]
    struct PanicOnGetStore;

    impl CredentialStore for PanicOnGetStore {
        fn get(&self, _key: &CredentialKey) -> Result<Option<String>, CredentialStoreError> {
            panic!("SSH authentication must not query the credential store");
        }

        fn set(&self, _key: &CredentialKey, _token: &str) -> Result<(), CredentialStoreError> {
            unreachable!("test store is read-only")
        }

        fn delete(&self, _key: &CredentialKey) -> Result<(), CredentialStoreError> {
            unreachable!("test store is read-only")
        }
    }

    #[derive(Default)]
    struct RecordingSshAgent {
        usernames: RefCell<Vec<String>>,
    }

    impl super::SshAgent for RecordingSshAgent {
        fn credential(&self, username: &str) -> Result<git2::Cred, git2::Error> {
            self.usernames.borrow_mut().push(username.to_owned());
            Err(git2::Error::from_str("test SSH agent was invoked"))
        }
    }

    #[test]
    fn saves_reads_and_forgets_an_https_token_using_a_non_secret_key() {
        let store = MemoryCredentialStore::default();
        let service = CredentialService::new(store);
        let remote_url = "https://git.example.test/org/repo.git";

        service.save_https(remote_url, "rene", "token-123").unwrap();

        let credential = service
            .lookup_https(remote_url, Some("rene"))
            .unwrap()
            .unwrap();
        assert_eq!(credential.username, "rene");
        assert_eq!(credential.token, "token-123");
        service.forget_https(remote_url, "rene").unwrap();
        assert!(service
            .lookup_https(remote_url, Some("rene"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn derives_a_key_without_the_default_https_port() {
        let key =
            CredentialKey::for_https("https://git.example.test:443/org/repo.git", "rene").unwrap();

        assert_eq!(key.service, "com.browsitory.git");
        assert_eq!(key.account, "https://git.example.test/rene");
    }

    #[test]
    fn derives_an_ipv6_key_without_the_default_https_port() {
        let key = CredentialKey::for_https("https://[::1]:443/org/repo.git", "rene").unwrap();

        assert_eq!(key.account, "https://[::1]/rene");
    }

    #[test]
    fn retains_brackets_once_for_an_ipv6_key_with_a_non_default_port() {
        let key =
            CredentialKey::for_https("https://[2001:db8::1]:8443/org/repo.git", "rene").unwrap();

        assert_eq!(key.account, "https://[2001:db8::1]:8443/rene");
    }

    #[test]
    fn retains_a_non_default_https_port_in_the_key() {
        let key =
            CredentialKey::for_https("https://git.example.test:8443/org/repo.git", "rene").unwrap();

        assert_eq!(key.account, "https://git.example.test:8443/rene");
    }

    #[test]
    fn https_provider_returns_a_plaintext_credential_only_when_the_store_has_a_token() {
        // Removing the lookup or passing a token through any configuration must fail this test.
        let store = MemoryCredentialStore::default();
        let service = CredentialService::new(store);
        let profile = RemoteAuthMode::HttpsToken {
            username: "rene".to_string(),
        };
        let mut provider = RemoteCredentialProvider::new(&service, Some(profile.clone()));

        let missing = match provider.credential(
            "https://git.example.test/owner/repo.git",
            Some("ignored-callback-user"),
            git2::CredentialType::USER_PASS_PLAINTEXT,
        ) {
            Ok(_) => panic!("missing token must reject authentication"),
            Err(error) => error,
        };
        assert_eq!(missing.message(), MISSING_CREDENTIAL_ERROR);

        service
            .save_https(
                "https://git.example.test/owner/repo.git",
                "rene",
                "token-123",
            )
            .unwrap();
        let credential = provider
            .credential(
                "https://git.example.test/owner/repo.git",
                Some("ignored-callback-user"),
                git2::CredentialType::USER_PASS_PLAINTEXT,
            )
            .expect("stored token supplies a libgit2 credential");
        assert!(credential.has_username());
    }

    #[test]
    fn ssh_provider_uses_the_callback_username_or_git_without_querying_the_store() {
        let store = PanicOnGetStore;
        let service = CredentialService::new(store);
        let profile = RemoteAuthMode::SshAgent;
        let agent = RecordingSshAgent::default();
        let mut provider =
            RemoteCredentialProvider::with_ssh_agent(&service, Some(profile), &agent);

        for username in [Some("alice"), None] {
            let error = match provider.credential(
                "ssh://example.test/owner/repo.git",
                username,
                git2::CredentialType::SSH_KEY,
            ) {
                Ok(_) => panic!("test agent always rejects after recording the username"),
                Err(error) => error,
            };
            assert_eq!(error.message(), "test SSH agent was invoked");
        }

        assert_eq!(agent.usernames.into_inner(), ["alice", "git"]);
    }
}
