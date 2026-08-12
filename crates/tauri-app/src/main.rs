#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod worker;

use commands::{get_status, open_repo, AppState};

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![open_repo, get_status])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
