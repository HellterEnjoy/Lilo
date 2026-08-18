//! Structured search parser and filter engine with quote support and tag/path/link operators.

use crate::storage::Note;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub tags: Vec<String>,
    pub paths: Vec<String>,
    pub links: Vec<String>,
    pub titles: Vec<String>,
    pub text_terms: Vec<String>,
}

impl SearchQuery {
    /// Parses a raw search string into structured operators and text terms.
    /// Supports quotes, e.g.:
    /// `tag:"rust/advanced tips" path:"Daily Notes" link:"Target Note" title:"My Title" keyword`
    pub fn parse(raw: &str) -> Self {
        let mut query = Self::default();
        let tokens = tokenize(raw);

        for token in tokens {
            if let Some(rest) = token.strip_prefix("tag:") {
                let clean = rest.trim_start_matches('#').trim().to_lowercase();
                if !clean.is_empty() {
                    query.tags.push(clean);
                }
            } else if let Some(rest) = token.strip_prefix('#') {
                let clean = rest.trim().to_lowercase();
                if !clean.is_empty() {
                    query.tags.push(clean);
                }
            } else if let Some(rest) = token.strip_prefix("path:") {
                let clean = rest.trim().replace('\\', "/").to_lowercase();
                if !clean.is_empty() {
                    query.paths.push(clean);
                }
            } else if let Some(rest) = token.strip_prefix("folder:") {
                let clean = rest.trim().replace('\\', "/").to_lowercase();
                if !clean.is_empty() {
                    query.paths.push(clean);
                }
            } else if let Some(rest) = token.strip_prefix("link:") {
                let clean = rest.trim().to_lowercase();
                if !clean.is_empty() {
                    query.links.push(clean);
                }
            } else if let Some(rest) = token.strip_prefix("title:") {
                let clean = rest.trim().to_lowercase();
                if !clean.is_empty() {
                    query.titles.push(clean);
                }
            } else {
                let clean = token.trim().to_lowercase();
                if !clean.is_empty() {
                    query.text_terms.push(clean);
                }
            }
        }

        query
    }

    /// Returns true if this search query has any active filters or text terms.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
            && self.paths.is_empty()
            && self.links.is_empty()
            && self.titles.is_empty()
            && self.text_terms.is_empty()
    }

    /// Evaluates whether a note matches all conditions of this search query.
    pub fn matches_note(
        &self,
        note: &Note,
        relative_folder: &Path,
        outgoing_link_targets: &[String],
    ) -> bool {
        if self.is_empty() {
            return true;
        }

        // Tag matching: note must match all requested tags
        let note_tags: Vec<String> = note
            .tags
            .iter()
            .map(|t| t.trim_start_matches('#').to_lowercase())
            .collect();

        for req_tag in &self.tags {
            let matched = note_tags
                .iter()
                .any(|t| t == req_tag || t.starts_with(&format!("{req_tag}/")))
                || note.content.to_lowercase().contains(&format!("#{req_tag}"));
            if !matched {
                return false;
            }
        }

        // Path / folder matching
        let folder_str = relative_folder
            .to_string_lossy()
            .replace('\\', "/")
            .to_lowercase();
        for req_path in &self.paths {
            if !folder_str.contains(req_path) {
                return false;
            }
        }

        // Link matching
        for req_link in &self.links {
            let matched = outgoing_link_targets
                .iter()
                .any(|link| link.to_lowercase().contains(req_link));
            if !matched {
                return false;
            }
        }

        // Title matching
        for req_title in &self.titles {
            let title_match = note.title.to_lowercase().contains(req_title)
                || note
                    .aliases
                    .iter()
                    .any(|a| a.to_lowercase().contains(req_title));
            if !title_match {
                return false;
            }
        }

        // General text terms matching
        for term in &self.text_terms {
            if !note.search_text.contains(term) {
                return false;
            }
        }

        true
    }
}

/// Tokenizes a query string while respecting double quotes.
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                    current.clear();
                }
            }
            other => {
                current.push(other);
            }
        }
    }

    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_operators_with_and_without_quotes() {
        let raw = r##"tag:"#rust/advanced tips" path:"Daily Notes" link:"Borrowing" title:"Memory" #learning simple"##;
        let query = SearchQuery::parse(raw);

        assert_eq!(query.tags, vec!["rust/advanced tips", "learning"]);
        assert_eq!(query.paths, vec!["daily notes"]);
        assert_eq!(query.links, vec!["borrowing"]);
        assert_eq!(query.titles, vec!["memory"]);
        assert_eq!(query.text_terms, vec!["simple"]);
    }

    #[test]
    fn matches_note_with_nested_tags_and_paths() {
        let mut note = Note::new(Path::new("."));
        note.title = "Memory Model".to_owned();
        note.content = "Content discussing [[Borrowing]] rules.".to_owned();
        note.tags = vec!["rust/advanced tips".to_owned()];
        note.refresh_search_text();

        let query = SearchQuery::parse(r#"tag:rust path:"daily" link:"Borrow""#);
        let folder = PathBuf::from("Daily/2026");
        let links = vec!["Borrowing".to_owned()];

        assert!(query.matches_note(&note, &folder, &links));
    }

    #[test]
    fn rejects_non_matching_query() {
        let mut note = Note::new(Path::new("."));
        note.title = "General Topics".to_owned();
        note.content = "Unrelated text".to_owned();
        note.refresh_search_text();

        let query = SearchQuery::parse(r#"tag:rust"#);
        let folder = PathBuf::new();
        let links = vec![];

        assert!(!query.matches_note(&note, &folder, &links));
    }
}
