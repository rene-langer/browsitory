use egui::{Button, Color32, ScrollArea, Ui};

use crate::state::RepoSession;

/// Remote list with inline add/remove/rename/edit-URL CRUD, a Fetch button
/// per remote, and a Pull (remote + branch) control — same row-based,
/// no-dialog pattern as `branch_panel.rs` (this codebase has no modal
/// dialogs anywhere in `ui/`).
pub fn show(ui: &mut Ui, session: &mut RepoSession) {
    ui.heading("Remotes");

    // Default the pull-branch field to the current branch's configured
    // upstream (shorthand like "origin/main") the first time it's empty, so
    // the common case ("pull my current branch") needs no typing. The user
    // can still overwrite it for a different remote/branch combination.
    if session.pull_branch.is_empty()
        && let Some(current) = session
            .branches
            .iter()
            .find(|b| b.is_head)
            .and_then(|b| b.upstream.as_ref())
        && let Some((_remote, branch)) = current.split_once('/')
    {
        session.pull_branch = branch.to_string();
    }

    ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
        let mut remove = None;
        let mut rename = None;
        let mut edit_url = None;
        let mut fetch = None;
        let mut pull = None;

        for remote in &session.remotes {
            ui.horizontal(|ui| {
                ui.label(&remote.name);
                ui.weak(&remote.url);

                if ui
                    .add_enabled(!session.transfer_in_progress, Button::new("Fetch"))
                    .clicked()
                {
                    fetch = Some(remote.name.clone());
                }

                let can_pull = !session.transfer_in_progress && !session.pull_branch.is_empty();
                if ui
                    .add_enabled(can_pull, Button::new("Pull"))
                    .on_hover_text(format!(
                        "Pull {} from {}",
                        if session.pull_branch.is_empty() {
                            "(no branch set)"
                        } else {
                            session.pull_branch.as_str()
                        },
                        remote.name
                    ))
                    .clicked()
                {
                    pull = Some((remote.name.clone(), session.pull_branch.clone()));
                }

                if ui.button("Rename").clicked() {
                    rename = Some(remote.name.clone());
                }
                if ui.button("Edit URL").clicked() {
                    edit_url = Some((remote.name.clone(), remote.url.clone()));
                }
                if ui.button("Remove").clicked() {
                    remove = Some(remote.name.clone());
                }
            });
        }

        if let Some(name) = remove {
            session.remove_remote(name);
        }
        if let Some(name) = rename {
            session.remote_rename_target = Some(name);
        }
        if let Some((name, url)) = edit_url {
            session.remote_url_edit_target = Some(name);
            session.remote_url_edit_input = url;
        }
        if let Some(name) = fetch {
            session.fetch(name);
        }
        if let Some((remote, branch)) = pull {
            session.pull(remote, branch);
        }
    });

    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Pull branch:");
        ui.text_edit_singleline(&mut session.pull_branch);
    });

    if let Some(progress) = &session.transfer_progress {
        ui.label(format!(
            "{:?}: {}/{} objects ({} bytes)",
            progress.stage,
            progress.received_objects,
            progress.total_objects,
            progress.received_bytes
        ));
    } else if session.transfer_in_progress {
        ui.label("Transferring...");
    }

    ui.separator();

    if let Some(old_name) = session.remote_rename_target.clone() {
        ui.horizontal(|ui| {
            ui.label(format!("Rename '{old_name}' to:"));
            ui.text_edit_singleline(&mut session.remote_rename_input);
            if ui.button("Confirm").clicked() {
                let new_name = session.remote_rename_input.trim().to_string();
                if !new_name.is_empty() {
                    session.rename_remote(old_name, new_name);
                }
                session.remote_rename_target = None;
                session.remote_rename_input.clear();
            }
            if ui.button("Cancel").clicked() {
                session.remote_rename_target = None;
                session.remote_rename_input.clear();
            }
        });
        ui.separator();
    }

    if let Some(name) = session.remote_url_edit_target.clone() {
        ui.horizontal(|ui| {
            ui.label(format!("New URL for '{name}':"));
            ui.text_edit_singleline(&mut session.remote_url_edit_input);
            if ui.button("Confirm").clicked() {
                let url = session.remote_url_edit_input.trim().to_string();
                if !url.is_empty() {
                    session.set_remote_url(name, url);
                }
                session.remote_url_edit_target = None;
                session.remote_url_edit_input.clear();
            }
            if ui.button("Cancel").clicked() {
                session.remote_url_edit_target = None;
                session.remote_url_edit_input.clear();
            }
        });
        ui.separator();
    }

    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut session.new_remote_name)
            .on_hover_text("Name");
        ui.text_edit_singleline(&mut session.new_remote_url)
            .on_hover_text("URL");
        if ui.button("Add Remote").clicked() {
            let name = session.new_remote_name.trim().to_string();
            let url = session.new_remote_url.trim().to_string();
            if !name.is_empty() && !url.is_empty() {
                session.add_remote(name, url);
                session.new_remote_name.clear();
                session.new_remote_url.clear();
            }
        }
    });

    if let Some(error) = &session.error {
        ui.colored_label(Color32::from_rgb(220, 80, 80), error);
    }
}
