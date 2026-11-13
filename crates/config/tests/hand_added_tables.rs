//! A config file the app saved with every list empty (#967): an entry added by hand as an
//! array-of-tables section at the end of the file, the natural way to write it, still reads back.

use std::path::{Path, PathBuf};

use config::{
    get_graph_branch_selection_at, get_graph_remote_branch_selection_at, get_merge_preference_at,
    get_pull_preference_at, list_open_repos_at, list_repo_aliases_at, list_workspaces_at,
    set_external_tool_settings_at, ExternalToolSettings, MergeFastForward, PullStrategyPreference,
};

/// A config file the app saved with defaults only, with `table` appended.
fn saved_then_appended(table: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_external_tool_settings_at(&file, &ExternalToolSettings::default()).unwrap();
    let saved = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, format!("{saved}\n{table}")).unwrap();
    (dir, file)
}

#[test]
fn open_repos() {
    let (_dir, file) = saved_then_appended("[[open_repos]]\npath = \"/repos/a\"\n");
    let (open, _) = list_open_repos_at(&file).unwrap();
    assert_eq!(
        open.iter().map(|e| e.path.clone()).collect::<Vec<_>>(),
        [PathBuf::from("/repos/a")]
    );
}

#[test]
fn workspaces() {
    let (_dir, file) = saved_then_appended(
        "[[workspaces]]\nid = \"w\"\nname = \"W\"\nroot_path = \"/repos\"\nmember_paths = []\n",
    );
    let names = list_workspaces_at(&file)
        .unwrap()
        .into_iter()
        .map(|w| w.name)
        .collect::<Vec<_>>();
    assert_eq!(names, ["W"]);
}

#[test]
fn graph_branch_selections() {
    let (_dir, file) = saved_then_appended(
        "[[graph_branch_selections]]\nrepo_path = \"/repos/a\"\nselected_branches = [\"main\"]\n",
    );
    assert_eq!(
        get_graph_branch_selection_at(&file, Path::new("/repos/a")).unwrap(),
        Some(vec!["main".to_string()])
    );
}

#[test]
fn graph_remote_branch_selections() {
    let (_dir, file) = saved_then_appended(
        "[[graph_remote_branch_selections]]\nrepo_path = \"/repos/a\"\n\
         selected_branches = [\"origin/main\"]\n",
    );
    assert_eq!(
        get_graph_remote_branch_selection_at(&file, Path::new("/repos/a")).unwrap(),
        Some(vec!["origin/main".to_string()])
    );
}

#[test]
fn merge_preferences() {
    let (_dir, file) = saved_then_appended(
        "[[merge_preferences]]\nrepo_path = \"/repos/a\"\nfast_forward = \"Never\"\n\
         squash = false\n",
    );
    let preference = get_merge_preference_at(&file, Path::new("/repos/a")).unwrap();
    assert_eq!(
        preference.map(|p| p.fast_forward),
        Some(MergeFastForward::Never)
    );
}

#[test]
fn pull_preferences() {
    let (_dir, file) = saved_then_appended(
        "[[pull_preferences]]\nrepo_path = \"/repos/a\"\nstrategy = \"Rebase\"\n",
    );
    assert_eq!(
        get_pull_preference_at(&file, Path::new("/repos/a")).unwrap(),
        Some(PullStrategyPreference::Rebase)
    );
}

#[test]
fn repo_aliases() {
    let (_dir, file) =
        saved_then_appended("[[repo_aliases]]\nrepo_path = \"/repos/a\"\nalias = \"A\"\n");
    let aliases = list_repo_aliases_at(&file)
        .unwrap()
        .into_iter()
        .map(|a| a.alias)
        .collect::<Vec<_>>();
    assert_eq!(aliases, ["A"]);
}
