//! Compact deterministic knowledge graph.

use crate::links::LinkIndex;
use crate::storage::{GraphNodeOffset, Note};
use eframe::egui::{self, Color32, FontId, Pos2, Sense, Stroke, Ui, Vec2};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::f32::consts::TAU;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum GraphScope {
    Local,
    Folder,
    Global,
}

pub struct GraphState {
    pub scope: GraphScope,
    pub filter: String,
    pub tag_filter: String,
    pub show_external: bool,
    pub show_missing: bool,
    pub show_labels: bool,
    pub max_nodes: usize,
    pan: Vec2,
    zoom: f32,
    node_offsets: HashMap<(GraphScope, Uuid), Vec2>,
    dragged_node_id: Option<Uuid>,
    selection_cache: Option<(u64, Arc<GraphSelection>)>,
}

impl GraphState {
    pub fn restore(offsets: &[GraphNodeOffset]) -> Self {
        let mut state = Self::default();
        for offset in offsets {
            let scope = match offset.scope.as_str() {
                "local" => GraphScope::Local,
                "folder" => GraphScope::Folder,
                "global" => GraphScope::Global,
                _ => continue,
            };
            state
                .node_offsets
                .insert((scope, offset.note_id), Vec2::new(offset.x, offset.y));
        }
        state
    }

    pub fn persisted_offsets(&self) -> Vec<GraphNodeOffset> {
        self.node_offsets
            .iter()
            .map(|((scope, note_id), offset)| GraphNodeOffset {
                scope: match scope {
                    GraphScope::Local => "local",
                    GraphScope::Folder => "folder",
                    GraphScope::Global => "global",
                }
                .to_owned(),
                note_id: *note_id,
                x: offset.x,
                y: offset.y,
            })
            .collect()
    }
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            scope: GraphScope::Local,
            filter: String::new(),
            tag_filter: String::new(),
            show_external: true,
            show_missing: true,
            show_labels: true,
            max_nodes: 80,
            pan: Vec2::ZERO,
            zoom: 1.0,
            node_offsets: HashMap::new(),
            dragged_node_id: None,
            selection_cache: None,
        }
    }
}

#[derive(Default)]
pub struct GraphOutput {
    pub opened_note_id: Option<Uuid>,
    pub create_missing_target: Option<String>,
    pub state_changed: bool,
    pub persist_layout: bool,
    pub expand: bool,
}

struct GraphSelection {
    node_ids: Vec<Uuid>,
    edges: Vec<(Uuid, Uuid)>,
    external_ids: HashSet<Uuid>,
    center_id: Option<Uuid>,
}

pub fn show(
    ui: &mut Ui,
    state: &mut GraphState,
    notes: &[Note],
    links: &LinkIndex,
    selected_note_id: Option<Uuid>,
    notes_root: &Path,
    selected_folder: &Path,
) -> GraphOutput {
    let mut scope_changed = false;
    let mut fit = false;
    let mut expand = false;
    let mut state_changed = false;
    let mut persist_layout = false;
    ui.horizontal_wrapped(|ui| {
        for (scope, label) in [
            (GraphScope::Local, "Local"),
            (GraphScope::Folder, "Folder"),
            (GraphScope::Global, "Vault"),
        ] {
            if ui.selectable_label(state.scope == scope, label).clicked() {
                state.scope = scope;
                scope_changed = true;
                state_changed = true;
                persist_layout = true;
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("Full view")
                .on_hover_text("Toggle graph workspace without side panels")
                .clicked()
            {
                expand = true;
            }
            if ui
                .small_button("Fit")
                .on_hover_text("Fit visible nodes without changing their positions")
                .clicked()
            {
                fit = true;
            }
            if ui.small_button("Reset").clicked() {
                state.pan = Vec2::ZERO;
                state.zoom = 1.0;
                state
                    .node_offsets
                    .retain(|(scope, _), _| *scope != state.scope);
                state.dragged_node_id = None;
                state_changed = true;
                persist_layout = true;
            }
            if ui.small_button("+").on_hover_text("Zoom in").clicked() {
                state.zoom = (state.zoom * 1.2).min(2.5);
            }
            if ui.small_button("−").on_hover_text("Zoom out").clicked() {
                state.zoom = (state.zoom / 1.2).max(0.35);
            }
            ui.small(format!("{}%", (state.zoom * 100.0).round()));
        });
    });
    if scope_changed {
        state.pan = Vec2::ZERO;
        state.zoom = 1.0;
        state.dragged_node_id = None;
    }
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(ui.visuals().hyperlink_color, "● current");
        ui.colored_label(ui.visuals().hyperlink_color, "● linked");
        ui.colored_label(Color32::from_gray(80), "● external");
        ui.colored_label(Color32::from_gray(125), "◌ missing");
    });
    ui.collapsing("Filters and density", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.filter)
                    .hint_text("Find title or alias")
                    .desired_width(150.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut state.tag_filter)
                    .hint_text("Tag filter")
                    .desired_width(110.0),
            );
            ui.checkbox(&mut state.show_external, "External");
            ui.checkbox(&mut state.show_missing, "Missing");
            ui.checkbox(&mut state.show_labels, "Labels");
            ui.add(egui::Slider::new(&mut state.max_nodes, 10..=200).text("Node limit"));
        });
    });

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (
        links.generation(),
        selected_note_id,
        notes_root,
        selected_folder,
        state.scope,
        &state.filter,
        &state.tag_filter,
        state.show_external,
        state.max_nodes,
    )
        .hash(&mut hasher);
    let key = hasher.finish();
    let selection = if let Some((cached_key, selection)) = &state.selection_cache
        && *cached_key == key
    {
        Arc::clone(selection)
    } else {
        let selection = Arc::new(select_graph(
            state,
            notes,
            links,
            selected_note_id,
            notes_root,
            selected_folder,
        ));
        state.selection_cache = Some((key, Arc::clone(&selection)));
        selection
    };
    let note_by_id: HashMap<Uuid, &Note> = notes.iter().map(|note| (note.id, note)).collect();

    let canvas_size = ui.available_size().max(Vec2::new(80.0, 80.0));
    let (response, painter) = ui.allocate_painter(canvas_size, Sense::click_and_drag());
    if response.hovered() {
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            let old_zoom = state.zoom;
            state.zoom = (state.zoom * (scroll * 0.002).exp()).clamp(0.35, 2.5);
            state_changed = true;

            if let Some(pointer) = ui.input(|input| input.pointer.hover_pos()) {
                let relative = pointer - response.rect.center();
                state.pan = relative - (relative - state.pan) * (state.zoom / old_zoom);
            }
        }
    }

    if selection.node_ids.is_empty() {
        painter.text(
            response.rect.center(),
            egui::Align2::CENTER_CENTER,
            "No notes in this scope",
            FontId::proportional(14.0),
            ui.visuals().weak_text_color(),
        );
        return GraphOutput {
            expand,
            state_changed,
            persist_layout,
            ..Default::default()
        };
    }

    let world_positions = layout_positions(&selection);
    if fit {
        let mut bounds = egui::Rect::NOTHING;
        for (id, pos) in &world_positions {
            bounds.extend_with(
                *pos + state
                    .node_offsets
                    .get(&(state.scope, *id))
                    .copied()
                    .unwrap_or_default(),
            );
        }
        state.zoom = ((response.rect.width() - 80.0) / bounds.width().max(1.0))
            .min((response.rect.height() - 80.0) / bounds.height().max(1.0))
            .clamp(0.1, 2.5);
        state.pan = -bounds.center().to_vec2() * state.zoom;
    }
    let screen_positions = calculate_screen_positions(
        &selection.node_ids,
        &world_positions,
        &state.node_offsets,
        state.scope,
        response.rect.center(),
        state.pan,
        state.zoom,
    );

    let pointer = ui.input(|input| input.pointer.hover_pos());
    let hovered_before_drag = hovered_node(
        pointer,
        &selection.node_ids,
        &screen_positions,
        selected_note_id,
    );

    if response.drag_started() {
        state.dragged_node_id = hovered_before_drag;
    }
    if response.dragged() {
        if let Some(id) = state.dragged_node_id {
            let offset = state.node_offsets.entry((state.scope, id)).or_default();
            *offset += response.drag_delta() / state.zoom;
            state_changed = true;
        } else {
            state.pan += response.drag_delta();
            state_changed = true;
        }
    }
    if response.drag_stopped() {
        state.dragged_node_id = None;
        persist_layout = true;
    }

    // Repaint nodes and edges in the same drag frame.
    let screen_positions = calculate_screen_positions(
        &selection.node_ids,
        &world_positions,
        &state.node_offsets,
        state.scope,
        response.rect.center(),
        state.pan,
        state.zoom,
    );
    let hovered_id = hovered_node(
        pointer,
        &selection.node_ids,
        &screen_positions,
        selected_note_id,
    );

    let direct_neighbors: HashSet<Uuid> = selection
        .edges
        .iter()
        .filter_map(|(from, to)| match selected_note_id {
            Some(selected) if *from == selected => Some(*to),
            Some(selected) if *to == selected => Some(*from),
            _ => None,
        })
        .collect();
    let mut degree = HashMap::<Uuid, usize>::new();
    for (from, to) in &selection.edges {
        *degree.entry(*from).or_default() += 1;
        *degree.entry(*to).or_default() += 1;
        let start = screen_positions.get(from).copied().unwrap_or_default();
        let end = screen_positions.get(to).copied().unwrap_or_default();
        let color = if Some(*from) == selected_note_id || Some(*to) == selected_note_id {
            ui.visuals().hyperlink_color
        } else {
            Color32::from_gray(75)
        };
        let connected_to_selection =
            selected_note_id.is_none_or(|selected| *from == selected || *to == selected);
        let color = if connected_to_selection {
            color
        } else {
            color.gamma_multiply(0.35)
        };
        painter.line_segment(
            [start, end],
            Stroke::new(if connected_to_selection { 1.8 } else { 1.0 }, color),
        );
        paint_arrow(&painter, start, end, color);
    }

    for id in &selection.node_ids {
        let position = screen_positions.get(id).copied().unwrap_or_default();
        let selected = Some(*id) == selected_note_id;
        let external = selection.external_ids.contains(id);
        let hovered = Some(*id) == hovered_id;
        let dragging = state.dragged_node_id == Some(*id);
        let radius =
            node_radius(*id, selected_note_id) + if hovered || dragging { 1.5 } else { 0.0 };
        let connected = selected || direct_neighbors.contains(id);
        let fill = if selected {
            ui.visuals().hyperlink_color
        } else if external {
            Color32::from_gray(80)
        } else if connected || hovered || dragging {
            ui.visuals().hyperlink_color
        } else {
            Color32::from_gray(155)
        };
        let fill = if selected_note_id.is_some() && !connected {
            fill.gamma_multiply(0.45)
        } else {
            fill
        };

        painter.circle_filled(position, radius, fill);
        if selected || external {
            painter.circle_stroke(
                position,
                radius + 3.0,
                Stroke::new(
                    1.0,
                    if selected {
                        fill
                    } else {
                        Color32::from_gray(105)
                    },
                ),
            );
        }

        if (selection.node_ids.len() <= 30
            || selected
            || hovered
            || dragging
            || degree.get(id).copied().unwrap_or(0) >= 4)
            && state.show_labels
            && let Some(note) = note_by_id.get(id)
        {
            let title = display_title(note);
            painter.text(
                position + Vec2::new(0.0, radius + 7.0),
                egui::Align2::CENTER_TOP,
                title,
                FontId::proportional(12.0),
                ui.visuals().text_color(),
            );
        }
    }

    if let Some(id) = hovered_id {
        ui.ctx()
            .set_cursor_icon(if state.dragged_node_id.is_some() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        if let Some(note) = note_by_id.get(&id) {
            let mut details = display_title(note).to_owned();
            if !note.tags.is_empty() {
                details.push_str(&format!("\nTags: {}", note.tags.join(", ")));
            }
            if !note.aliases.is_empty() {
                details.push_str(&format!("\nAliases: {}", note.aliases.join(", ")));
            }
            response.clone().on_hover_text(details);
        }
        if response.double_clicked() {
            return GraphOutput {
                opened_note_id: Some(id),
                state_changed,
                persist_layout,
                ..Default::default()
            };
        }
    } else if response.hovered() {
        ui.ctx()
            .set_cursor_icon(if state.dragged_node_id.is_some() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
    }

    if state.scope == GraphScope::Local
        && state.show_missing
        && let Some(selected) = selected_note_id
        && let Some(note_links) = links.links_for(selected)
        && let Some(center) = screen_positions.get(&selected).copied()
    {
        let missing_count = note_links.unresolved.len().min(8);
        for (index, target) in note_links.unresolved.iter().take(missing_count).enumerate() {
            let angle = index as f32 / missing_count.max(1) as f32 * TAU - 0.5;
            let position = center + Vec2::angled(angle) * 78.0 * state.zoom;
            paint_dashed_circle(&painter, position, 5.0, Color32::from_gray(125));
            painter.text(
                position + Vec2::new(0.0, 9.0),
                egui::Align2::CENTER_TOP,
                target,
                FontId::proportional(10.0),
                ui.visuals().weak_text_color(),
            );
            if pointer.is_some_and(|pointer| pointer.distance(position) <= 11.0)
                && response.double_clicked()
            {
                return GraphOutput {
                    create_missing_target: Some(target.clone()),
                    state_changed,
                    persist_layout,
                    ..Default::default()
                };
            }
        }
    }

    painter.text(
        response.rect.left_bottom() + Vec2::new(6.0, -6.0),
        egui::Align2::LEFT_BOTTOM,
        "Drag: move  •  Wheel: zoom  •  Double-click: open",
        FontId::proportional(10.0),
        ui.visuals().weak_text_color(),
    );

    GraphOutput {
        expand,
        state_changed,
        persist_layout,
        ..Default::default()
    }
}

fn paint_dashed_circle(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    for index in (0..16).step_by(2) {
        let start = index as f32 / 16.0 * TAU;
        let end = (index + 1) as f32 / 16.0 * TAU;
        painter.line_segment(
            [
                center + Vec2::angled(start) * radius,
                center + Vec2::angled(end) * radius,
            ],
            Stroke::new(1.0, color),
        );
    }
}

fn calculate_screen_positions(
    node_ids: &[Uuid],
    world_positions: &HashMap<Uuid, Pos2>,
    offsets: &HashMap<(GraphScope, Uuid), Vec2>,
    scope: GraphScope,
    canvas_center: Pos2,
    pan: Vec2,
    zoom: f32,
) -> HashMap<Uuid, Pos2> {
    node_ids
        .iter()
        .map(|id| {
            let base = world_positions.get(id).copied().unwrap_or(Pos2::ZERO);
            let manual_offset = offsets.get(&(scope, *id)).copied().unwrap_or_default();
            (
                *id,
                canvas_center + pan + (base.to_vec2() + manual_offset) * zoom,
            )
        })
        .collect()
}

fn hovered_node(
    pointer: Option<Pos2>,
    node_ids: &[Uuid],
    screen_positions: &HashMap<Uuid, Pos2>,
    selected_note_id: Option<Uuid>,
) -> Option<Uuid> {
    let pointer = pointer?;
    node_ids
        .iter()
        .filter_map(|id| {
            let position = screen_positions.get(id)?;
            let distance = pointer.distance(*position);
            (distance <= node_radius(*id, selected_note_id) + 7.0).then_some((*id, distance))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(id, _)| id)
}

fn display_title(note: &Note) -> &str {
    if note.title.trim().is_empty() {
        "Untitled"
    } else {
        note.title.as_str()
    }
}

fn select_graph(
    state: &GraphState,
    notes: &[Note],
    links: &LinkIndex,
    selected_note_id: Option<Uuid>,
    notes_root: &Path,
    selected_folder: &Path,
) -> GraphSelection {
    let all_ids: HashSet<Uuid> = notes.iter().map(|note| note.id).collect();
    let all_edges: Vec<(Uuid, Uuid)> = links
        .edges()
        .filter(|(from, to)| all_ids.contains(from) && all_ids.contains(to))
        .collect();
    let mut visible_ids = HashSet::new();
    let mut external_ids = HashSet::new();

    match state.scope {
        GraphScope::Global => {
            let mut newest = notes.iter().collect::<Vec<_>>();
            newest.sort_by_key(|note| std::cmp::Reverse(note.updated_at));
            visible_ids.extend(newest.into_iter().take(state.max_nodes).map(|note| note.id));
        }
        GraphScope::Local => {
            if let Some(selected) = selected_note_id
                && all_ids.contains(&selected)
            {
                visible_ids.insert(selected);
                for (from, to) in &all_edges {
                    if *from == selected || *to == selected {
                        visible_ids.insert(*from);
                        visible_ids.insert(*to);
                    }
                }
            }
        }
        GraphScope::Folder => {
            for note in notes {
                if note_is_in_folder(note, notes_root, selected_folder) {
                    visible_ids.insert(note.id);
                }
            }
            let folder_ids = visible_ids.clone();
            for (from, to) in &all_edges {
                if folder_ids.contains(from) || folder_ids.contains(to) {
                    visible_ids.insert(*from);
                    visible_ids.insert(*to);
                }
            }
            external_ids.extend(visible_ids.difference(&folder_ids).copied());
        }
    }

    if !state.show_external {
        visible_ids.retain(|id| !external_ids.contains(id));
        external_ids.clear();
    }

    let query = state.filter.trim().to_lowercase();
    let tag_query = state
        .tag_filter
        .trim()
        .trim_start_matches('#')
        .to_lowercase();
    if !query.is_empty() || !tag_query.is_empty() {
        let matching = notes
            .iter()
            .filter(|note| {
                let matches_query = query.is_empty()
                    || note.title.to_lowercase().contains(&query)
                    || note
                        .aliases
                        .iter()
                        .any(|alias| alias.to_lowercase().contains(&query));
                let matches_tag = tag_query.is_empty()
                    || note
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&tag_query));
                matches_query && matches_tag
            })
            .map(|note| note.id)
            .collect::<HashSet<_>>();
        visible_ids.retain(|id| matching.contains(id) || Some(*id) == selected_note_id);
        external_ids.retain(|id| visible_ids.contains(id));
    }

    if visible_ids.len() > state.max_nodes {
        let mut newest = notes
            .iter()
            .filter(|note| visible_ids.contains(&note.id))
            .collect::<Vec<_>>();
        newest.sort_by_key(|note| std::cmp::Reverse(note.updated_at));
        let mut kept = newest
            .into_iter()
            .take(state.max_nodes)
            .map(|note| note.id)
            .collect::<HashSet<_>>();
        if let Some(selected) = selected_note_id {
            kept.insert(selected);
        }
        visible_ids.retain(|id| kept.contains(id));
        external_ids.retain(|id| visible_ids.contains(id));
    }

    let mut node_ids: Vec<Uuid> = visible_ids.iter().copied().collect();
    node_ids.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    let edges = all_edges
        .into_iter()
        .filter(|(from, to)| visible_ids.contains(from) && visible_ids.contains(to))
        .collect();
    let center_id = selected_note_id
        .filter(|id| visible_ids.contains(id))
        .or_else(|| node_ids.first().copied());

    GraphSelection {
        node_ids,
        edges,
        external_ids,
        center_id,
    }
}

fn note_is_in_folder(note: &Note, notes_root: &Path, selected_folder: &Path) -> bool {
    note.file_path
        .parent()
        .and_then(|parent| parent.strip_prefix(notes_root).ok())
        .is_some_and(|relative| relative.starts_with(selected_folder))
}

fn layout_positions(selection: &GraphSelection) -> HashMap<Uuid, Pos2> {
    let Some(center) = selection.center_id else {
        return HashMap::new();
    };
    let visible: HashSet<Uuid> = selection.node_ids.iter().copied().collect();
    let mut adjacency: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for (from, to) in &selection.edges {
        adjacency.entry(*from).or_default().push(*to);
        adjacency.entry(*to).or_default().push(*from);
    }

    let mut distances = HashMap::from([(center, 0_usize)]);
    let mut queue = VecDeque::from([center]);
    while let Some(current) = queue.pop_front() {
        let next_distance = distances[&current] + 1;
        for neighbor in adjacency.get(&current).into_iter().flatten() {
            if visible.contains(neighbor) && !distances.contains_key(neighbor) {
                distances.insert(*neighbor, next_distance);
                queue.push_back(*neighbor);
            }
        }
    }

    let maximum_connected_depth = distances.values().copied().max().unwrap_or(0);
    let mut rings: BTreeMap<usize, Vec<Uuid>> = BTreeMap::new();
    let mut disconnected_index = 0;
    for id in &selection.node_ids {
        let depth = distances.get(id).copied().unwrap_or_else(|| {
            let depth = maximum_connected_depth + 1 + disconnected_index / 14;
            disconnected_index += 1;
            depth
        });
        rings.entry(depth).or_default().push(*id);
    }

    let mut positions = HashMap::new();
    for (depth, ids) in rings {
        if depth == 0 {
            positions.insert(center, Pos2::ZERO);
            continue;
        }
        let base_radius = 95.0 * depth as f32;
        for id in ids {
            let bytes = id.as_bytes();
            let angle_seed = u64::from_be_bytes(bytes[..8].try_into().unwrap_or([0; 8]));
            let radius_seed = u16::from_be_bytes([bytes[8], bytes[9]]);
            let angle = angle_seed as f64 / u64::MAX as f64 * TAU as f64;
            let radius = base_radius * (0.92 + radius_seed as f32 / u16::MAX as f32 * 0.16);
            positions.insert(
                id,
                Pos2::new(angle.cos() as f32 * radius, angle.sin() as f32 * radius),
            );
        }
    }
    positions
}

fn node_radius(id: Uuid, selected_note_id: Option<Uuid>) -> f32 {
    if Some(id) == selected_note_id {
        6.0
    } else {
        3.5
    }
}

fn paint_arrow(painter: &egui::Painter, start: Pos2, end: Pos2, color: Color32) {
    let direction = end - start;
    if direction.length_sq() < 1.0 {
        return;
    }
    let direction = direction.normalized();
    let tip = end - direction * 7.0;
    let side = Vec2::new(-direction.y, direction.x);
    painter.line_segment(
        [tip, tip - direction * 6.0 + side * 3.0],
        Stroke::new(1.2, color),
    );
    painter.line_segment(
        [tip, tip - direction * 6.0 - side * 3.0],
        Stroke::new(1.2, color),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn note(root: &Path, folder: &str, title: &str, content: &str) -> Note {
        let directory = root.join(folder);
        let mut note = Note::new_named(&directory, title);
        note.content = content.to_owned();
        note.file_path = directory.join(format!("{title}--1234abcd.md"));
        note
    }

    #[test]
    fn local_scope_contains_current_note_and_direct_neighbors() {
        let root = PathBuf::from("vault/Notes");
        let first = note(&root, "Programming", "First", "[[Second]]");
        let second = note(&root, "Programming", "Second", "[[Third]]");
        let third = note(&root, "Biologia", "Third", "");
        let first_id = first.id;
        let second_id = second.id;
        let notes = vec![first, second, third];
        let links = LinkIndex::build(&notes, &root);
        let state = GraphState::default();

        let selection = select_graph(
            &state,
            &notes,
            &links,
            Some(first_id),
            &root,
            Path::new("Programming"),
        );

        assert_eq!(selection.node_ids.len(), 2);
        assert!(selection.node_ids.contains(&first_id));
        assert!(selection.node_ids.contains(&second_id));
    }

    #[test]
    fn folder_scope_marks_cross_folder_neighbors_as_external() {
        let root = PathBuf::from("vault/Notes");
        let programming = note(&root, "Programming", "Rust", "[[Biologia/Bones]]");
        let bones = note(&root, "Biologia", "Bones", "");
        let bones_id = bones.id;
        let notes = vec![programming, bones];
        let links = LinkIndex::build(&notes, &root);
        let state = GraphState {
            scope: GraphScope::Folder,
            ..Default::default()
        };

        let selection = select_graph(
            &state,
            &notes,
            &links,
            None,
            &root,
            Path::new("Programming"),
        );

        assert_eq!(selection.node_ids.len(), 2);
        assert_eq!(selection.external_ids, HashSet::from([bones_id]));
    }

    #[test]
    fn manual_node_offset_moves_with_zoom_in_world_coordinates() {
        let id = Uuid::new_v4();
        let base = HashMap::from([(id, Pos2::new(10.0, 20.0))]);
        let offsets = HashMap::from([((GraphScope::Local, id), Vec2::new(5.0, -5.0))]);

        let positions = calculate_screen_positions(
            &[id],
            &base,
            &offsets,
            GraphScope::Local,
            Pos2::new(100.0, 100.0),
            Vec2::new(10.0, 0.0),
            2.0,
        );

        assert_eq!(positions[&id], Pos2::new(140.0, 130.0));
    }

    #[test]
    fn global_scope_is_limited_for_compact_rendering() {
        let root = PathBuf::from("vault/Notes");
        let notes = (0..100)
            .map(|index| note(&root, "", &format!("Note {index}"), ""))
            .collect::<Vec<_>>();
        let links = LinkIndex::build(&notes, &root);
        let state = GraphState {
            scope: GraphScope::Global,
            ..Default::default()
        };

        let selection = select_graph(&state, &notes, &links, None, &root, Path::new(""));

        assert_eq!(selection.node_ids.len(), 80);
    }

    #[test]
    fn saved_offsets_round_trip_through_settings_format() {
        let id = Uuid::new_v4();
        let state = GraphState::restore(&[GraphNodeOffset {
            scope: "folder".to_owned(),
            note_id: id,
            x: 12.0,
            y: -8.0,
        }]);

        let offsets = state.persisted_offsets();

        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0].note_id, id);
        assert_eq!(offsets[0].scope, "folder");
        assert_eq!((offsets[0].x, offsets[0].y), (12.0, -8.0));
    }

    #[test]
    fn graph_filters_titles_aliases_and_tags_but_keeps_the_current_note() {
        let root = PathBuf::from("vault/Notes");
        let current = note(&root, "", "Current", "[[Rust ownership]] [[Python]]");
        let current_id = current.id;
        let mut rust = note(&root, "", "Ownership", "");
        rust.aliases.push("Rust ownership".to_owned());
        rust.tags.push("rust".to_owned());
        let rust_id = rust.id;
        let mut python = note(&root, "", "Python", "");
        python.tags.push("python".to_owned());
        let notes = vec![current, rust, python];
        let links = LinkIndex::build(&notes, &root);
        let state = GraphState {
            filter: "ownership".to_owned(),
            tag_filter: "#rust".to_owned(),
            ..Default::default()
        };

        let selection = select_graph(
            &state,
            &notes,
            &links,
            Some(current_id),
            &root,
            Path::new(""),
        );

        assert_eq!(selection.node_ids.len(), 2);
        assert!(selection.node_ids.contains(&current_id));
        assert!(selection.node_ids.contains(&rust_id));
    }

    #[test]
    fn folder_graph_can_hide_cross_folder_nodes() {
        let root = PathBuf::from("vault/Notes");
        let inside = note(&root, "Inside", "One", "[[Outside/Two]]");
        let inside_id = inside.id;
        let outside = note(&root, "Outside", "Two", "");
        let notes = vec![inside, outside];
        let links = LinkIndex::build(&notes, &root);
        let state = GraphState {
            scope: GraphScope::Folder,
            show_external: false,
            ..Default::default()
        };

        let selection = select_graph(&state, &notes, &links, None, &root, Path::new("Inside"));

        assert_eq!(selection.node_ids, vec![inside_id]);
        assert!(selection.external_ids.is_empty());
        assert!(selection.edges.is_empty());
    }

    #[test]
    fn an_unrelated_node_does_not_move_existing_layout_positions() {
        let center = Uuid::from_u128(1);
        let linked = Uuid::from_u128(2);
        let initial = GraphSelection {
            node_ids: vec![center, linked],
            edges: vec![(center, linked)],
            external_ids: HashSet::new(),
            center_id: Some(center),
        };
        let initial_positions = layout_positions(&initial);
        let unrelated = Uuid::from_u128(3);
        let expanded = GraphSelection {
            node_ids: vec![center, linked, unrelated],
            edges: vec![(center, linked)],
            external_ids: HashSet::new(),
            center_id: Some(center),
        };
        let expanded_positions = layout_positions(&expanded);

        assert_eq!(initial_positions[&center], expanded_positions[&center]);
        assert_eq!(initial_positions[&linked], expanded_positions[&linked]);
    }

    #[test]
    fn dense_global_graph_respects_limit_with_a_large_vault() {
        let root = PathBuf::from("vault/Notes");
        let notes = (0..1_000)
            .map(|index| {
                note(
                    &root,
                    &format!("Area {}", index % 20),
                    &format!("Note {index}"),
                    &format!("[[Note {}]]", (index + 1) % 1_000),
                )
            })
            .collect::<Vec<_>>();
        let started = std::time::Instant::now();
        let links = LinkIndex::build(&notes, &root);
        let state = GraphState {
            scope: GraphScope::Global,
            max_nodes: 120,
            ..Default::default()
        };
        let selection = select_graph(&state, &notes, &links, None, &root, Path::new(""));
        let positions = layout_positions(&selection);
        let elapsed = started.elapsed();

        assert_eq!(selection.node_ids.len(), 120);
        assert_eq!(positions.len(), 120);
        assert!(elapsed < std::time::Duration::from_secs(10));
    }
}
