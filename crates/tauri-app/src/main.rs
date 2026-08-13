#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod worker;

use commands::{
    abort_merge, apply_stash, commit, create_branch, delete_branch, drop_stash, get_blame,
    get_commit_diff, get_commit_files, get_commit_graph, get_conflict_hunks, get_merge_message,
    get_status, get_working_diff, list_branches, list_recent_repos, list_stashes, open_repo,
    pick_repo_folder, rename_branch, resolve_add_delete_conflict, resolve_conflict, save_stash,
    stage_file, start_merge, switch_branch, unstage_file, AppState,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
