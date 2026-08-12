#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod worker;

use commands::{
    commit, create_branch, delete_branch, get_commit_diff, get_commit_files, get_log, get_status,
    get_working_diff, list_branches, list_recent_repos, open_repo, pick_repo_folder, rename_branch,
    stage_file, switch_branch, unstage_file, AppState,
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_repo,
            get_status,
            get_log,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
