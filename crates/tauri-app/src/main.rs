#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod worker;

use commands::{
    commit, get_commit_diff, get_commit_files, get_log, get_status, get_working_diff, open_repo,
    stage_file, unstage_file, AppState,
};

fn main() {
    tauri::Builder::default()
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
