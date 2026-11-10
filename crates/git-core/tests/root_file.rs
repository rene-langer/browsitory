mod common;

use std::path::Path;

use common::{init_repo, write_file};
use git_core::root_file::{create_root_file, existing_root_files, RootFileError};

fn contents(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

#[test]
fn creates_a_missing_gitignore_empty_at_the_root() {
    let (dir, _repo) = init_repo();
    assert!(existing_root_files(dir.path()).is_empty());
    create_root_file(dir.path(), ".gitignore").unwrap();
    assert_eq!(contents(dir.path(), ".gitignore"), "");
    assert_eq!(existing_root_files(dir.path()), [".gitignore"]);
}

#[test]
fn creates_a_missing_gitattributes() {
    let (dir, _repo) = init_repo();
    create_root_file(dir.path(), ".gitattributes").unwrap();
    assert!(dir.path().join(".gitattributes").is_file());
    assert_eq!(existing_root_files(dir.path()), [".gitattributes"]);
}

#[test]
fn lists_both_files_in_menu_order() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), ".gitattributes", "* text=auto\n");
    write_file(dir.path(), ".gitignore", "*.log\n");
    assert_eq!(
        existing_root_files(dir.path()),
        [".gitignore", ".gitattributes"]
    );
}

#[test]
fn leaves_an_existing_file_untouched() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), ".gitignore", "*.log\n");
    create_root_file(dir.path(), ".gitignore").unwrap();
    assert_eq!(contents(dir.path(), ".gitignore"), "*.log\n");
}

#[test]
fn refuses_any_other_name_or_path() {
    let (dir, _repo) = init_repo();
    for bad in [
        "",
        "README.md",
        ".git/config",
        "sub/.gitignore",
        "../.gitignore",
        "./.gitignore",
        ".gitignore/",
        ".GITIGNORE",
        ".gitmodules",
    ] {
        assert!(
            matches!(
                create_root_file(dir.path(), bad),
                Err(RootFileError::NotAllowed)
            ),
            "{bad:?}"
        );
    }
    assert!(!dir.path().join("README.md").exists());
    assert!(!dir.path().join("sub").exists());
}

#[test]
fn refuses_a_folder_of_that_name() {
    let (dir, _repo) = init_repo();
    std::fs::create_dir(dir.path().join(".gitattributes")).unwrap();
    assert!(create_root_file(dir.path(), ".gitattributes").is_err());
    assert!(dir.path().join(".gitattributes").is_dir());
}

#[cfg(unix)]
#[test]
fn does_not_follow_a_symlink_out_of_the_work_tree() {
    let (dir, _repo) = init_repo();
    let outside = common::outside_dir();
    let target = outside.path().join("created-through-link");
    std::os::unix::fs::symlink(&target, dir.path().join(".gitignore")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("target.txt"),
        dir.path().join(".gitattributes"),
    )
    .unwrap();
    // A dangling link: nothing is created at its target.
    assert!(matches!(
        create_root_file(dir.path(), ".gitignore"),
        Err(RootFileError::NotAFile(_))
    ));
    assert!(!target.exists());
    // A link to an existing file: refused, and the file is left as it was.
    assert!(matches!(
        create_root_file(dir.path(), ".gitattributes"),
        Err(RootFileError::NotAFile(_))
    ));
    common::assert_outside_untouched(outside.path());
}
