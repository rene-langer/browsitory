#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod worker;

use commands::{
    abort_merge, abort_rebase, add_remote, apply_stash, clear_current_upstream, commit,
    commits_since, create_branch, delete_branch, drop_stash, fetch_remote, get_blame,
    get_commit_diff, get_commit_files, get_commit_graph, get_conflict_hunks, get_current_upstream,
    get_merge_message, get_rebase_progress, get_remote_upstreams, get_status, get_working_diff,
    list_branches, list_recent_repos, list_remotes, list_stashes, open_repo, pick_repo_folder,
    pull_current_upstream, rebase_continue, remove_remote, rename_branch, rename_remote,
    resolve_add_delete_conflict, resolve_conflict, save_stash, set_current_upstream, stage_file,
    start_merge, start_rebase, switch_branch, unstage_file, update_remote_urls, AppState,
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_repo,
            get_status,
            get_commit_graph,
            get_working_diff,
            get_commit_diff,
            get_commit_files,
            stage_file,
            unstage_file,
            commit,
            pick_repo_folder,
            list_recent_repos,
            list_branches,
            create_branch,
            switch_branch,
            delete_branch,
            rename_branch,
            list_remotes,
            get_current_upstream,
            get_remote_upstreams,
            add_remote,
            rename_remote,
            update_remote_urls,
            remove_remote,
            set_current_upstream,
            clear_current_upstream,
            fetch_remote,
            pull_current_upstream,
            list_stashes,
            save_stash,
            apply_stash,
            drop_stash,
            get_blame,
            start_merge,
            get_conflict_hunks,
            resolve_conflict,
            resolve_add_delete_conflict,
            abort_merge,
            get_merge_message,
            commits_since,
            start_rebase,
            rebase_continue,
            abort_rebase,
            get_rebase_progress,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
