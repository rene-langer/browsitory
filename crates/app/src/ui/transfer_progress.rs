//! A small status-bar-style banner reporting the progress of an in-flight
//! push (and, once Workstream D's fetch/pull lands, those too — both report
//! through the same `Event::TransferProgress`/`RepoSession::transfer_progress`
//! plumbing, see `worker.rs`'s `Event::TransferProgress` doc comment).
//!
//! Not a dialog: this codebase has no modal dialogs anywhere in `ui/` (see
//! `branch_panel.rs`'s inline rename/create controls for the established
//! pattern), so this renders as an inline banner the caller places wherever
//! it fits — typically right above or below whatever panel triggered the
//! transfer.

use egui::{ProgressBar, Ui};
use git_core::TransferStage;

use crate::state::RepoSession;

/// Renders the progress banner if a transfer is in flight; a no-op
/// otherwise, so callers can call this unconditionally each frame.
pub fn show(ui: &mut Ui, session: &RepoSession) {
    let Some(progress) = &session.transfer_progress else {
        return;
    };

    ui.horizontal(|ui| {
        ui.label(stage_label(progress.stage));
        if progress.total_objects > 0 {
            let fraction = progress.indexed_objects as f32 / progress.total_objects as f32;
            ui.add(
                ProgressBar::new(fraction.clamp(0.0, 1.0))
                    .desired_width(160.0)
                    .text(format!(
                        "{}/{} objects",
                        progress.indexed_objects, progress.total_objects
                    )),
            );
        } else {
            // Before the pack negotiation reports counts there's nothing to
            // show a determinate fraction for — an indeterminate spinner
            // communicates "working" without implying a bogus percentage.
            ui.spinner();
        }
        if progress.received_bytes > 0 {
            ui.weak(format_bytes(progress.received_bytes));
        }
    });
}

fn stage_label(stage: TransferStage) -> &'static str {
    match stage {
        TransferStage::Negotiating => "Negotiating...",
        TransferStage::Receiving => "Receiving...",
        TransferStage::Indexing => "Indexing...",
        TransferStage::Pushing => "Pushing...",
        TransferStage::Done => "Done",
    }
}

fn format_bytes(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}
