//! Tag indexing, hierarchy building, and vault-wide safe tag renaming.

use crate::storage::Note;
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagInfo {
    pub tag: String,
    pub count: usize,
    pub note_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagTreeNode {
    pub name: String,
    pub full_tag: String,
    pub count: usize,
    pub direct_count: usize,
    pub children: Vec<TagTreeNode>,
}

#[derive(Default, Clone, Debug)]
pub struct TagIndex {
    #[allow(dead_code)]
    by_tag: HashMap<String, Vec<Uuid>>,
    all_tags: Vec<TagInfo>,
}

impl TagIndex {
    /// Builds a tag index across all notes in the vault.
    pub fn build(notes: &[Note]) -> Self {
        let mut by_tag: HashMap<String, Vec<Uuid>> = HashMap::new();

        for note in notes {
            let note_tags = extract_all_tags_from_note(note);
            for tag in note_tags {
                let list = by_tag.entry(tag).or_default();
                if !list.contains(&note.id) {
                    list.push(note.id);
                }
            }
        }

        let mut all_tags: Vec<TagInfo> = by_tag
            .iter()
            .map(|(tag, note_ids)| TagInfo {
                tag: tag.clone(),
                count: note_ids.len(),
                note_ids: note_ids.clone(),
            })
            .collect();

        all_tags.sort_by(|a, b| a.tag.cmp(&b.tag));

        Self { by_tag, all_tags }
    }

    pub fn all_tags(&self) -> &[TagInfo] {
        &self.all_tags
    }

    #[allow(dead_code)]
    pub fn notes_for_tag(&self, tag: &str) -> Option<&[Uuid]> {
        let clean = clean_tag(tag);
        self.by_tag.get(&clean).map(Vec::as_slice)
    }

    /// Builds a hierarchical tree for nested tags like `#project/lilo/release`.
    pub fn build_tree(&self) -> Vec<TagTreeNode> {
        let mut root_map: BTreeMap<String, RawTagNode> = BTreeMap::new();

        for tag_info in &self.all_tags {
            let parts: Vec<&str> = tag_info.tag.split('/').filter(|p| !p.is_empty()).collect();
            if parts.is_empty() {
                continue;
            }

            let mut current_map = &mut root_map;
            let mut prefix = String::new();

            for (idx, &part) in parts.iter().enumerate() {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);

                let is_leaf = idx == parts.len() - 1;
                let entry = current_map
                    .entry(part.to_string())
                    .or_insert_with(|| RawTagNode {
                        name: part.to_string(),
                        full_tag: prefix.clone(),
                        direct_count: 0,
                        children: BTreeMap::new(),
                    });

                if is_leaf {
                    entry.direct_count = tag_info.count;
                }

                current_map = &mut entry.children;
            }
        }

        fn convert_node(raw: RawTagNode) -> TagTreeNode {
            let children: Vec<TagTreeNode> = raw.children.into_values().map(convert_node).collect();
            let total_count = raw.direct_count + children.iter().map(|c| c.count).sum::<usize>();
            TagTreeNode {
                name: raw.name,
                full_tag: raw.full_tag,
                count: total_count,
                direct_count: raw.direct_count,
                children,
            }
        }

        root_map.into_values().map(convert_node).collect()
    }
}

struct RawTagNode {
    name: String,
    full_tag: String,
    direct_count: usize,
    children: BTreeMap<String, RawTagNode>,
}

/// Normalizes tag string by stripping `#`, trimming, and converting to lowercase.
pub fn clean_tag(tag: &str) -> String {
    tag.trim()
        .trim_start_matches('#')
        .trim()
        .trim_matches('/')
        .to_lowercase()
}

/// Extracts all unique tags from frontmatter and content for a given note.
pub fn extract_all_tags_from_note(note: &Note) -> HashSet<String> {
    let mut tags = HashSet::new();

    // 1. Frontmatter tags
    for tag in &note.tags {
        let cleaned = clean_tag(tag);
        if !cleaned.is_empty() {
            tags.insert(cleaned);
        }
    }

    // 2. Inline hashtags from content
    for inline in extract_inline_tags(&note.content) {
        tags.insert(inline);
    }

    tags
}

/// Extracts inline `#tag` or `#parent/subtag` identifiers from Markdown text.
/// Ignores code blocks, inline code, and URLs/hex colors.
pub fn extract_inline_tags(source: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut in_code_block = false;
    let mut in_inline_code = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            if chars[i] == '`' {
                in_inline_code = !in_inline_code;
                i += 1;
                continue;
            }
            if in_inline_code {
                i += 1;
                continue;
            }

            if chars[i] == '#' {
                // Must be preceded by start of line or whitespace or punctuation
                let valid_prefix = if i == 0 {
                    true
                } else {
                    let prev = chars[i - 1];
                    prev.is_whitespace() || matches!(prev, '(' | '[' | '{' | ',' | ';' | ':' | '>')
                };

                if valid_prefix && i + 1 < len {
                    let next = chars[i + 1];
                    // Header # must be followed by space; tags cannot start with space, digit, or special chars
                    if !next.is_whitespace()
                        && next != '#'
                        && !next.is_ascii_digit()
                        && (next.is_alphanumeric() || next == '_' || next == '/')
                    {
                        let mut end = i + 1;
                        while end < len {
                            let ch = chars[end];
                            if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '/' {
                                end += 1;
                            } else {
                                break;
                            }
                        }

                        let tag_candidate: String = chars[i + 1..end].iter().collect();
                        let cleaned = clean_tag(&tag_candidate);
                        if !cleaned.is_empty() && !cleaned.bytes().all(|b| b.is_ascii_hexdigit()) {
                            // Don't treat hex colors like #fff or #aabbcc as tags if they are 3 or 6 hex digits
                            if !(cleaned.len() == 3 || cleaned.len() == 6)
                                || cleaned.chars().any(|c| !c.is_ascii_hexdigit())
                            {
                                results.push(cleaned);
                            }
                        }
                        i = end;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    results
}

/// Renames a tag in memory and returns every note that must be persisted.
pub fn rename_tag_in_vault(notes: &mut [Note], old_tag: &str, new_tag: &str) -> Vec<Uuid> {
    let clean_old = clean_tag(old_tag);
    let clean_new = clean_tag(new_tag);

    if clean_old.is_empty() || clean_new.is_empty() || clean_old == clean_new {
        return Vec::new();
    }

    let mut modified_note_ids = Vec::new();
    let old_hashtag = format!("#{clean_old}");
    let new_hashtag = format!("#{clean_new}");

    for note in notes.iter_mut() {
        let mut note_changed = false;

        // 1. Update frontmatter tags (both exact and nested subtags)
        let mut new_tags = Vec::with_capacity(note.tags.len());
        for t in &note.tags {
            let cleaned = clean_tag(t);
            if cleaned == clean_old {
                if !new_tags
                    .iter()
                    .any(|existing: &String| clean_tag(existing) == clean_new)
                {
                    new_tags.push(clean_new.clone());
                }
                note_changed = true;
            } else if cleaned.starts_with(&format!("{clean_old}/")) {
                let suffix = &cleaned[clean_old.len()..];
                let sub_new = format!("{clean_new}{suffix}");
                if !new_tags
                    .iter()
                    .any(|existing: &String| clean_tag(existing) == sub_new)
                {
                    new_tags.push(sub_new);
                }
                note_changed = true;
            } else if !new_tags
                .iter()
                .any(|existing: &String| clean_tag(existing) == cleaned)
            {
                new_tags.push(t.clone());
            }
        }
        if note_changed {
            note.tags = new_tags;
        }

        // 2. Update inline hashtags in content
        if note.content.to_lowercase().contains(&old_hashtag) {
            let replaced = replace_hashtag_in_content(&note.content, &clean_old, &new_hashtag);
            if replaced != note.content {
                note.content = replaced;
                note_changed = true;
            }
        }

        if note_changed {
            note.mark_as_updated();
            note.refresh_search_text();
            modified_note_ids.push(note.id);
        }
    }

    modified_note_ids
}

/// Helper function to replace hashtags in markdown while respecting word boundaries and nested subtags.
fn replace_hashtag_in_content(content: &str, old_tag: &str, new_hashtag: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let old_lower = old_tag.to_lowercase();
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '#' {
            let valid_prefix = if i == 0 {
                true
            } else {
                let prev = chars[i - 1];
                prev.is_whitespace() || matches!(prev, '(' | '[' | '{' | ',' | ';' | ':' | '>')
            };

            if valid_prefix && i + 1 < len {
                let mut end = i + 1;
                while end < len {
                    let ch = chars[end];
                    if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '/' {
                        end += 1;
                    } else {
                        break;
                    }
                }

                let tag_candidate: String = chars[i + 1..end].iter().collect();
                let candidate_clean = clean_tag(&tag_candidate);
                if candidate_clean == old_lower {
                    result.push_str(new_hashtag);
                    i = end;
                    continue;
                } else if candidate_clean.starts_with(&format!("{old_lower}/")) {
                    let suffix = &candidate_clean[old_lower.len()..];
                    result.push_str(&format!("{new_hashtag}{suffix}"));
                    i = end;
                    continue;
                }
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn extracts_frontmatter_and_inline_tags() {
        let mut note = Note::new(Path::new("."));
        note.tags = vec!["daily".to_owned(), "rust/core".to_owned()];
        note.content =
            "Discussion on #rust/async and #productivity!\n```rust\n#not_a_tag\n```".to_owned();

        let tags = extract_all_tags_from_note(&note);
        assert!(tags.contains("daily"));
        assert!(tags.contains("rust/core"));
        assert!(tags.contains("rust/async"));
        assert!(tags.contains("productivity"));
        assert!(!tags.contains("not_a_tag"));
    }

    #[test]
    fn builds_hierarchical_tag_tree() {
        let mut note1 = Note::new(Path::new("."));
        note1.tags = vec!["dev/rust/async".to_owned(), "dev/rust/macros".to_owned()];

        let mut note2 = Note::new(Path::new("."));
        note2.tags = vec!["dev/go".to_owned(), "daily".to_owned()];

        let index = TagIndex::build(&[note1, note2]);
        let tree = index.build_tree();

        assert_eq!(tree.len(), 2); // "daily" and "dev"
        let dev_node = tree.iter().find(|n| n.name == "dev").unwrap();
        assert_eq!(dev_node.count, 3);
        assert_eq!(dev_node.children.len(), 2); // "go" and "rust"
        assert_eq!(index.notes_for_tag("daily").unwrap().len(), 1);
        assert_eq!(index.notes_for_tag("dev/rust/async").unwrap().len(), 1);
    }

    #[test]
    fn renames_tag_across_vault_frontmatter_and_content() {
        let mut note1 = Note::new(Path::new("."));
        note1.title = "Note 1".to_owned();
        note1.tags = vec!["todo".to_owned()];
        note1.content = "Action item with #todo here.".to_owned();

        let mut note2 = Note::new(Path::new("."));
        note2.title = "Note 2".to_owned();
        note2.tags = vec!["other".to_owned()];
        note2.content = "No tag here.".to_owned();

        let mut notes = vec![note1, note2];
        let modified = rename_tag_in_vault(&mut notes, "todo", "task");

        assert_eq!(modified, vec![notes[0].id]);
        assert_eq!(notes[0].tags, vec!["task".to_owned()]);
        assert_eq!(notes[0].content, "Action item with #task here.");
    }

    #[test]
    fn renames_subtag_namespaces_across_vault() {
        let mut note1 = Note::new(Path::new("."));
        note1.tags = vec!["project/lilo".to_owned()];
        note1.content = "Tracked in #project/lilo and #project/lilo/ui today.".to_owned();

        let mut notes = vec![note1];
        let modified = rename_tag_in_vault(&mut notes, "project/lilo", "app/lilo");

        assert_eq!(modified, vec![notes[0].id]);
        assert_eq!(notes[0].tags, vec!["app/lilo".to_owned()]);
        assert_eq!(
            notes[0].content,
            "Tracked in #app/lilo and #app/lilo/ui today."
        );
    }
}
