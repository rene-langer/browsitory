#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod identifier_migration;

use commands::{
    abort_merge, abort_rebase, abort_sequence, add_remote, amend_commit, apply_commit_change,
    apply_patch, apply_stash, background_fetch, cancel_clone, cancel_search, cancel_transfer,
    checkout_commit, cherry_pick, cherry_pick_many, clear_current_upstream,
    clear_graph_branch_selection, clear_graph_remote_branch_selection, clone_repo, close_repo,
    commit, commits_since, continue_sequence, create_branch, create_pull_request, create_tag,
    create_worktree, delete_branch, delete_remote_branch, delete_remote_tag, delete_tag,
    delete_workspace, detect_forge_repository, discard_all_unstaged, discard_file, discard_hunk,
    discard_lines, drop_stash, fetch_all_remotes, fetch_remote, force_push_current_branch,
    forget_forge_token, forget_https_credential, get_app_version, get_blame,
    get_branch_compare_range, get_commit_diff, get_commit_files, get_commit_graph,
    get_commit_message, get_commit_message_settings, get_commit_rename_diff, get_commit_signature,
    get_commit_tree, get_conflict_hunks, get_current_upstream, get_diff_settings,
    get_external_tool_settings, get_fetch_settings, get_file_at_commit, get_file_history,
    get_file_sides, get_git_tools, get_graph_branch_selection, get_graph_remote_branch_selection,
    get_head_info, get_last_seen_version, get_line_history, get_merge_message, get_merge_options,
    get_patch, get_range_diff, get_range_files, get_reflog, get_remote_upstreams, get_stash_files,
    get_status, get_undo_state, get_working_diff, get_working_tree_compare_diff,
    get_working_tree_compare_files, ignore_path, init_repo, init_submodule, list_branches,
    list_open_repos, list_pull_requests, list_recent_authors, list_recent_repos, list_reflog_refs,
    list_remote_branches, list_remotes, list_stashes, list_submodules, list_tags, list_workspaces,
    list_worktrees, locate_commit, mark_resolved, open_diff_tool, open_external_url,
    open_in_editor, open_merge_tool, open_patch_file, open_repo, open_terminal, persist_open_repos,
    pick_repo_folder, pop_stash, prune_worktrees, pull_current_upstream, push_branch,
    push_current_branch, push_tags, rebase_continue, redo_last, remove_remote, remove_worktree,
    rename_branch, rename_remote, reset_to_commit, resolve_add_delete_conflict, resolve_conflict,
    resolve_whole_file, restore_file_version, restore_reflog_entry, reveal_in_file_manager,
    revert_commit, revert_many, save_file_version, save_forge_token, save_https_credential,
    save_patch_file, save_stash, save_workspace, scan_repos_in_root, search_commits,
    set_commit_message_settings, set_current_upstream, set_diff_settings,
    set_external_tool_settings, set_fetch_settings, set_graph_branch_selection,
    set_graph_remote_branch_selection, set_last_seen_version, set_merge_options,
    set_remote_auth_mode, stage_file, stage_hunk, stage_lines, start_merge, start_rebase,
    switch_branch, undo_last, unstage_file, unstage_hunk, unstage_lines, update_remote_urls,
    update_submodule, update_workspace, AppState, CloneState,
};

/// Declares the desktop app's Tauri commands once: `invoke_handler` registers them, and
/// `COMMAND_NAMES` lists the same names for the transport parity check (#306), which compares it
/// with `frontend/src/ipc/generated/tauri-commands.json`.
macro_rules! commands {
    ($($name:ident),* $(,)?) => {
        fn invoke_handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
            tauri::generate_handler![$($name),*]
        }

        #[cfg(test)]
        const COMMAND_NAMES: &[&str] = &[$(stringify!($name)),*];
    };
}

commands![
    open_repo,
    clone_repo,
    cancel_clone,
    init_repo,
    close_repo,
    get_status,
    get_commit_graph,
    locate_commit,
    get_graph_branch_selection,
    set_graph_branch_selection,
    clear_graph_branch_selection,
    get_graph_remote_branch_selection,
    set_graph_remote_branch_selection,
    clear_graph_remote_branch_selection,
    get_working_diff,
    get_commit_diff,
    get_commit_rename_diff,
    get_range_files,
    get_range_diff,
    get_working_tree_compare_files,
    get_working_tree_compare_diff,
    get_branch_compare_range,
    get_commit_files,
    get_file_history,
    get_line_history,
    search_commits,
    cancel_search,
    get_commit_message,
    get_commit_signature,
    stage_file,
    stage_hunk,
    unstage_file,
    unstage_hunk,
    discard_hunk,
    stage_lines,
    unstage_lines,
    discard_lines,
    discard_file,
    discard_all_unstaged,
    apply_commit_change,
    get_patch,
    apply_patch,
    save_patch_file,
    open_patch_file,
    ignore_path,
    get_diff_settings,
    set_diff_settings,
    commit,
    pick_repo_folder,
    list_recent_repos,
    get_last_seen_version,
    set_last_seen_version,
    get_app_version,
    list_open_repos,
    persist_open_repos,
    list_repo_aliases,
    set_repo_alias,
    scan_repos_in_root,
    list_workspaces,
    save_workspace,
    update_workspace,
    delete_workspace,
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
    list_remote_branches,
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
    fetch_all_remotes,
    push_branch,
    delete_remote_branch,
    delete_remote_tag,
    cancel_transfer,
    push_current_branch,
    force_push_current_branch,
    get_head_info,
    amend_commit,
    get_undo_state,
    undo_last,
    redo_last,
    cherry_pick,
    revert_commit,
    cherry_pick_many,
    revert_many,
    continue_sequence,
    abort_sequence,
    reset_to_commit,
    checkout_commit,
    push_tags,
    pull_current_upstream,
    list_stashes,
    save_stash,
    apply_stash,
    pop_stash,
    get_stash_files,
    drop_stash,
    get_blame,
    start_merge,
    get_conflict_hunks,
    resolve_conflict,
    resolve_add_delete_conflict,
    abort_merge,
    get_merge_message,
    get_merge_options,
    set_merge_options,
    commits_since,
    start_rebase,
    rebase_continue,
    abort_rebase,
    detect_forge_repository,
    save_forge_token,
    forget_forge_token,
    list_pull_requests,
    create_pull_request,
    open_external_url,
    open_in_editor,
    reveal_in_file_manager,
    open_terminal,
    open_diff_tool,
    open_merge_tool,
    mark_resolved,
    resolve_whole_file,
    get_external_tool_settings,
    set_external_tool_settings,
    get_git_tools,
    get_file_sides,
    get_commit_tree,
    get_file_at_commit,
    save_file_version,
    restore_file_version,
    list_recent_authors,
    get_commit_message_settings,
    set_commit_message_settings,
    get_fetch_settings,
    set_fetch_settings,
    background_fetch,
];

fn main() {
    // Installed before `.run()`, so it only fires once the log plugin below has a chance to
    // set up its logger: catches a worker thread panic (see worker/mod.rs's threading model
    // notes) that would otherwise just silently stop replying to that repo's commands.
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
    }));

    tauri::Builder::default()
        // First: moves the 0.3.x folders to the new identifier's names before the log plugin
        // opens its file and before the webview opens its storage (#433).
        .plugin(identifier_migration::plugin())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("browsitory".into()),
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                ])
                .level(log::LevelFilter::Info)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(5))
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
        .manage(CloneState::default())
        .setup(|app| {
            identifier_migration::log_report(app.handle());
            Ok(())
        })
        .invoke_handler(invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Hide the developer's / CI runner's global and system git config from every test (#111).
#[cfg(test)]
test_support::hermetic_git!();

#[cfg(test)]
mod tests {
    /// Regenerate with `UPDATE_TRANSPORT_METHODS=1 cargo test -p tauri-app command_list_is_current`.
    #[test]
    fn command_list_is_current() {
        let mut unique = super::COMMAND_NAMES.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            super::COMMAND_NAMES.len(),
            "a command is listed twice"
        );
        test_support::assert_generated_file(
            &test_support::repo_root().join("frontend/src/ipc/generated/tauri-commands.json"),
            &test_support::json_name_list(super::COMMAND_NAMES),
        );
    }
}
