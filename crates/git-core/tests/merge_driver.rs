//! Custom merge drivers (#1165). A path whose `merge` attribute names a driver configured as
//! `merge.<name>.driver` (a command) is left conflicted when both sides changed it (owner
//! decision: Browsitory never runs the command), in every operation that merges: merge (a pull's
//! too), squash, cherry-pick, revert, rebase steps, stash apply and pop. Every other file merges
//! as before, and the status names the driver of such a conflict. libgit2's built-in drivers
//! (`binary`, `text`, `union`, `-merge`, `merge`) and a driver name without config keep git's
//! meaning, compared with `git merge` on a copy of the same repository.

mod common;

use std::path::Path;

use common::{commit_all, git_ok, init_repo, twin, write_file};
use git_core::hooks::HookOptions;
use git_core::merge::{MergeOptions, MergeOutcome};
use git_core::pick::PickOutcome;
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};
use git_core::stash::StashApplyOutcome;

const BASE: &str = "1\n2\n3\n4\n5\n";
/// master changes the first line, topic the last: a text merge takes both without a conflict.
const OURS: &str = "1 master\n2\n3\n4\n5\n";
const THEIRS: &str = "1\n2\n3\n4\n5 topic\n";
const MERGED: &str = "1 master\n2\n3\n4\n5 topic\n";

/// Where the driver command would leave a mark if it ever ran.
fn marker(repo: &git2::Repository) -> std::path::PathBuf {
    repo.path().join("driver-ran")
}

/// Configures `merge.ours.driver` as a command that leaves `marker` behind (and keeps ours,
/// like the common `true`).
fn configure_ours(repo: &git2::Repository) {
    let marker = marker(repo).to_string_lossy().replace('\\', "/");
    repo.config()
        .unwrap()
        .set_str("merge.ours.driver", &format!("touch '{marker}'"))
        .unwrap();
}

/// `.gitattributes` = `attributes`; `version.txt` (under the attribute) and `other.txt` are
/// changed on both master and topic without overlapping, `only-topic.txt` (under it too) on topic
/// only. master is checked out.
fn diverged(attributes: &str) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, ".gitattributes", attributes);
    for file in ["version.txt", "other.txt", "only-topic.txt"] {
        write_file(d, file, BASE);
    }
    commit_all(&repo, "base");
    git_ok(d, &["branch", "-M", "master"]);
    git_ok(d, &["checkout", "-q", "-b", "topic"]);
    for file in ["version.txt", "other.txt", "only-topic.txt"] {
        write_file(d, file, THEIRS);
    }
    commit_all(&repo, "topic");
    git_ok(d, &["checkout", "-q", "master"]);
    for file in ["version.txt", "other.txt"] {
        write_file(d, file, OURS);
    }
    commit_all(&repo, "master");
    (dir, repo)
}

const OURS_ATTRIBUTES: &str = "version.txt merge=ours\nonly-topic.txt merge=ours\n";

fn diverged_under_ours() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = diverged(OURS_ATTRIBUTES);
    configure_ours(&repo);
    (dir, repo)
}

fn read(dir: &Path, file: &str) -> String {
    std::fs::read_to_string(dir.join(file)).unwrap()
}

/// The index's stages of `path` (1 ancestor, 2 ours, 3 theirs) with their content.
fn stages(repo: &git2::Repository, path: &str) -> Vec<(i32, String)> {
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    let mut stages: Vec<(i32, String)> = index
        .iter()
        .filter(|entry| entry.path == path.as_bytes())
        .map(|entry| {
            let stage = i32::from((entry.flags >> 12) & 0x3);
            let blob = repo.find_blob(entry.id).unwrap();
            (stage, String::from_utf8_lossy(blob.content()).into_owned())
        })
        .collect();
    stages.sort();
    stages
}

/// What every operation leaves: `version.txt` conflicted with its three versions, the other
/// files merged (or taken from the one side that changed them), the driver never run.
fn assert_driver_file_conflicted(dir: &Path, repo: &git2::Repository) {
    assert_eq!(
        stages(repo, "version.txt"),
        vec![
            (1, BASE.to_string()),
            (2, OURS.to_string()),
            (3, THEIRS.to_string())
        ]
    );
    assert_eq!(read(dir, "other.txt"), MERGED);
    assert_eq!(read(dir, "only-topic.txt"), THEIRS);
    assert!(!marker(repo).exists(), "the driver command ran");
}

fn conflicted(files: &[&str]) -> Vec<String> {
    files.iter().map(|file| file.to_string()).collect()
}

#[test]
fn a_merge_leaves_a_file_under_a_configured_driver_conflicted_and_merges_the_rest() {
    let (dir, repo) = diverged_under_ours();

    let outcome = git_core::merge::start_merge(&repo, "topic", MergeOptions::default()).unwrap();

    assert_eq!(
        outcome,
        MergeOutcome::Conflicted {
            files: conflicted(&["version.txt"])
        }
    );
    assert_driver_file_conflicted(dir.path(), &repo);
}

#[test]
fn a_pulls_merge_of_the_upstream_leaves_it_conflicted_too() {
    let (dir, repo) = diverged_under_ours();
    git_ok(
        dir.path(),
        &["update-ref", "refs/remotes/origin/master", "topic"],
    );

    let outcome =
        git_core::merge::start_merge(&repo, "refs/remotes/origin/master", MergeOptions::default())
            .unwrap();

    assert_eq!(
        outcome,
        MergeOutcome::Conflicted {
            files: conflicted(&["version.txt"])
        }
    );
    assert_driver_file_conflicted(dir.path(), &repo);
}

#[test]
fn a_squash_merge_leaves_it_conflicted() {
    let (dir, repo) = diverged_under_ours();
    let options = MergeOptions {
        squash: true,
        ..MergeOptions::default()
    };

    let outcome = git_core::merge::start_merge(&repo, "topic", options).unwrap();

    assert_eq!(
        outcome,
        MergeOutcome::Conflicted {
            files: conflicted(&["version.txt"])
        }
    );
    assert_driver_file_conflicted(dir.path(), &repo);
}

#[test]
fn a_cherry_pick_leaves_it_conflicted() {
    let (dir, repo) = diverged_under_ours();
    let topic = repo.revparse_single("topic").unwrap().id().to_string();

    let outcome = git_core::pick::cherry_pick(&repo, &topic, None, HookOptions::default()).unwrap();

    assert_eq!(
        outcome,
        PickOutcome::Conflicted {
            files: conflicted(&["version.txt"])
        }
    );
    assert_driver_file_conflicted(dir.path(), &repo);
}

#[test]
fn a_cherry_pick_sequence_stops_at_it() {
    let (dir, repo) = diverged_under_ours();
    let topic = repo.revparse_single("topic").unwrap().id().to_string();

    let run = git_core::pick::cherry_pick_many(&repo, &[topic], HookOptions::default());

    assert!(
        matches!(
            &run.outcome,
            Ok(git_core::pick::SequenceOutcome::Conflicted { files, .. })
                if *files == conflicted(&["version.txt"])
        ),
        "{:?}",
        run.outcome
    );
    assert_driver_file_conflicted(dir.path(), &repo);
}

#[test]
fn a_revert_leaves_it_conflicted() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, ".gitattributes", "version.txt merge=ours\n");
    write_file(d, "version.txt", BASE);
    commit_all(&repo, "base");
    write_file(d, "version.txt", THEIRS);
    commit_all(&repo, "change the last line");
    let reverted = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    write_file(d, "version.txt", MERGED);
    commit_all(&repo, "change the first line");
    configure_ours(&repo);

    let outcome = git_core::pick::revert(&repo, &reverted, None, HookOptions::default()).unwrap();

    assert_eq!(
        outcome,
        PickOutcome::Conflicted {
            files: conflicted(&["version.txt"])
        }
    );
    assert!(!marker(&repo).exists(), "the driver command ran");
}

#[test]
fn a_rebase_step_stops_at_it() {
    let (dir, repo) = diverged_under_ours();
    git_ok(dir.path(), &["checkout", "-q", "topic"]);
    let plan = git_core::rebase::commits_since(&repo, "master")
        .unwrap()
        .into_iter()
        .map(|commit| RebasePlanEntry {
            commit_id: commit.id,
            action: RebaseAction::Pick,
            combined_message: None,
        })
        .collect();

    let (_, result) = git_core::rebase::start_rebase(&repo, "master", plan).unwrap();

    assert_eq!(
        result,
        RebaseStepResult::Conflicted {
            files: conflicted(&["version.txt"])
        }
    );
    assert_driver_file_conflicted(dir.path(), &repo);
}

/// master's `version.txt` and `other.txt` with the last line changed and stashed, then the first
/// line changed and committed.
fn stash_over_a_change() -> (tempfile::TempDir, git2::Repository) {
    let (dir, mut repo) = init_repo();
    let d = dir.path();
    write_file(d, ".gitattributes", "version.txt merge=ours\n");
    write_file(d, "version.txt", BASE);
    write_file(d, "other.txt", BASE);
    commit_all(&repo, "base");
    write_file(d, "version.txt", THEIRS);
    write_file(d, "other.txt", THEIRS);
    let options = git_core::stash::StashOptions {
        message: None,
        keep_index: false,
        include_untracked: false,
        paths: Vec::new(),
    };
    git_core::stash::save_stash(&mut repo, &options).unwrap();
    write_file(d, "version.txt", OURS);
    write_file(d, "other.txt", OURS);
    commit_all(&repo, "change the first line");
    configure_ours(&repo);
    (dir, repo)
}

#[test]
fn a_stash_apply_leaves_it_conflicted() {
    let (dir, mut repo) = stash_over_a_change();

    let outcome = git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap();

    assert_eq!(
        outcome,
        StashApplyOutcome::Conflicted {
            files: conflicted(&["version.txt"])
        }
    );
    assert_eq!(read(dir.path(), "other.txt"), MERGED);
    assert!(!marker(&repo).exists(), "the driver command ran");
}

#[test]
fn a_stash_pop_leaves_it_conflicted_and_keeps_the_stash() {
    let (dir, mut repo) = stash_over_a_change();

    let outcome = git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap();

    assert_eq!(
        outcome,
        StashApplyOutcome::Conflicted {
            files: conflicted(&["version.txt"])
        }
    );
    assert_eq!(read(dir.path(), "other.txt"), MERGED);
    assert!(repo.revparse_single("refs/stash").is_ok());
}

#[test]
fn merge_default_naming_a_configured_driver_applies_to_files_without_the_attribute() {
    let (dir, repo) = diverged("");
    configure_ours(&repo);
    repo.config()
        .unwrap()
        .set_str("merge.default", "ours")
        .unwrap();

    let outcome = git_core::merge::start_merge(&repo, "topic", MergeOptions::default()).unwrap();

    assert_eq!(
        outcome,
        MergeOutcome::Conflicted {
            files: conflicted(&["other.txt", "version.txt"])
        }
    );
    assert_eq!(read(dir.path(), "only-topic.txt"), THEIRS);
    assert!(!marker(&repo).exists(), "the driver command ran");
}

#[test]
fn a_file_both_sides_changed_alike_is_no_conflict() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, ".gitattributes", "version.txt merge=ours\n");
    write_file(d, "version.txt", BASE);
    commit_all(&repo, "base");
    git_ok(d, &["branch", "-M", "master"]);
    git_ok(d, &["checkout", "-q", "-b", "topic"]);
    write_file(d, "version.txt", MERGED);
    write_file(d, "t.txt", "t\n");
    commit_all(&repo, "topic");
    git_ok(d, &["checkout", "-q", "master"]);
    write_file(d, "version.txt", MERGED);
    commit_all(&repo, "master");
    configure_ours(&repo);

    let outcome = git_core::merge::start_merge(&repo, "topic", MergeOptions::default()).unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert!(!marker(&repo).exists(), "the driver command ran");
}

#[test]
fn the_status_names_the_driver_of_such_a_conflict() {
    let (_dir, repo) = diverged_under_ours();
    git_core::merge::start_merge(&repo, "topic", MergeOptions::default()).unwrap();

    let status = git_core::status::status_refreshing_index(&repo).unwrap();
    let stamped = git_core::change_stamp::status_with_stamp(&repo)
        .unwrap()
        .status;

    for entries in [status, stamped] {
        let version = entries
            .iter()
            .find(|entry| entry.path == "version.txt")
            .unwrap();
        assert_eq!(version.merge_driver.as_deref(), Some("ours"));
        assert!(entries
            .iter()
            .filter(|entry| entry.path != "version.txt")
            .all(|entry| entry.merge_driver.is_none()));
    }
}

#[test]
fn the_status_names_the_driver_merge_default_gives() {
    let (_dir, repo) = diverged("");
    configure_ours(&repo);
    repo.config()
        .unwrap()
        .set_str("merge.default", "ours")
        .unwrap();
    git_core::merge::start_merge(&repo, "topic", MergeOptions::default()).unwrap();

    let status = git_core::status::status_refreshing_index(&repo).unwrap();

    let drivers: Vec<_> = status
        .iter()
        .map(|entry| (entry.path.as_str(), entry.merge_driver.as_deref()))
        .collect();
    assert_eq!(
        drivers,
        vec![
            // Staged from topic, which alone changed it: no conflict, no driver named.
            ("only-topic.txt", None),
            ("other.txt", Some("ours")),
            ("version.txt", Some("ours"))
        ]
    );
}

#[test]
fn the_status_names_no_driver_for_a_built_in_or_unconfigured_one() {
    // `merge=binary` conflicts (libgit2's own driver); `merge=custom` has no config, so it
    // merges as text and isn't a conflict.
    let (_dir, repo) = diverged("version.txt merge=binary\nother.txt merge=custom\n");
    git_core::merge::start_merge(&repo, "topic", MergeOptions::default()).unwrap();

    let status = git_core::status::status_refreshing_index(&repo).unwrap();

    let version = status
        .iter()
        .find(|entry| entry.path == "version.txt")
        .unwrap();
    assert_eq!(version.kind, git_core::status::StatusKind::Conflicted);
    assert_eq!(version.merge_driver, None);
}

/// Merges topic into master here with `start_merge` and with `git merge` on a copy, and returns
/// for each whether it conflicted and `version.txt`'s content.
fn merge_both(dir: &Path, repo: &git2::Repository) -> ((bool, String), (bool, String)) {
    let (twin_dir, _twin_repo) = twin(dir);
    let gits = test_support::git(twin_dir.path(), &["merge", "-q", "--no-edit", "topic"]);
    let outcome = git_core::merge::start_merge(repo, "topic", MergeOptions::default()).unwrap();
    let ours = (
        matches!(outcome, MergeOutcome::Conflicted { .. }),
        read(dir, "version.txt"),
    );
    let gits = (!gits.status.success(), read(twin_dir.path(), "version.txt"));
    (ours, gits)
}

#[test]
fn built_in_drivers_and_unconfigured_names_merge_as_git_merges_them() {
    for attribute in ["merge", "merge=text", "merge=union", "merge=custom"] {
        let (dir, repo) = diverged(&format!("version.txt {attribute}\n"));

        let (ours, gits) = merge_both(dir.path(), &repo);

        assert_eq!(ours, gits, "{attribute}");
        assert_eq!(ours, (false, MERGED.to_string()), "{attribute}");
    }
}

#[test]
fn the_binary_driver_conflicts_as_git_does() {
    for attribute in ["merge=binary", "-merge"] {
        let (dir, repo) = diverged(&format!("version.txt {attribute}\n"));

        let (ours, gits) = merge_both(dir.path(), &repo);

        assert_eq!(ours.0, gits.0, "{attribute}");
        assert!(ours.0, "{attribute}");
        assert_eq!(
            stages(&repo, "version.txt"),
            vec![
                (1, BASE.to_string()),
                (2, OURS.to_string()),
                (3, THEIRS.to_string())
            ],
            "{attribute}"
        );
    }
}
