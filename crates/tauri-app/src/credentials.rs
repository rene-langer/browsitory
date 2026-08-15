use std::fmt;

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
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::{CredentialKey, CredentialService, CredentialStore, CredentialStoreError};

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
    fn retains_a_non_default_https_port_in_the_key() {
        let key =
            CredentialKey::for_https("https://git.example.test:8443/org/repo.git", "rene").unwrap();

        assert_eq!(key.account, "https://git.example.test:8443/rene");
    }
}
