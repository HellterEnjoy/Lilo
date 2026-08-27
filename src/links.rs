//! Derived wiki-link and backlink index.

use crate::storage::Note;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WikiLink {
    pub target: String,
    pub label: Option<String>,
    pub heading: Option<String>,
    pub source_range: Range<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteLinks {
    pub outgoing: Vec<Uuid>,
    pub backlinks: Vec<Uuid>,
    pub unresolved: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkResolution {
    Resolved(Uuid),
    Missing,
    Ambiguous,
}

#[derive(Default)]
pub struct LinkIndex {
    by_note: HashMap<Uuid, NoteLinks>,
    names: NameLookup,
}

impl LinkIndex {
    pub fn build(notes: &[Note], notes_root: &Path) -> Self {
        let names = NameLookup::build(notes, notes_root);
        let mut by_note = HashMap::with_capacity(notes.len());

        for note in notes {
            by_note.insert(note.id, resolve_note_links(note, &names));
        }

        let mut index = Self { by_note, names };
        index.rebuild_backlinks();
        index
    }

    pub fn links_for(&self, note_id: Uuid) -> Option<&NoteLinks> {
        self.by_note.get(&note_id)
    }

    pub fn resolve_target(&self, target: &str) -> LinkResolution {
        self.names.resolve(target)
    }

    pub fn edges(&self) -> impl Iterator<Item = (Uuid, Uuid)> + '_ {
        self.by_note
            .iter()
            .flat_map(|(from, links)| links.outgoing.iter().map(|to| (*from, *to)))
    }

    /// Reparses one note without rescanning the vault.
    pub fn refresh_note_content(&mut self, note: &Note) {
        self.by_note
            .insert(note.id, resolve_note_links(note, &self.names));
        self.rebuild_backlinks();
    }

    fn rebuild_backlinks(&mut self) {
        for links in self.by_note.values_mut() {
            links.backlinks.clear();
        }

        let edges: Vec<(Uuid, Uuid)> = self
            .by_note
            .iter()
            .flat_map(|(from, links)| links.outgoing.iter().map(|to| (*from, *to)))
            .collect();

        for (from, to) in edges {
            if let Some(target_links) = self.by_note.get_mut(&to)
                && !target_links.backlinks.contains(&from)
            {
                target_links.backlinks.push(from);
            }
        }
    }
}

#[derive(Default)]
struct NameLookup {
    paths: HashMap<String, Vec<Uuid>>,
    titles: HashMap<String, Vec<Uuid>>,
    aliases: HashMap<String, Vec<Uuid>>,
    file_names: HashMap<String, Vec<Uuid>>,
}

impl NameLookup {
    fn build(notes: &[Note], notes_root: &Path) -> Self {
        let mut lookup = Self::default();

        for note in notes {
            insert_name(&mut lookup.titles, &note.title, note.id);
            for alias in &note.aliases {
                insert_name(&mut lookup.aliases, alias, note.id);
            }

            if let Some(stem) = note.file_path.file_stem().and_then(|stem| stem.to_str()) {
                insert_name(&mut lookup.file_names, stem, note.id);

                // Index both physical and UUID-free file names.
                if let Some((friendly, suffix)) = stem.rsplit_once("--")
                    && suffix.len() == 8
                    && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    insert_name(&mut lookup.file_names, friendly, note.id);
                }
            }

            if let Ok(relative_file) = note.file_path.strip_prefix(notes_root) {
                let parent = relative_file.parent().unwrap_or_else(|| Path::new(""));
                insert_path_name(&mut lookup.paths, &parent.join(&note.title), note.id);

                if let Some(stem) = relative_file.file_stem().and_then(|stem| stem.to_str()) {
                    let friendly_stem = stem
                        .rsplit_once("--")
                        .filter(|(_, suffix)| {
                            suffix.len() == 8 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
                        })
                        .map_or(stem, |(friendly, _)| friendly);
                    insert_path_name(&mut lookup.paths, &parent.join(friendly_stem), note.id);
                }
            }
        }

        lookup
    }

    fn resolve(&self, target: &str) -> LinkResolution {
        if target.contains(['/', '\\']) {
            return match match_name(&self.paths, &normalize_path_name(target)) {
                NameMatch::Missing => LinkResolution::Missing,
                NameMatch::Unique(id) => LinkResolution::Resolved(id),
                NameMatch::Ambiguous => LinkResolution::Ambiguous,
            };
        }

        let key = normalize_name(target);
        for names in [&self.titles, &self.aliases, &self.file_names] {
            match match_name(names, &key) {
                NameMatch::Unique(id) => return LinkResolution::Resolved(id),
                NameMatch::Ambiguous => return LinkResolution::Ambiguous,
                NameMatch::Missing => {}
            }
        }
        LinkResolution::Missing
    }
}

fn resolve_note_links(note: &Note, names: &NameLookup) -> NoteLinks {
    let mut links = NoteLinks::default();
    let mut seen_outgoing = HashSet::new();
    let mut seen_unresolved = HashSet::new();

    for wiki_link in extract_wiki_links(&note.content) {
        match names.resolve(&wiki_link.target) {
            LinkResolution::Resolved(target_id) => {
                if seen_outgoing.insert(target_id) {
                    links.outgoing.push(target_id);
                }
            }
            LinkResolution::Missing | LinkResolution::Ambiguous => {
                let normalized = normalize_name(&wiki_link.target);
                if seen_unresolved.insert(normalized) {
                    links.unresolved.push(wiki_link.target);
                }
            }
        }
    }

    links
}

fn insert_name(map: &mut HashMap<String, Vec<Uuid>>, name: &str, id: Uuid) {
    let normalized = normalize_name(name);
    if normalized.is_empty() {
        return;
    }

    let ids = map.entry(normalized).or_default();
    if !ids.contains(&id) {
        ids.push(id);
    }
}

fn insert_path_name(map: &mut HashMap<String, Vec<Uuid>>, path: &Path, id: Uuid) {
    let normalized = normalize_path_name(&path.to_string_lossy());
    if normalized.is_empty() {
        return;
    }
    let ids = map.entry(normalized).or_default();
    if !ids.contains(&id) {
        ids.push(id);
    }
}

enum NameMatch {
    Missing,
    Unique(Uuid),
    Ambiguous,
}

/// Never resolve duplicate names arbitrarily.
fn match_name(map: &HashMap<String, Vec<Uuid>>, key: &str) -> NameMatch {
    match map.get(key).map(Vec::as_slice) {
        None | Some([]) => NameMatch::Missing,
        Some([id]) => NameMatch::Unique(*id),
        Some(_) => NameMatch::Ambiguous,
    }
}

fn normalize_name(name: &str) -> String {
    name.trim()
        .strip_suffix(".md")
        .unwrap_or(name.trim())
        .trim()
        .to_lowercase()
}

fn normalize_path_name(path: &str) -> String {
    let normalized_separators = path.trim().replace('\\', "/");
    let without_extension = normalized_separators
        .strip_suffix(".md")
        .unwrap_or(&normalized_separators);
    without_extension
        .split('/')
        .map(str::trim)
        .filter(|component| !component.is_empty() && *component != ".")
        .collect::<Vec<_>>()
        .join("/")
        .to_lowercase()
}

/// Extracts supported Obsidian-style links.
pub fn extract_wiki_links(source: &str) -> Vec<WikiLink> {
    let mut links = Vec::new();
    let mut search_from = 0;

    while let Some(relative_start) = source[search_from..].find("[[") {
        let start = search_from + relative_start;
        let content_start = start + 2;

        if is_escaped(source, start) {
            search_from = content_start;
            continue;
        }

        let Some(relative_end) = source[content_start..].find("]]") else {
            break;
        };
        let content_end = content_start + relative_end;
        let expression_end = content_end + 2;
        let inner = source[content_start..content_end].trim();

        let (destination, label) = inner
            .split_once('|')
            .map_or((inner, None), |(left, right)| {
                let label = (!right.trim().is_empty()).then(|| right.trim().to_owned());
                (left.trim(), label)
            });
        let (target, heading) =
            destination
                .split_once('#')
                .map_or((destination, None), |(left, right)| {
                    let heading = (!right.trim().is_empty()).then(|| right.trim().to_owned());
                    (left.trim(), heading)
                });
        let target = target.trim().trim_end_matches(".md").trim();

        if !target.is_empty() {
            links.push(WikiLink {
                target: target.to_owned(),
                label,
                heading,
                source_range: start..expression_end,
            });
        }

        search_from = expression_end;
    }

    links
}

/// Finds a wiki-link at an egui character index.
pub fn wiki_link_at_character(source: &str, character_index: usize) -> Option<WikiLink> {
    let byte_index = source
        .char_indices()
        .nth(character_index)
        .map_or(source.len(), |(byte_index, _)| byte_index);

    extract_wiki_links(source)
        .into_iter()
        .find(|wiki_link| wiki_link.source_range.contains(&byte_index))
}

/// Splits a safe link target into folder and title.
pub fn split_target_path(target: &str) -> Option<(PathBuf, String)> {
    let normalized = target.trim().replace('\\', "/");
    let mut components: Vec<&str> = normalized.split('/').map(str::trim).collect();
    if components.is_empty()
        || components
            .iter()
            .any(|component| component.is_empty() || *component == "." || *component == "..")
    {
        return None;
    }

    let title = components.pop()?.trim_end_matches(".md").trim().to_owned();
    if title.is_empty() {
        return None;
    }
    let folder = components.iter().collect::<PathBuf>();
    Some((folder, title))
}

fn is_escaped(source: &str, marker_start: usize) -> bool {
    source[..marker_start]
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\\')
        .count()
        % 2
        == 1
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnresolvedLinkEntry {
    pub target: String,
    pub referencing_notes: Vec<(Uuid, String)>,
}

/// Collects all unresolved wiki-links across all notes in the vault.
#[allow(dead_code)]
pub fn collect_all_unresolved_links(
    notes: &[Note],
    link_index: &LinkIndex,
) -> Vec<UnresolvedLinkEntry> {
    let mut map: HashMap<String, Vec<(Uuid, String)>> = HashMap::new();

    for note in notes {
        if let Some(links) = link_index.links_for(note.id) {
            for target in &links.unresolved {
                let entry = map.entry(target.clone()).or_default();
                if !entry.iter().any(|(id, _)| *id == note.id) {
                    entry.push((note.id, note.title.clone()));
                }
            }
        }
    }

    let mut result: Vec<UnresolvedLinkEntry> = map
        .into_iter()
        .map(|(target, referencing_notes)| UnresolvedLinkEntry {
            target,
            referencing_notes,
        })
        .collect();

    result.sort_by(|a, b| a.target.cmp(&b.target));
    result
}

/// Replaces references to an old note title with a new title across all notes in the vault.
/// Handles `[[old_title]]`, `[[old_title|alias]]`, `[[old_title#heading]]`, `[[old_title#heading|alias]]`, and `[[folder/old_title]]`.
/// Returns every modified note so the caller can persist all changed files.
pub fn rename_note_references(notes: &mut [Note], old_title: &str, new_title: &str) -> Vec<Uuid> {
    let clean_old = old_title.trim().trim_end_matches(".md");
    let clean_new = new_title.trim().trim_end_matches(".md");
    if clean_old.is_empty() || clean_new.is_empty() || clean_old == clean_new {
        return Vec::new();
    }

    let mut modified_note_ids = Vec::new();

    for note in notes.iter_mut() {
        let links = extract_wiki_links(&note.content);
        if links.is_empty() {
            continue;
        }

        let mut replaced_content = String::with_capacity(note.content.len());
        let mut last_idx = 0;
        let mut changed = false;

        for link in links {
            let matches_target = link.target.eq_ignore_ascii_case(clean_old)
                || link.target.ends_with(&format!("/{clean_old}"))
                || link.target.ends_with(&format!("\\{clean_old}"));

            if matches_target {
                replaced_content.push_str(&note.content[last_idx..link.source_range.start]);

                // Construct new link target preserving directory prefix if present
                let new_target = if let Some((parent, _)) = link.target.rsplit_once('/') {
                    format!("{parent}/{clean_new}")
                } else if let Some((parent, _)) = link.target.rsplit_once('\\') {
                    format!("{parent}\\{clean_new}")
                } else {
                    clean_new.to_string()
                };

                let mut new_link_inner = new_target;
                if let Some(heading) = &link.heading {
                    new_link_inner.push('#');
                    new_link_inner.push_str(heading);
                }
                if let Some(label) = &link.label {
                    new_link_inner.push('|');
                    new_link_inner.push_str(label);
                }

                replaced_content.push_str(&format!("[[{new_link_inner}]]"));
                last_idx = link.source_range.end;
                changed = true;
            }
        }

        if changed {
            replaced_content.push_str(&note.content[last_idx..]);
            note.content = replaced_content;
            note.mark_as_updated();
            note.refresh_search_text();
            modified_note_ids.push(note.id);
        }
    }

    modified_note_ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn note(title: &str, aliases: &[&str], content: &str) -> Note {
        let mut note = Note::new(Path::new("."));
        note.title = title.to_owned();
        note.aliases = aliases.iter().map(|alias| (*alias).to_owned()).collect();
        note.content = content.to_owned();
        note.file_path = PathBuf::from(format!("{title}--1234abcd.md"));
        note
    }

    #[test]
    fn parser_extracts_aliases_headings_and_ranges() {
        let source = "See [[Target#Part|visible text]] and [[Second.md]].";
        let links = extract_wiki_links(source);

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "Target");
        assert_eq!(links[0].heading.as_deref(), Some("Part"));
        assert_eq!(links[0].label.as_deref(), Some("visible text"));
        assert_eq!(
            &source[links[0].source_range.clone()],
            "[[Target#Part|visible text]]"
        );
        assert_eq!(links[1].target, "Second");
    }

    #[test]
    fn parser_ignores_escaped_incomplete_and_empty_links() {
        let source = r"\[[escaped]] [[]] [[unfinished";

        assert!(extract_wiki_links(source).is_empty());
    }

    #[test]
    fn index_resolves_links_and_builds_backlinks() {
        let first = note(
            "First",
            &[],
            "[[Second]] [[Knowledge alias]] [[Missing]] [[Second]]",
        );
        let second = note("Second", &[], "");
        let knowledge = note("Knowledge", &["Knowledge alias"], "[[First]]");
        let first_id = first.id;
        let second_id = second.id;
        let knowledge_id = knowledge.id;

        let index = LinkIndex::build(&[first, second, knowledge], Path::new(""));

        assert_eq!(
            index.links_for(first_id).expect("first links").outgoing,
            vec![second_id, knowledge_id]
        );
        assert_eq!(
            index.links_for(first_id).expect("first links").unresolved,
            vec!["Missing"]
        );
        assert_eq!(
            index.links_for(second_id).expect("second links").backlinks,
            vec![first_id]
        );
        assert_eq!(
            index
                .links_for(first_id)
                .expect("first backlinks")
                .backlinks,
            vec![knowledge_id]
        );
    }

    #[test]
    fn duplicate_titles_are_not_resolved_arbitrarily() {
        let source = note("Source", &[], "[[Duplicate]]");
        let source_id = source.id;
        let first_duplicate = note("Duplicate", &[], "");
        let second_duplicate = note("Duplicate", &[], "");

        let index = LinkIndex::build(&[source, first_duplicate, second_duplicate], Path::new(""));
        let links = index.links_for(source_id).expect("source links");

        assert!(links.outgoing.is_empty());
        assert_eq!(links.unresolved, vec!["Duplicate"]);
        assert_eq!(index.resolve_target("Duplicate"), LinkResolution::Ambiguous);
    }

    #[test]
    fn link_under_character_handles_cyrillic_before_the_link() {
        let source = "Привет [[Цель|текст]] после";
        let character_index = "Привет [[Цель".chars().count();
        let link = wiki_link_at_character(source, character_index).expect("link under cursor");

        assert_eq!(link.target, "Цель");
    }

    #[test]
    fn path_target_disambiguates_duplicate_titles() {
        let root = Path::new("vault/Notes");
        let source = note("Source", &[], "[[Programming/Rust Tips]]");
        let source_id = source.id;
        let mut programming = note("Rust Tips", &[], "");
        programming.file_path = root.join("Programming/Rust Tips--1234abcd.md");
        let programming_id = programming.id;
        let mut archive = note("Rust Tips", &[], "");
        archive.file_path = root.join("Archive/Rust Tips--87654321.md");

        let index = LinkIndex::build(&[source, programming, archive], root);

        assert_eq!(
            index.links_for(source_id).expect("source links").outgoing,
            vec![programming_id]
        );
    }

    #[test]
    fn target_path_is_safe_to_use_for_note_creation() {
        assert_eq!(
            split_target_path(r"Programming\Rust Tips.md"),
            Some((PathBuf::from("Programming"), "Rust Tips".to_owned()))
        );
        assert_eq!(split_target_path("../Outside"), None);
        assert_eq!(split_target_path("Folder//Note"), None);
    }

    #[test]
    fn renames_note_references_preserving_headings_and_aliases() {
        let note1 = note(
            "Caller",
            &[],
            "See [[Old Note]], [[Old Note#Heading]], [[Old Note|Custom Alias]], and [[Folder/Old Note#Sub|Label]].",
        );
        let mut notes = vec![note1];

        let modified = rename_note_references(&mut notes, "Old Note", "New Name");
        assert_eq!(modified, vec![notes[0].id]);
        assert_eq!(
            notes[0].content,
            "See [[New Name]], [[New Name#Heading]], [[New Name|Custom Alias]], and [[Folder/New Name#Sub|Label]]."
        );
    }

    #[test]
    fn collects_unresolved_links_across_vault() {
        let note1 = note(
            "Alpha",
            &[],
            "Links to [[Dead Link]] and [[Missing Target]].",
        );
        let note2 = note("Beta", &[], "Also links to [[Dead Link]].");
        let index = LinkIndex::build(&[note1.clone(), note2.clone()], Path::new(""));

        let unresolved = collect_all_unresolved_links(&[note1, note2], &index);
        assert_eq!(unresolved.len(), 2);
        assert_eq!(unresolved[0].target, "Dead Link");
        assert_eq!(unresolved[0].referencing_notes.len(), 2);
        assert_eq!(unresolved[1].target, "Missing Target");
        assert_eq!(unresolved[1].referencing_notes.len(), 1);
    }
}
