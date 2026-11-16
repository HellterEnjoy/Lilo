//! UI-independent editor helpers: text statistics, in-note find/replace and
//! `[[wiki link]]` / `#tag` completion.
//!
//! Every position in this module is a character index, matching egui's
//! `CCursor`, so results can be passed to the text editor directly.

use std::ops::Range;

/// Average silent reading speed used for the reading-time estimate.
const WORDS_PER_MINUTE: usize = 220;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextStats {
    pub words: usize,
    pub chars: usize,
    pub chars_without_spaces: usize,
    pub lines: usize,
}

impl TextStats {
    pub fn of(text: &str) -> Self {
        let mut stats = Self {
            lines: if text.is_empty() {
                0
            } else {
                text.lines().count().max(1)
            },
            ..Self::default()
        };
        for character in text.chars() {
            stats.chars += 1;
            if !character.is_whitespace() {
                stats.chars_without_spaces += 1;
            }
        }
        stats.words = text
            .split_whitespace()
            .filter(|word| word.chars().any(char::is_alphanumeric))
            .count();
        stats
    }

    /// Whole minutes needed to read the text; at least one for non-empty text.
    pub fn reading_minutes(&self) -> usize {
        if self.words == 0 {
            0
        } else {
            self.words.div_ceil(WORDS_PER_MINUTE)
        }
    }
}

/// Returns the characters of `text` in `range` (character indices).
pub fn char_slice(text: &str, range: Range<usize>) -> &str {
    let start = char_to_byte(text, range.start);
    let end = char_to_byte(text, range.end.max(range.start));
    &text[start..end]
}

/// Finds every non-overlapping occurrence of `query`.
pub fn find_matches(text: &str, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let needle: Vec<char> = fold(query, case_sensitive).collect();
    let haystack: Vec<char> = text
        .chars()
        .map(|character| fold_char(character, case_sensitive))
        .collect();
    let mut matches = Vec::new();
    let mut index = 0;
    while index + needle.len() <= haystack.len() {
        if haystack[index..index + needle.len()] == needle[..] {
            matches.push(index..index + needle.len());
            index += needle.len();
        } else {
            index += 1;
        }
    }
    matches
}

/// Replaces the characters in `range` and returns the range of the inserted text.
pub fn replace_range(text: &mut String, range: Range<usize>, replacement: &str) -> Range<usize> {
    let start = char_to_byte(text, range.start);
    let end = char_to_byte(text, range.end);
    text.replace_range(start..end, replacement);
    range.start..range.start + replacement.chars().count()
}

/// Replaces every occurrence of `query` and returns the number of replacements.
pub fn replace_all(
    text: &mut String,
    query: &str,
    replacement: &str,
    case_sensitive: bool,
) -> usize {
    let matches = find_matches(text, query, case_sensitive);
    for range in matches.iter().rev() {
        replace_range(text, range.clone(), replacement);
    }
    matches.len()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    WikiLink,
    Tag,
}

/// The partially typed link or tag immediately before the caret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionContext {
    pub kind: CompletionKind,
    /// First character of the typed query (just after `[[` or `#`).
    pub start: usize,
    /// The caret position the context was computed for.
    pub end: usize,
    pub query: String,
}

/// Detects whether the caret is inside an unfinished `[[link` or `#tag`.
///
/// Fenced code blocks and inline code are ignored, and a `#` at the start of
/// a line followed by a space (a heading) never triggers tag completion.
pub fn completion_context(text: &str, cursor: usize) -> Option<CompletionContext> {
    let cursor_byte = char_to_byte(text, cursor);
    let before = &text[..cursor_byte];
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let line = &before[line_start..];
    if inside_fenced_block(&before[..line_start]) || line.matches('`').count() % 2 == 1 {
        return None;
    }

    if let Some(open) = line.rfind("[[") {
        let query = &line[open + 2..];
        if !query.contains("]]") && !query.contains('|') && !query.contains('#') {
            return Some(CompletionContext {
                kind: CompletionKind::WikiLink,
                start: cursor - query.chars().count(),
                end: cursor,
                query: query.to_owned(),
            });
        }
    }

    let hash = line.rfind('#')?;
    let query = &line[hash + 1..];
    let preceded_by_boundary = line[..hash]
        .chars()
        .next_back()
        .is_none_or(|character| character.is_whitespace() || character == '(');
    let valid_tag = !query.is_empty()
        && query
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '-' | '_' | '/'))
        && query.chars().next().is_some_and(|c| !c.is_ascii_digit());
    (preceded_by_boundary && valid_tag).then(|| CompletionContext {
        kind: CompletionKind::Tag,
        start: cursor - query.chars().count(),
        end: cursor,
        query: query.to_owned(),
    })
}

/// Inserts the chosen completion and returns the new caret position.
pub fn apply_completion(text: &mut String, context: &CompletionContext, value: &str) -> usize {
    match context.kind {
        CompletionKind::WikiLink => {
            let closed = char_slice(text, context.end..context.end + 2) == "]]";
            let replacement = if closed {
                value.to_owned()
            } else {
                format!("{value}]]")
            };
            let inserted = replace_range(text, context.start..context.end, &replacement);
            if closed {
                inserted.end + 2
            } else {
                inserted.end
            }
        }
        CompletionKind::Tag => replace_range(text, context.start..context.end, value).end,
    }
}

/// Ranks `candidates` against `query` with the shared fuzzy scorer, best first.
pub fn rank<'a, I>(query: &str, candidates: I, limit: usize) -> Vec<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut scored: Vec<(i64, &str)> = candidates
        .into_iter()
        .filter(|candidate| !candidate.trim().is_empty())
        .filter_map(|candidate| {
            crate::commands::fuzzy_score(query, candidate).map(|score| (score, candidate))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.len().cmp(&b.1.len())));
    scored.dedup_by(|a, b| a.1.eq_ignore_ascii_case(b.1));
    scored.truncate(limit);
    scored.into_iter().map(|(_, candidate)| candidate).collect()
}

fn inside_fenced_block(text: &str) -> bool {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("```") || trimmed.starts_with("~~~")
        })
        .count()
        % 2
        == 1
}

fn fold(text: &str, case_sensitive: bool) -> impl Iterator<Item = char> + '_ {
    text.chars()
        .map(move |character| fold_char(character, case_sensitive))
}

/// Simple one-to-one case folding keeps character indices stable.
fn fold_char(character: char, case_sensitive: bool) -> char {
    if case_sensitive {
        character
    } else {
        let mut lower = character.to_lowercase();
        match (lower.next(), lower.next()) {
            (Some(single), None) => single,
            _ => character,
        }
    }
}

fn char_to_byte(text: &str, character_index: usize) -> usize {
    text.char_indices()
        .nth(character_index)
        .map_or(text.len(), |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(text_with_caret: &str) -> Option<CompletionContext> {
        let cursor = text_with_caret.find('^').unwrap();
        let cursor_chars = text_with_caret[..cursor].chars().count();
        let text = text_with_caret.replacen('^', "", 1);
        completion_context(&text, cursor_chars)
    }

    #[test]
    fn stats_count_words_lines_and_reading_time() {
        let stats = TextStats::of("# Заголовок\n\nOne two — three.\n");
        assert_eq!(stats.words, 4);
        assert_eq!(stats.lines, 3);
        assert_eq!(stats.reading_minutes(), 1);
        assert_eq!(TextStats::of("").reading_minutes(), 0);
        assert_eq!(TextStats::of("a b").chars_without_spaces, 2);
    }

    #[test]
    fn find_is_case_insensitive_and_unicode_safe() {
        let text = "Привет мир, ПРИВЕТ снова";
        assert_eq!(find_matches(text, "привет", false), vec![0..6, 12..18]);
        assert_eq!(
            find_matches(text, "привет", true),
            Vec::<Range<usize>>::new()
        );
        assert_eq!(find_matches("aaaa", "aa", true), vec![0..2, 2..4]);
        assert!(find_matches("text", "", false).is_empty());
    }

    #[test]
    fn replace_all_and_single_replacement() {
        let mut text = "Cat cat CAT".to_owned();
        assert_eq!(replace_all(&mut text, "cat", "кот", false), 3);
        assert_eq!(text, "кот кот кот");
        let inserted = replace_range(&mut text, 4..7, "пёс");
        assert_eq!(text, "кот пёс кот");
        assert_eq!(inserted, 4..7);
    }

    #[test]
    fn wiki_link_context_and_completion() {
        let found = context("See [[Proj^").unwrap();
        assert_eq!(found.kind, CompletionKind::WikiLink);
        assert_eq!(found.query, "Proj");

        let mut text = "See [[Proj".to_owned();
        let caret = apply_completion(&mut text, &found, "Project notes");
        assert_eq!(text, "See [[Project notes]]");
        assert_eq!(caret, text.chars().count());

        let mut closed = "See [[Pro]] later".to_owned();
        let found = completion_context(&closed, 9).unwrap();
        let caret = apply_completion(&mut closed, &found, "Project");
        assert_eq!(closed, "See [[Project]] later");
        assert_eq!(caret, 15);

        assert!(context("[[Done]] and ^").is_none());
        assert!(context("[[Note|alias^").is_none());
    }

    #[test]
    fn tag_context_ignores_headings_code_and_words() {
        let found = context("Idea #wo^").unwrap();
        assert_eq!(found.kind, CompletionKind::Tag);
        assert_eq!(found.query, "wo");
        assert_eq!(context("#проект/ле^").unwrap().query, "проект/ле");
        assert!(context("# Heading^").is_none());
        assert!(context("C#^").is_none());
        assert!(context("issue #12^").is_none());
        assert!(context("`#code^").is_none());
        assert!(context("```\n#inside^").is_none());
        assert!(context("```\ncode\n```\n#after^").is_some());
    }

    #[test]
    fn ranking_prefers_prefix_and_deduplicates() {
        let ranked = rank(
            "pro",
            ["Groceries", "Project", "project", "Prototype", "Other"],
            5,
        );
        assert_eq!(ranked[0], "Project");
        assert!(!ranked.contains(&"Other"));
        assert_eq!(
            ranked
                .iter()
                .filter(|c| c.eq_ignore_ascii_case("project"))
                .count(),
            1
        );
    }
}
