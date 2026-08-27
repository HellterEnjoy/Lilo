//! Structured search parser and filter engine with quote support, multi-tag filtering, and negated operators.

use crate::storage::Note;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub tags: Vec<String>,
    pub negated_tags: Vec<String>,
    pub paths: Vec<String>,
    pub negated_paths: Vec<String>,
    pub links: Vec<String>,
    pub titles: Vec<String>,
    pub text_terms: Vec<String>,
    pub negated_terms: Vec<String>,
}

impl SearchQuery {
    /// Parses a raw search string into structured operators, multi-tags, and text terms.
    /// Supports quotes and negations, e.g.:
    /// `tag:"rust/advanced tips" -tag:draft path:"Daily Notes" link:"Target Note" -term title:"My Title" keyword`
    pub fn parse(raw: &str) -> Self {
        let mut query = Self::default();
        let tokens = tokenize(raw);

        for token in tokens {
            let (is_negated, body) = if let Some(rest) = token.strip_prefix('-') {
                (true, rest)
            } else {
                (false, token.as_str())
            };

            if let Some(rest) = body.strip_prefix("tag:") {
                for sub_tag in rest.split(',') {
                    let clean = sub_tag.trim().trim_start_matches('#').trim().to_lowercase();
                    if !clean.is_empty() {
                        if is_negated {
                            query.negated_tags.push(clean);
                        } else {
                            query.tags.push(clean);
                        }
                    }
                }
            } else if let Some(rest) = body.strip_prefix('#') {
                for sub_tag in rest.split(',') {
                    let clean = sub_tag.trim().to_lowercase();
                    if !clean.is_empty() {
                        if is_negated {
                            query.negated_tags.push(clean);
                        } else {
                            query.tags.push(clean);
                        }
                    }
                }
            } else if let Some(rest) = body.strip_prefix("path:") {
                let clean = rest.trim().replace('\\', "/").to_lowercase();
                if !clean.is_empty() {
                    if is_negated {
                        query.negated_paths.push(clean);
                    } else {
                        query.paths.push(clean);
                    }
                }
            } else if let Some(rest) = body.strip_prefix("folder:") {
                let clean = rest.trim().replace('\\', "/").to_lowercase();
                if !clean.is_empty() {
                    if is_negated {
                        query.negated_paths.push(clean);
                    } else {
                        query.paths.push(clean);
                    }
                }
            } else if let Some(rest) = body.strip_prefix("link:") {
                let clean = rest.trim().to_lowercase();
                if !clean.is_empty() {
                    query.links.push(clean);
                }
            } else if let Some(rest) = body.strip_prefix("title:") {
                let clean = rest.trim().to_lowercase();
                if !clean.is_empty() {
                    query.titles.push(clean);
                }
            } else {
                let clean = body.trim().to_lowercase();
                if !clean.is_empty() {
                    if is_negated {
                        query.negated_terms.push(clean);
                    } else {
                        query.text_terms.push(clean);
                    }
                }
            }
        }

        query
    }

    /// Returns true if this search query has any active filters or text terms.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
            && self.negated_tags.is_empty()
            && self.paths.is_empty()
            && self.negated_paths.is_empty()
            && self.links.is_empty()
            && self.titles.is_empty()
            && self.text_terms.is_empty()
            && self.negated_terms.is_empty()
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

        // Tag matching
        let note_tags: Vec<String> = note
            .tags
            .iter()
            .map(|t| t.trim_start_matches('#').to_lowercase())
            .collect();

        // 1. Required tags (AND logic)
        for req_tag in &self.tags {
            let matched = note_tags
                .iter()
                .any(|t| t == req_tag || t.starts_with(&format!("{req_tag}/")))
                || note.content.to_lowercase().contains(&format!("#{req_tag}"));
            if !matched {
                return false;
            }
        }

        // 2. Negated tags
        for neg_tag in &self.negated_tags {
            let matched = note_tags
                .iter()
                .any(|t| t == neg_tag || t.starts_with(&format!("{neg_tag}/")))
                || note.content.to_lowercase().contains(&format!("#{neg_tag}"));
            if matched {
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
        for neg_path in &self.negated_paths {
            if folder_str.contains(neg_path) {
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
        for neg_term in &self.negated_terms {
            if note.search_text.contains(neg_term) {
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

    #[test]
    fn handles_negated_tags_and_comma_separated_tags() {
        let mut note = Note::new(Path::new("."));
        note.title = "Task List".to_owned();
        note.tags = vec!["todo".to_owned(), "work".to_owned()];
        note.refresh_search_text();

        let query = SearchQuery::parse(r#"tag:todo -tag:archive"#);
        assert!(query.matches_note(&note, Path::new(""), &[]));

        let query_negated_match = SearchQuery::parse(r#"tag:todo -tag:work"#);
        assert!(!query_negated_match.matches_note(&note, Path::new(""), &[]));

        let query_multi = SearchQuery::parse(r#"tag:todo,work"#);
        assert_eq!(query_multi.tags, vec!["todo", "work"]);
        assert!(query_multi.matches_note(&note, Path::new(""), &[]));
    }
}
