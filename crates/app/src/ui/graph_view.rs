use std::collections::HashMap;

use egui::{Align2, Color32, FontId, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, Vec2};
use git_core::{GraphCommit, Oid};

use crate::state::RepoSession;

const ROW_HEIGHT: f32 = 26.0;
const LANE_WIDTH: f32 = 18.0;
const NODE_RADIUS: f32 = 5.0;
const GUTTER_PADDING: f32 = 12.0;

const LANE_COLORS: [Color32; 6] = [
    Color32::from_rgb(220, 120, 120),
    Color32::from_rgb(120, 180, 220),
    Color32::from_rgb(160, 210, 120),
    Color32::from_rgb(220, 180, 110),
    Color32::from_rgb(180, 130, 220),
    Color32::from_rgb(120, 210, 200),
];

fn lane_color(lane: usize) -> Color32 {
    LANE_COLORS[lane % LANE_COLORS.len()]
}

pub fn show(ui: &mut Ui, session: &mut RepoSession) {
    ui.heading("Graph");
    let Some(commits) = &session.graph else {
        ui.spinner();
        return;
    };
    if commits.is_empty() {
        ui.weak("No commits yet.");
        return;
    }

    let lanes = assign_lanes(commits);
    let max_lane = lanes.iter().copied().max().unwrap_or(0);
    let gutter_width = LANE_WIDTH * (max_lane as f32 + 1.0) + GUTTER_PADDING;
    let total_height = ROW_HEIGHT * commits.len() as f32;

    let mut clicked = None;
    ScrollArea::vertical().show(ui, |ui| {
        let width = ui.available_width();
        let (rect, _response) =
            ui.allocate_exact_size(Vec2::new(width, total_height), Sense::hover());
        let origin = rect.min;
        let painter = ui.painter();

        let positions: Vec<Pos2> = (0..commits.len())
            .map(|row| node_pos(origin, row, lanes[row]))
            .collect();
        let index_by_id: HashMap<Oid, usize> =
            commits.iter().enumerate().map(|(i, c)| (c.id, i)).collect();

        // Edges first, so commit nodes paint on top of them.
        for (row, commit) in commits.iter().enumerate() {
            for parent in &commit.parent_ids {
                if let Some(&parent_row) = index_by_id.get(parent) {
                    let stroke = Stroke::new(2.0, lane_color(lanes[row]));
                    painter.line_segment([positions[row], positions[parent_row]], stroke);
                }
            }
        }

        for (row, commit) in commits.iter().enumerate() {
            let pos = positions[row];
            let color = lane_color(lanes[row]);
            let selected = session.selected_commit == Some(commit.id);
            if selected {
                painter.circle_filled(pos, NODE_RADIUS + 3.0, Color32::WHITE);
            }
            painter.circle_filled(pos, NODE_RADIUS, color);

            let label = format!("{}  {}", short_hash(commit.id), commit.summary);
            painter.text(
                Pos2::new(origin.x + gutter_width, pos.y),
                Align2::LEFT_CENTER,
                label,
                FontId::monospace(12.0),
                ui.visuals().text_color(),
            );

            if !commit.refs.is_empty() {
                let refs_label = commit.refs.join(", ");
                let pill_color = Color32::from_rgb(210, 190, 120);
                let text_rect = painter.text(
                    Pos2::new(rect.max.x - 6.0, pos.y),
                    Align2::RIGHT_CENTER,
                    &refs_label,
                    FontId::proportional(11.0),
                    pill_color,
                );
                painter.rect_stroke(
                    text_rect.expand(3.0),
                    3.0,
                    Stroke::new(1.0, pill_color),
                    StrokeKind::Outside,
                );
            }

            let row_rect = Rect::from_min_size(
                Pos2::new(origin.x, origin.y + row as f32 * ROW_HEIGHT),
                Vec2::new(width, ROW_HEIGHT),
            );
            let id = ui.make_persistent_id(("graph_node", commit.id));
            let response = ui
                .interact(row_rect, id, Sense::click())
                .on_hover_text(format!(
                    "{}\n{} — {}",
                    commit.summary,
                    commit.author_name,
                    short_hash(commit.id)
                ));
            if response.clicked() {
                clicked = Some(commit.id);
            }
        }
    });

    if let Some(id) = clicked {
        session.select_commit(id);
    }
}

fn node_pos(origin: Pos2, row: usize, lane: usize) -> Pos2 {
    Pos2::new(
        origin.x + LANE_WIDTH * (lane as f32 + 0.5),
        origin.y + ROW_HEIGHT * (row as f32 + 0.5),
    )
}

fn short_hash(id: Oid) -> String {
    let hash = id.to_string();
    hash[..hash.len().min(7)].to_string()
}

/// Assigns each commit a lane, walking newest-to-oldest (the order
/// `graph_log` already returns).
///
/// `lanes[i]` tracks, for each currently-active lane, the `Oid` it is
/// "waiting" for next (i.e. the id a previously-visited, newer commit named
/// as a parent). A commit greedily reuses the lane already waiting for it if
/// one exists (a straight vertical line, or a merge converging in), or
/// claims the lowest-numbered free lane otherwise (a new branch tip). Its
/// first parent continues in the same lane; any additional parents (merge
/// sources) each claim their own lane. A lane is freed once the branch it
/// was tracking has no more commits to place: it hit a root commit, or
/// another lane converged onto the same commit first.
fn assign_lanes(commits: &[GraphCommit]) -> Vec<usize> {
    let mut lanes: Vec<Option<Oid>> = Vec::new();
    let mut result = Vec::with_capacity(commits.len());

    for commit in commits {
        let lane = match lanes.iter().position(|slot| *slot == Some(commit.id)) {
            Some(l) => l,
            None => match lanes.iter().position(|slot| slot.is_none()) {
                Some(l) => l,
                None => {
                    lanes.push(None);
                    lanes.len() - 1
                }
            },
        };
        result.push(lane);

        // Any other lane also waiting for this commit converges here; it has
        // nowhere further to go on its own, so free it.
        for (i, slot) in lanes.iter_mut().enumerate() {
            if i != lane && *slot == Some(commit.id) {
                *slot = None;
            }
        }

        match commit.parent_ids.split_first() {
            Some((first, rest)) => {
                lanes[lane] = Some(*first);
                for parent in rest {
                    if lanes.contains(&Some(*parent)) {
                        continue; // already tracked by another lane
                    }
                    match lanes.iter().position(|slot| slot.is_none()) {
                        Some(l) => lanes[l] = Some(*parent),
                        None => lanes.push(Some(*parent)),
                    }
                }
            }
            None => lanes[lane] = None, // root commit: this lane ends here
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(id: Oid, parents: Vec<Oid>) -> GraphCommit {
        GraphCommit {
            id,
            parent_ids: parents,
            summary: String::new(),
            author_name: String::new(),
            time: 0,
            refs: Vec::new(),
        }
    }

    fn oid(byte: u8) -> Oid {
        Oid::from_bytes(&[byte; 20]).unwrap()
    }

    #[test]
    fn linear_history_stays_in_one_lane() {
        let (c0, c1, c2) = (oid(0), oid(1), oid(2));
        let commits = vec![
            commit(c2, vec![c1]),
            commit(c1, vec![c0]),
            commit(c0, vec![]),
        ];
        assert_eq!(assign_lanes(&commits), vec![0, 0, 0]);
    }

    #[test]
    fn diverging_branches_use_two_lanes_and_converge() {
        let base = oid(0);
        let (main_tip, feature_tip) = (oid(1), oid(2));
        // Newest-first: two children of `base`, then `base` itself.
        let commits = vec![
            commit(main_tip, vec![base]),
            commit(feature_tip, vec![base]),
            commit(base, vec![]),
        ];
        let lanes = assign_lanes(&commits);
        assert_eq!(lanes[0], 0);
        assert_eq!(lanes[1], 1);
        assert_eq!(lanes[2], 0); // lowest-numbered lane wins at convergence
    }

    #[test]
    fn merge_commit_converges_two_parent_lanes() {
        let base = oid(0);
        let feature = oid(1);
        let merge = oid(2);
        let commits = vec![
            commit(merge, vec![base, feature]),
            commit(feature, vec![base]),
            commit(base, vec![]),
        ];
        let lanes = assign_lanes(&commits);
        assert_eq!(lanes[0], 0); // merge
        assert_eq!(lanes[1], 1); // feature, merge's second parent
        assert_eq!(lanes[2], 0); // base, both lanes converge here
    }

    #[test]
    fn empty_history_has_no_lanes() {
        assert!(assign_lanes(&[]).is_empty());
    }
}
