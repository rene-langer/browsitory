//! TEMPORARY probe for #389 on the CI runners (printed by the `--nocapture` step); removed
//! before merge.
use test_support::git;

fn init_repo() -> (tempfile::TempDir, git2::Repository) {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "T").unwrap();
        config.set_str("user.email", "t@e").unwrap();
        config.set_bool("core.autocrlf", false).unwrap();
        config.set_bool("commit.gpgSign", false).unwrap();
    }
    (dir, repo)
}

fn write_file(dir: &std::path::Path, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).unwrap();
}

fn commit_all(repo: &git2::Repository) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
        .unwrap();
}

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
    String::from_utf8_lossy(bytes)
        .chars()
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
    let out = git(
        dir,
        &["-c", "core.quotepath=false", "status", "--porcelain"],
    );
    show(&out.stdout).replace('\n', ";")
}

fn state(dir: &std::path::Path, old: &str, new: &str) -> String {
    let r = |p: &str| {
        std::fs::read_to_string(dir.join(p))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "-".into())
    };
    format!(
        "wt=[{}] ix=[{}] st=[{}] old={} new={}",
        names(dir),
        index(dir),
        status(dir),
        r(old),
        r(new)
    )
}

#[test]
fn probe_389() {
    let cases: [(&str, &str, &str); 4] = [
        ("sharp-s", "Stra\u{df}e", "Strasse"),
        ("nfc-nfd", "Caf\u{e9}", "Cafe\u{301}"),
        ("nfd-nfc", "Cafe\u{301}", "Caf\u{e9}"),
        ("e-acute-case", "Caf\u{e9}", "CAF\u{c9}"),
    ];
    let precomposes: &[Option<bool>] = if cfg!(target_os = "macos") {
        &[Some(true), Some(false)]
    } else {
        &[None]
    };
    let mut out = vec![format!("PROBE389 OS {}", std::env::consts::OS)];
    for &precompose in precomposes {
        for (label, old, new) in cases {
            for stage in ["git-add-A", "git2-add-all"] {
                for mode in ["all", "file-new", "file-old"] {
                    let (dir, repo) = init_repo();
                    let d = dir.path();
                    if !cfg!(any(target_os = "macos", windows)) {
                        repo.config()
                            .unwrap()
                            .set_bool("core.ignorecase", true)
                            .unwrap();
                    }
                    if let Some(value) = precompose {
                        repo.config()
                            .unwrap()
                            .set_bool("core.precomposeunicode", value)
                            .unwrap();
                    }
                    write_file(d, old, "content\n");
                    write_file(d, "other", "x\n");
                    commit_all(&repo);
                    let head = index(d);
                    let renamed = std::fs::rename(d.join(old), d.join(new)).is_ok();
                    drop(repo);
                    if stage == "git-add-A" {
                        git(d, &["add", "-A"]);
                    } else {
                        let repo = git2::Repository::open(d).unwrap();
                        let mut ix = repo.index().unwrap();
                        ix.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                            .unwrap();
                        ix.update_all(["*"].iter(), None).unwrap();
                        ix.write().unwrap();
                    }
                    write_file(d, "other", "changed\n");
                    let before = state(d, old, new);
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
                    let after = state(d, old, new);
                    let pop = if r.is_ok() {
                        let p = git_core::stash::pop_stash(&mut repo, 0, None, true);
                        format!("{p:?} {}", state(d, old, new))
                    } else {
                        String::new()
                    };
                    out.push(format!(
                        "{label} pc={precompose:?} {stage} {mode} head=[{head}] renamed={renamed}\n  before {before}\n  stash {r:?} -> {after}\n  pop {pop}"
                    ));
                }
            }
            // git stash for comparison (git add -A staging).
            let (dir, repo) = init_repo();
            let d = dir.path();
            if !cfg!(any(target_os = "macos", windows)) {
                repo.config()
                    .unwrap()
                    .set_bool("core.ignorecase", true)
                    .unwrap();
            }
            if let Some(value) = precompose {
                repo.config()
                    .unwrap()
                    .set_bool("core.precomposeunicode", value)
                    .unwrap();
            }
            write_file(d, old, "content\n");
            write_file(d, "other", "x\n");
            commit_all(&repo);
            let _ = std::fs::rename(d.join(old), d.join(new));
            git(d, &["add", "-A"]);
            write_file(d, "other", "changed\n");
            let before = state(d, old, new);
            let o = git(d, &["stash"]);
            let after = state(d, old, new);
            let p = git(d, &["stash", "pop"]);
            out.push(format!(
                "{label} pc={precompose:?} GITSTASH\n  before {before}\n  stash ok={} -> {after}\n  pop ok={} -> {}",
                o.status.success(),
                p.status.success(),
                state(d, old, new)
            ));
        }
    }
    eprintln!("{}\nPROBE389 END", out.join("\n"));
}
