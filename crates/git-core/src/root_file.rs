//! The repository's own `.gitignore` and `.gitattributes` at the work tree root, which the tab
//! menu and the palette open in the editor (#624), and the one way to create one that is
//! missing. There is no general file creation: only these two names, only at the root.

use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;

use thiserror::Error;

/// The files `create_root_file` may create, in menu order.
pub const ROOT_FILES: [&str; 2] = [".gitignore", ".gitattributes"];

#[derive(Debug, Error)]
pub enum RootFileError {
    #[error("Only .gitignore and .gitattributes can be created.")]
    NotAllowed,
    #[error("{0} exists but isn't a regular file.")]
    NotAFile(String),
    #[error("Couldn't create {name}: {source}")]
    Io {
        name: String,
        #[source]
        source: std::io::Error,
    },
}

/// The `ROOT_FILES` that exist at the work tree root, in `ROOT_FILES` order. A symbolic link
/// counts, as it does for "Open in editor" (`launch::prepare_editor`).
pub fn existing_root_files(workdir: &Path) -> Vec<String> {
    ROOT_FILES
        .iter()
        .filter(|name| std::fs::symlink_metadata(workdir.join(name)).is_ok())
        .map(|name| name.to_string())
        .collect()
}

/// Creates an empty `name`, one of `ROOT_FILES`, at the work tree root. Never overwrites: an
/// existing regular file is left as it is (it may have appeared since the menu was shown). Never
/// follows a symbolic link: `File::create_new` (`O_EXCL`, `CREATE_NEW`) refuses any existing
/// entry, a dangling link included, and an existing link or folder is then refused.
pub fn create_root_file(workdir: &Path, name: &str) -> Result<(), RootFileError> {
    if !ROOT_FILES.contains(&name) {
        return Err(RootFileError::NotAllowed);
    }
    let path = workdir.join(name);
    let error = match File::create_new(&path) {
        Ok(_) => return Ok(()),
        Err(error) => error,
    };
    if error.kind() != ErrorKind::AlreadyExists {
        return Err(RootFileError::Io {
            name: name.to_string(),
            source: error,
        });
    }
    let is_file = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_file());
    if is_file {
        Ok(())
    } else {
        Err(RootFileError::NotAFile(name.to_string()))
    }
}
