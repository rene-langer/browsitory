use egui::{Button, Color32, ScrollArea, Ui};

use crate::state::RepoSession;

pub fn show(ui: &mut Ui, session: &mut RepoSession) {
    ui.heading("Branches");

    ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
        let mut switch_to = None;
        let mut delete = None;
        let mut rename = None;

        for branch in &session.branches {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(branch.is_head, &branch.name)
                    .on_hover_text(if branch.is_head {
                        "Current branch".to_string()
                    } else {
                        format!("Switch to {}", branch.name)
                    })
                    .clicked()
                    && !branch.is_head
                {
                    switch_to = Some(branch.name.clone());
                }

                if let Some(upstream) = &branch.upstream {
                    ui.weak(format!("-> {upstream}"));
                }

                // The currently checked-out branch can't be deleted:
                // git2 would leave HEAD dangling (see
                // `git_core::delete_branch`'s doc comment).
                if ui
                    .add_enabled(!branch.is_head, Button::new("Delete"))
                    .clicked()
                {
                    delete = Some(branch.name.clone());
                }
                if ui.button("Rename").clicked() {
                    rename = Some(branch.name.clone());
                }
            });
        }

        if let Some(name) = switch_to {
            session.switch_branch(name);
        }
        if let Some(name) = delete {
            session.delete_branch(name);
        }
        if let Some(name) = rename {
            session.rename_target = Some(name);
        }
    });

    ui.separator();

    if let Some(old_name) = session.rename_target.clone() {
        ui.horizontal(|ui| {
            ui.label(format!("Rename '{old_name}' to:"));
            ui.text_edit_singleline(&mut session.rename_input);
            if ui.button("Confirm").clicked() {
                let new_name = session.rename_input.trim().to_string();
                if !new_name.is_empty() {
                    session.rename_branch(old_name, new_name);
                }
                session.rename_target = None;
                session.rename_input.clear();
            }
            if ui.button("Cancel").clicked() {
                session.rename_target = None;
                session.rename_input.clear();
            }
        });
        ui.separator();
    }

    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut session.new_branch_name);
        if ui.button("Create Branch").clicked() {
            let name = session.new_branch_name.trim().to_string();
            if !name.is_empty() {
                session.create_branch(name, None);
                session.new_branch_name.clear();
            }
        }
    });

    if let Some(error) = &session.error {
        ui.colored_label(Color32::from_rgb(220, 80, 80), error);
    }
}
