//! #534: a fresh submodule clone whose commit holds a case pair, with a global
//! `core.ignorecase=true` applying to the new clone on a case-sensitive file system. libgit2's
//! ignore-case index kept only one of the two names and the other's file was never written.
//! The update is refused now (`CasePairBroughtIn`), leaving the clone with an empty work tree.
//!
//! One test only: it points libgit2's global config search path at its own `.gitconfig`, which
//! no other test in the same process may see.
#![cfg(target_os = "linux")]

mod common;

use common::{commit_all, init_repo, write_file};
use git2::ConfigLevel;

#[test]
fn fresh_submodule_update_refuses_a_commit_holding_a_case_pair() {
    let global = test_support::paths()
        .search_dir
        .parent()
        .unwrap()
        .join("submodule-ignorecase-global");
    std::fs::create_dir_all(&global).unwrap();
    std::fs::write(global.join(".gitconfig"), "[core]\n\tignorecase = true\n").unwrap();

    // Committed with the case folding off: the source repository's own config says so.
    let mut sources = Vec::new();
    for pair in [
        ("A", "a"),
        ("a", "A"),
        ("x/B", "x/b"),
        ("x/b", "x/B"),
        ("D/f", "d/f"),
    ] {
        let (dir, src) = init_repo();
        src.config()
            .unwrap()
            .set_bool("core.ignorecase", false)
            .unwrap();
        write_file(dir.path(), pair.0, "first\n");
        write_file(dir.path(), pair.1, "second\n");
        write_file(dir.path(), "conf", "base\n");
        commit_all(&src, "pair");
        let id = src.head().unwrap().target().unwrap();
        sources.push((pair, dir, id));
    }

    // SAFETY: this binary has exactly one test, so no other thread is using libgit2 while the
    // search path changes.
    unsafe {
        git2::opts::set_search_path(ConfigLevel::Global, &global).unwrap();
    }

    for (pair, src_dir, id) in &sources {
        let (dir, repo) = init_repo();
        let url = src_dir.path().to_string_lossy().into_owned();
        write_file(
            dir.path(),
            ".gitmodules",
            &format!("[submodule \"sub\"]\n\tpath = sub\n\turl = {url}\n"),
        );
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(".gitmodules")).unwrap();
        index
            .add(&git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: 0o160000,
                uid: 0,
                gid: 0,
                file_size: 0,
                id: *id,
                flags: 0,
                flags_extended: 0,
                path: b"sub".to_vec(),
            })
            .unwrap();
        index.write().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        git_core::submodule::init_submodule(&repo, "sub").unwrap();

        let error = git_core::submodule::update_submodule(&repo, "sub", false).unwrap_err();

        assert!(
            matches!(
                error,
                git_core::submodule::SubmoduleError::CasePairBroughtIn(_)
            ),
            "{pair:?}: {error:?}"
        );
        let mut names = [pair.0, pair.1];
        names.sort();
        assert_eq!(
            error.to_string(),
            format!(
                "'{}' and '{}' differ only in case, which this repository's core.ignorecase setting takes for one name, so the result would keep only one of them. Rename one of the two where it was added, then try again.",
                names[0], names[1]
            ),
            "{pair:?}"
        );
        let sub_dir = dir.path().join("sub");
        assert!(!sub_dir.join(pair.0).exists(), "{pair:?}");
        assert!(!sub_dir.join(pair.1).exists(), "{pair:?}");
        assert!(!sub_dir.join("conf").exists(), "{pair:?}");
    }
}
