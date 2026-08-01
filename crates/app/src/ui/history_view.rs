use egui::{Color32, Frame, ScrollArea, Sense, Ui};

use crate::state::RepoSession;

pub fn show(ui: &mut Ui, session: &mut RepoSession) {
    ui.heading("History");
    let mut clicked = None;
    ScrollArea::vertical().show(ui, |ui| {
        for commit in &session.commits {
            let hash = commit.id.to_string();
            let short_hash = &hash[..hash.len().min(7)];
            let selected = session.selected_commit == Some(commit.id);
            let bg = if selected {
                Color32::from_rgb(40, 60, 90)
            } else {
                Color32::TRANSPARENT
            };

            let response = Frame::new()
                .fill(bg)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.monospace(short_hash);
                        ui.label(&commit.summary);
                    });
                    ui.small(format!("{} <{}>", commit.author_name, commit.author_email));
                })
                .response
                .interact(Sense::click());
            if response.clicked() {
                clicked = Some(commit.id);
            }
            ui.separator();
        }
    });
    if let Some(id) = clicked {
        session.select_commit(id);
    }
}
