use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Deserialize(#[from] toml::de::Error),
    #[error(transparent)]
    Serialize(#[from] toml::ser::Error),
    #[error("could not determine the OS config directory")]
    NoConfigDir,
}

pub type Result<T> = std::result::Result<T, ConfigError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Preferences {
    #[serde(default)]
    pub theme: Theme,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OnDisk {
    #[serde(default)]
    repos: Vec<PathBuf>,
    #[serde(default)]
    preferences: Preferences,
}

/// The list of repositories the user has opened, plus app preferences,
/// persisted as a single TOML file in the OS config directory. This replaces
/// the old browser build's IndexedDB-backed repository registry — a native
/// app just writes a real file.
pub struct ConfigStore {
    path: PathBuf,
    data: OnDisk,
}

impl ConfigStore {
    pub fn load() -> Result<Self> {
        Self::load_from(default_config_path()?)
    }

    pub fn load_from(path: PathBuf) -> Result<Self> {
        let data = if path.exists() {
            toml::from_str(&fs::read_to_string(&path)?)?
        } else {
            OnDisk::default()
        };
        Ok(Self { path, data })
    }

    pub fn repos(&self) -> &[PathBuf] {
        &self.data.repos
    }

    pub fn preferences(&self) -> &Preferences {
        &self.data.preferences
    }

    /// No-op (and no disk write) if the path is already registered.
    pub fn add_repo(&mut self, repo_path: PathBuf) -> Result<()> {
        if !self.data.repos.contains(&repo_path) {
            self.data.repos.push(repo_path);
            self.save()?;
        }
        Ok(())
    }

    pub fn remove_repo(&mut self, repo_path: &Path) -> Result<()> {
        self.data.repos.retain(|p| p != repo_path);
        self.save()
    }

    pub fn set_preferences(&mut self, preferences: Preferences) -> Result<()> {
        self.data.preferences = preferences;
        self.save()
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, toml::to_string_pretty(&self.data)?)?;
        Ok(())
    }
}

fn default_config_path() -> Result<PathBuf> {
    let dirs =
        directories::ProjectDirs::from("", "", "browsitory").ok_or(ConfigError::NoConfigDir)?;
    Ok(dirs.config_dir().join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, ConfigStore) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let store = ConfigStore::load_from(path).unwrap();
        (dir, store)
    }

    #[test]
    fn starts_empty_when_no_file_exists_yet() {
        let (_dir, store) = temp_store();
        assert!(store.repos().is_empty());
        assert_eq!(store.preferences().theme, Theme::System);
    }

    #[test]
    fn add_repo_persists_across_reloads() {
        let (_dir, mut store) = temp_store();
        let path = store.path.clone();
        store.add_repo(PathBuf::from("/repos/one")).unwrap();

        let reloaded = ConfigStore::load_from(path).unwrap();
        assert_eq!(reloaded.repos(), &[PathBuf::from("/repos/one")]);
    }

    #[test]
    fn add_repo_is_idempotent() {
        let (_dir, mut store) = temp_store();
        store.add_repo(PathBuf::from("/repos/one")).unwrap();
        store.add_repo(PathBuf::from("/repos/one")).unwrap();
        assert_eq!(store.repos().len(), 1);
    }

    #[test]
    fn remove_repo_drops_it() {
        let (_dir, mut store) = temp_store();
        store.add_repo(PathBuf::from("/repos/one")).unwrap();
        store.remove_repo(Path::new("/repos/one")).unwrap();
        assert!(store.repos().is_empty());
    }

    #[test]
    fn preferences_round_trip() {
        let (_dir, mut store) = temp_store();
        let path = store.path.clone();
        store
            .set_preferences(Preferences { theme: Theme::Dark })
            .unwrap();

        let reloaded = ConfigStore::load_from(path).unwrap();
        assert_eq!(reloaded.preferences().theme, Theme::Dark);
    }
}
