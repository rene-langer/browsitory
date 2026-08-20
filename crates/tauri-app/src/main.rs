#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod credentials;
mod pull_requests;
mod worker;

use commands::{
    abort_merge, abort_rebase, add_remote, apply_stash, clear_current_upstream, close_repo, commit,
    commits_since, create_branch, create_pull_request, create_tag, create_worktree, delete_branch,
    delete_tag, detect_forge_repository, drop_stash, fetch_remote, forget_forge_token,
    forget_https_credential, get_blame, get_commit_diff, get_commit_files, get_commit_graph,
    get_conflict_hunks, get_current_upstream, get_merge_message, get_rebase_progress, get_reflog,
    get_remote_upstreams, get_status, get_working_diff, init_submodule, list_branches,
    list_open_repos, list_pull_requests, list_recent_repos, list_reflog_refs, list_remotes,
    list_stashes, list_submodules, list_tags, list_worktrees, open_external_url, open_repo,
    persist_open_repos, pick_repo_folder, prune_worktrees, pull_current_upstream,
    push_current_branch, push_tags, rebase_continue, remove_remote, remove_worktree, rename_branch,
    rename_remote, resolve_add_delete_conflict, resolve_conflict, restore_reflog_entry,
    save_forge_token, save_https_credential, save_stash, set_current_upstream,
    set_remote_auth_mode, stage_file, start_merge, start_rebase, switch_branch, unstage_file,
    update_remote_urls, update_submodule, AppState,
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_repo,
            close_repo,
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
            list_open_repos,
            persist_open_repos,
            list_branches,
            create_branch,
            switch_branch,
            delete_branch,
            rename_branch,
            list_worktrees,
            create_worktree,
            remove_worktree,
            prune_worktrees,
            list_submodules,
            init_submodule,
            update_submodule,
            list_reflog_refs,
            get_reflog,
            restore_reflog_entry,
            list_remotes,
            get_current_upstream,
            get_remote_upstreams,
            add_remote,
            rename_remote,
            update_remote_urls,
            remove_remote,
            save_https_credential,
            forget_https_credential,
            set_remote_auth_mode,
            set_current_upstream,
            clear_current_upstream,
            list_tags,
            create_tag,
            delete_tag,
            fetch_remote,
            push_current_branch,
            push_tags,
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
            detect_forge_repository,
            save_forge_token,
            forget_forge_token,
            list_pull_requests,
            create_pull_request,
            open_external_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
