use egui::{Color32, Frame, RichText, ScrollArea, Ui};
use git_core::{DiffLine, FileDiff, LineKind, word_diff};
use similar::ChangeTag;

use crate::state::RepoSession;

pub fn show(ui: &mut Ui, session: &RepoSession) {
    let Some(selected) = &session.selected_diff else {
        ui.weak("Select a file to view its diff.");
        return;
    };

    ui.heading(&selected.path);
    let Some(diff) = &selected.diff else {
        ui.spinner();
        return;
    };

    ScrollArea::vertical().show(ui, |ui| {
        render_lines(ui, diff);
    });
}

/// Renders one side of a diff (context/addition/deletion lines, with
/// word-level highlighting for changed-line pairs). `pub(crate)` so
/// `conflict_view.rs` can reuse it for the side-by-side ours/theirs
/// conflict view instead of duplicating the diff-coloring logic.
pub(crate) fn render_lines(ui: &mut Ui, diff: &FileDiff) {
    let lines = &diff.lines;
    let mut i = 0;
    while i < lines.len() {
        // A removal immediately followed by a single addition is rendered as
        // a word-level highlighted "changed line" pair (GitHub/GitLab style)
        // instead of two flat +/- blocks.
        let is_changed_pair = lines[i].kind == LineKind::Deletion
            && i + 1 < lines.len()
            && lines[i + 1].kind == LineKind::Addition
            && (i + 2 >= lines.len() || lines[i + 2].kind != LineKind::Addition);

        if is_changed_pair {
            render_word_diff_pair(ui, &lines[i], &lines[i + 1]);
            i += 2;
        } else {
            render_plain_line(ui, &lines[i]);
            i += 1;
        }
    }
}

fn render_plain_line(ui: &mut Ui, line: &DiffLine) {
    let (bg, prefix) = match line.kind {
        LineKind::Addition => (Color32::from_rgb(20, 60, 20), "+"),
        LineKind::Deletion => (Color32::from_rgb(70, 20, 20), "-"),
        LineKind::Context => (Color32::TRANSPARENT, " "),
    };
    Frame::new().fill(bg).show(ui, |ui| {
        ui.monospace(format!("{prefix} {}", line.content));
    });
}

fn render_word_diff_pair(ui: &mut Ui, old_line: &DiffLine, new_line: &DiffLine) {
    let changes = word_diff(&old_line.content, &new_line.content);

    Frame::new()
        .fill(Color32::from_rgb(70, 20, 20))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.monospace("- ");
                for (tag, text) in &changes {
                    if *tag != ChangeTag::Insert {
                        word_span(ui, *tag, text);
                    }
                }
            });
        });
    Frame::new()
        .fill(Color32::from_rgb(20, 60, 20))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.monospace("+ ");
                for (tag, text) in &changes {
                    if *tag != ChangeTag::Delete {
                        word_span(ui, *tag, text);
                    }
                }
            });
        });
}

fn word_span(ui: &mut Ui, tag: ChangeTag, text: &str) {
    let rich = if tag == ChangeTag::Equal {
        RichText::new(text).monospace()
    } else {
        RichText::new(text)
            .monospace()
            .strong()
            .background_color(Color32::from_rgb(120, 90, 20))
    };
    ui.label(rich);
}
