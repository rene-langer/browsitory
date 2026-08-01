use egui::{Color32, ScrollArea, Ui};

use crate::state::RepoSession;

pub fn show(ui: &mut Ui, session: &mut RepoSession) {
    ui.heading("Stash");

    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut session.new_stash_message);
        if ui.button("Stash Changes").clicked() {
            let message = session.new_stash_message.trim();
            let message = if message.is_empty() {
                None
            } else {
                Some(message.to_string())
            };
            session.create_stash(message);
            session.new_stash_message.clear();
        }
    });

    ui.separator();

    ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
        let mut apply = None;
        let mut pop = None;
        let mut drop = None;

        for stash in &session.stashes {
            ui.horizontal(|ui| {
                ui.label(format!("stash@{{{}}}: {}", stash.index, stash.message));
                if ui.small_button("Apply").clicked() {
                    apply = Some(stash.index);
                }
                if ui.small_button("Pop").clicked() {
                    pop = Some(stash.index);
                }
                if ui.small_button("Drop").clicked() {
                    drop = Some(stash.index);
                }
            });
        }

        if let Some(index) = apply {
            session.apply_stash(index);
        }
        if let Some(index) = pop {
            session.pop_stash(index);
        }
        if let Some(index) = drop {
            session.drop_stash(index);
        }
    });

    if let Some(error) = &session.error {
        ui.colored_label(Color32::from_rgb(220, 80, 80), error);
    }
}
