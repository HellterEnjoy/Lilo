//! Declarative Markdown template engine with Unicode character-indexed cursor positioning.

use chrono::{DateTime, Duration, Local};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateEntry {
    pub name: String,
    pub relative_path: PathBuf,
}

pub struct TemplateEngine;

impl TemplateEngine {
    /// Strips YAML frontmatter from markdown text if present.
    pub fn strip_frontmatter(raw: &str) -> &str {
        let normalized = raw.trim_start();
        if let Some(after_opening) = normalized.strip_prefix("---\n") {
            if let Some(closing_pos) = after_opening.find("\n---\n") {
                return after_opening[closing_pos + "\n---\n".len()..]
                    .trim_start_matches(['\r', '\n']);
            }
        } else if let Some(after_opening) = normalized.strip_prefix("---\r\n")
            && let Some(closing_pos) = after_opening.find("\r\n---\r\n")
        {
            return after_opening[closing_pos + "\r\n---\r\n".len()..]
                .trim_start_matches(['\r', '\n']);
        }
        raw
    }

    /// Expands template variables and resolves the `{{cursor}}` position in Unicode characters.
    ///
    /// Variables supported:
    /// - `{{date}}` -> YYYY-MM-DD
    /// - `{{time}}` -> HH:MM
    /// - `{{datetime}}` -> YYYY-MM-DD HH:MM
    /// - `{{title}}` -> note title
    /// - `{{yesterday}}` -> YYYY-MM-DD
    /// - `{{tomorrow}}` -> YYYY-MM-DD
    /// - `{{cursor}}` -> target cursor position (Unicode char index). All occurrences are removed.
    pub fn expand(
        template_text: &str,
        title: &str,
        now: DateTime<Local>,
    ) -> (String, Option<usize>) {
        let text_without_frontmatter = Self::strip_frontmatter(template_text);
        let date_str = now.format("%Y-%m-%d").to_string();
        let time_str = now.format("%H:%M").to_string();
        let datetime_str = now.format("%Y-%m-%d %H:%M").to_string();
        let yesterday_str = (now.date_naive() - Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let tomorrow_str = (now.date_naive() + Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        let mut expanded = text_without_frontmatter
            .replace("{{date}}", &date_str)
            .replace("{{time}}", &time_str)
            .replace("{{datetime}}", &datetime_str)
            .replace("{{title}}", title)
            .replace("{{yesterday}}", &yesterday_str)
            .replace("{{tomorrow}}", &tomorrow_str);

        // Find cursor positions and remove all {{cursor}} markers
        let cursor_marker = "{{cursor}}";
        let mut first_cursor_char_idx = None;

        while let Some(byte_idx) = expanded.find(cursor_marker) {
            // Count characters up to this byte index for exact Unicode positioning
            let char_idx = expanded[..byte_idx].chars().count();
            if first_cursor_char_idx.is_none() {
                first_cursor_char_idx = Some(char_idx);
            }
            expanded.replace_range(byte_idx..byte_idx + cursor_marker.len(), "");
        }

        (expanded, first_cursor_char_idx)
    }

    /// Lists all markdown templates found within `templates_folder` in the vault.
    pub fn list_templates(notes_dir: &Path, templates_folder: &Path) -> Vec<TemplateEntry> {
        let full_dir = if templates_folder.as_os_str().is_empty() {
            notes_dir.join("Templates")
        } else {
            notes_dir.join(templates_folder)
        };

        let mut results = Vec::new();
        if !full_dir.is_dir() {
            return results;
        }

        let mut stack = vec![full_dir.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(read_dir) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.is_file()
                    && path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                    && let Ok(rel) = path.strip_prefix(&full_dir)
                {
                    let name = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    results.push(TemplateEntry {
                        name,
                        relative_path: rel.to_path_buf(),
                    });
                }
            }
        }
        results.sort_by(|a, b| a.name.cmp(&b.name));
        results
    }

    /// Loads a template's content by name or relative path, stripping any note frontmatter.
    pub fn load_template(
        notes_dir: &Path,
        templates_folder: &Path,
        template_name: &str,
    ) -> Option<String> {
        let full_dir = if templates_folder.as_os_str().is_empty() {
            notes_dir.join("Templates")
        } else {
            notes_dir.join(templates_folder)
        };

        let clean_name = template_name.trim().trim_end_matches(".md");
        if clean_name.is_empty() {
            return None;
        }

        // Check direct filename with .md
        let direct_file = full_dir.join(format!("{clean_name}.md"));
        if direct_file.is_file() {
            return fs::read_to_string(direct_file)
                .ok()
                .map(|content| Self::strip_frontmatter(&content).to_owned());
        }

        // Try searching in subdirectories
        for entry in Self::list_templates(notes_dir, templates_folder) {
            if entry.name.eq_ignore_ascii_case(clean_name) {
                let path = full_dir.join(&entry.relative_path);
                return fs::read_to_string(path)
                    .ok()
                    .map(|content| Self::strip_frontmatter(&content).to_owned());
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn expands_variables_and_calculates_unicode_cursor() {
        let fixed_time = Local.with_ymd_and_hms(2026, 8, 18, 14, 30, 0).unwrap();
        let template =
            "# {{title}} ({{date}})\n\nПривет, мир! 🦀\n{{cursor}}\n\n## Notes\n{{cursor}}";

        let (expanded, cursor_pos) = TemplateEngine::expand(template, "Daily Plan", fixed_time);

        assert!(
            expanded.starts_with("# Daily Plan (2026-08-18)\n\nПривет, мир! 🦀\n\n\n## Notes\n")
        );
        assert!(!expanded.contains("{{cursor}}"));

        let expected_prefix = "# Daily Plan (2026-08-18)\n\nПривет, мир! 🦀\n";
        let expected_char_idx = expected_prefix.chars().count();
        assert_eq!(cursor_pos, Some(expected_char_idx));
    }

    #[test]
    fn strips_yaml_frontmatter_when_expanding() {
        let fixed_time = Local.with_ymd_and_hms(2026, 8, 18, 10, 0, 0).unwrap();
        let raw_note = "---\nid: 1c719075-6cfb-463f-bb31-fb2669bf00bb\ntitle: Plans\ncreated_at: 2026-08-18T08:27:26.798095700+03:00\nupdated_at: 2026-08-18T08:27:58.664287100+03:00\naliases:\n- Plans\n---\n\n# {{title}} Plan\n- [ ] Task for {{date}}";

        let (expanded, cursor) = TemplateEngine::expand(raw_note, "My", fixed_time);

        assert_eq!(expanded, "# My Plan\n- [ ] Task for 2026-08-18");
        assert_eq!(cursor, None);
        assert!(!expanded.contains("1c719075"));
        assert!(!expanded.contains("aliases"));
    }

    #[test]
    fn template_without_cursor_returns_none() {
        let fixed_time = Local.with_ymd_and_hms(2026, 8, 18, 10, 0, 0).unwrap();
        let template = "# {{title}}\nNo cursor here.";

        let (expanded, cursor_pos) = TemplateEngine::expand(template, "Simple", fixed_time);

        assert_eq!(expanded, "# Simple\nNo cursor here.");
        assert_eq!(cursor_pos, None);
    }

    #[test]
    fn handles_yesterday_and_tomorrow_variables() {
        let fixed_time = Local.with_ymd_and_hms(2026, 8, 18, 10, 0, 0).unwrap();
        let template = "Yesterday: {{yesterday}}, Today: {{date}}, Tomorrow: {{tomorrow}}";

        let (expanded, _) = TemplateEngine::expand(template, "T", fixed_time);

        assert_eq!(
            expanded,
            "Yesterday: 2026-08-17, Today: 2026-08-18, Tomorrow: 2026-08-19"
        );
    }
}
