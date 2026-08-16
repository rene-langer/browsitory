use std::path::{Component, Path};

use git2::Repository;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SubmoduleError {
    #[error("submodule was not found")]
    NotFound,
    #[error("submodule path is invalid")]
    InvalidPath,
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmoduleInfo {
    pub path: String,
    pub url: Option<String>,
    pub gitlink_id: Option<String>,
    pub initialized: bool,
    pub head_id: Option<String>,
}

pub fn list_submodules(repo: &Repository) -> Result<Vec<SubmoduleInfo>, SubmoduleError> {
    let config = repo.config()?;
    repo.submodules()?
        .into_iter()
        .map(|submodule| {
            let name = submodule.name()?;
            Ok(SubmoduleInfo {
                path: submodule.path().to_string_lossy().into_owned(),
                url: submodule.url()?.map(str::to_string),
                gitlink_id: submodule.head_id().map(|id| id.to_string()),
                initialized: config.get_string(&format!("submodule.{name}.url")).is_ok(),
                head_id: submodule.workdir_id().map(|id| id.to_string()),
            })
        })
        .collect()
}

pub fn init_submodule(repo: &Repository, path: &str) -> Result<(), SubmoduleError> {
    find_submodule(repo, path)?.init(false)?;
    Ok(())
}

pub fn update_submodule(
    repo: &Repository,
    path: &str,
    recursive: bool,
) -> Result<(), SubmoduleError> {
    let mut submodule = find_submodule(repo, path)?;
    submodule.update(false, None)?;

    if recursive {
        update_nested_submodules(&submodule.open()?)?;
    }

    Ok(())
}

fn find_submodule<'repo>(
    repo: &'repo Repository,
    path: &str,
) -> Result<git2::Submodule<'repo>, SubmoduleError> {
    validate_path(path)?;
    repo.submodules()?
        .into_iter()
        .find(|submodule| submodule.path().to_str() == Some(path))
        .ok_or(SubmoduleError::NotFound)
}

fn update_nested_submodules(repo: &Repository) -> Result<(), SubmoduleError> {
    for mut submodule in repo.submodules()? {
        submodule.update(false, None)?;
        update_nested_submodules(&submodule.open()?)?;
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<(), SubmoduleError> {
    if path.is_empty()
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(SubmoduleError::InvalidPath);
    }
    Ok(())
}
