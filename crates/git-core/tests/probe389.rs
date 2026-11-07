//! TEMPORARY probe for #389/#392 on the CI runners; removed before merge.
mod common;

use common::{commit_all, init_repo, write_file};
use test_support::git;

fn names(dir: &std::path::Path) -> String {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| show(entry.unwrap().file_name().to_string_lossy().as_bytes()))
        .filter(|name| name != ".git")
        .collect();
    names.sort();
    names.join(",")
}

fn show(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    s.chars()
        .map(|c| {
            if c.is_ascii() {
                c.to_string()
            } else {
                format!("<{:x}>", c as u32)
            }
        })
        .collect()
}

fn index(dir: &std::path::Path) -> String {
    let mut index = git2::Index::open(&dir.join(".git/index")).unwrap();
    index.read(true).unwrap();
    index
        .iter()
        .map(|e| show(&e.path))
        .collect::<Vec<_>>()
        .join(",")
}

fn status(dir: &std::path::Path) -> String {
    show(
        &git(
            dir,
            &["-c", "core.quotepath=false", "status", "--porcelain"],
        )
        .stdout,
    )
    .replace('\n', ";")
}

fn content(dir: &std::path::Path, old: &str, new: &str) -> String {
    let r = |p: &str| {
        std::fs::read_to_string(dir.join(p))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "-".into())
    };
    format!("old={} new={}", r(old), r(new))
}

#[test]
fn probe_389() {
    let cases: [(&str, &str, &str); 5] = [
        ("sharp-s", "Stra\u{df}e", "Strasse"),
        ("nfc-nfd", "Caf\u{e9}", "Cafe\u{301}"),
        ("nfd-nfc", "Cafe\u{301}", "Caf\u{e9}"),
        ("e-acute-case", "Caf\u{e9}", "CAF\u{c9}"),
        ("ascii", "Old", "old"),
    ];
    let mut out = vec![format!("OS {}", std::env::consts::OS)];
    for (label, old, new) in cases {
        for stage in ["git-add-A", "git2-add-all", "git-mv"] {
            for mode in ["all", "file-new", "file-old"] {
                let (dir, repo) = init_repo();
                let d = dir.path();
                let cfg = repo.config().unwrap();
                if !cfg!(any(target_os = "macos", windows)) {
                    drop(cfg);
                    repo.config()
                        .unwrap()
                        .set_bool("core.ignorecase", true)
                        .unwrap();
                }
                let cfgs = format!(
                    "icase={:?} precompose={:?}",
                    repo.config().unwrap().get_bool("core.ignorecase").ok(),
                    repo.config()
                        .unwrap()
                        .get_bool("core.precomposeunicode")
                        .ok()
                );
                write_file(d, old, "content\n");
                write_file(d, "other", "x\n");
                commit_all(&repo, "base");
                let head = index(d);
                let staged = match stage {
                    "git-mv" => {
                        let o = git(d, &["mv", old, new]);
                        format!("mv={}", o.status.success())
                    }
                    _ => {
                        let r = std::fs::rename(d.join(old), d.join(new));
                        let s = format!("rename={}", r.is_ok());
                        if stage == "git-add-A" {
                            git(d, &["add", "-A"]);
                        } else {
                            drop(repo);
                            let repo = git2::Repository::open(d).unwrap();
                            let mut ix = repo.index().unwrap();
                            ix.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                                .unwrap();
                            ix.update_all(["*"].iter(), None).unwrap();
                            ix.write().unwrap();
                        }
                        s
                    }
                };
                write_file(d, "other", "changed\n");
                let before = format!(
                    "wt=[{}] ix=[{}] st=[{}] {}",
                    names(d),
                    index(d),
                    status(d),
                    content(d, old, new)
                );
                let mut repo = git2::Repository::open(d).unwrap();
                let paths = match mode {
                    "all" => vec![],
                    "file-new" => vec![new.to_string()],
                    _ => vec![old.to_string()],
                };
                let opts = git_core::stash::StashOptions {
                    include_untracked: false,
                    paths,
                    ..Default::default()
                };
                let r = git_core::stash::save_stash(&mut repo, &opts);
                let after = format!(
                    "wt=[{}] ix=[{}] st=[{}] {}",
                    names(d),
                    index(d),
                    status(d),
                    content(d, old, new)
                );
                let pop = if r.is_ok() {
                    let p = git_core::stash::pop_stash(&mut repo, 0, None, true);
                    format!(
                        "{p:?} wt=[{}] ix=[{}] st=[{}] {}",
                        names(d),
                        index(d),
                        status(d),
                        content(d, old, new)
                    )
                } else {
                    String::new()
                };
                out.push(format!(
                    "{label} {stage} {mode} {cfgs} head=[{head}] {staged}\n  before {before}\n  stash {r:?} -> {after}\n  pop {pop}"
                ));
                if stage == "git-mv" || mode == "all" && stage == "git2-add-all" {
                    // fine
                }
            }
        }
        // git stash for comparison (all, git add -A staging).
        let (dir, repo) = init_repo();
        let d = dir.path();
        if !cfg!(any(target_os = "macos", windows)) {
            repo.config()
                .unwrap()
                .set_bool("core.ignorecase", true)
                .unwrap();
        }
        write_file(d, old, "content\n");
        write_file(d, "other", "x\n");
        commit_all(&repo, "base");
        let _ = std::fs::rename(d.join(old), d.join(new));
        git(d, &["add", "-A"]);
        write_file(d, "other", "changed\n");
        let before = format!("wt=[{}] ix=[{}] st=[{}]", names(d), index(d), status(d));
        let o = git(d, &["stash"]);
        let after = format!(
            "wt=[{}] ix=[{}] st=[{}] {}",
            names(d),
            index(d),
            status(d),
            content(d, old, new)
        );
        let p = git(d, &["stash", "pop"]);
        out.push(format!(
            "{label} GITSTASH before {before}\n  stash ok={} {} -> {after}\n  pop ok={} -> wt=[{}] ix=[{}] st=[{}] {}",
            o.status.success(),
            show(&o.stderr).replace('\n', ";"),
            p.status.success(),
            names(d),
            index(d),
            status(d),
            content(d, old, new)
        ));
    }
    panic!("PROBE389\n{}", out.join("\n"));
}
