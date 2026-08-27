//! Local date service and daily notes management.

use chrono::{DateTime, Duration, Local, NaiveDate};
use std::path::PathBuf;

/// Service for local date resolution and date arithmetic.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalDateService;

impl LocalDateService {
    /// Returns the current local date.
    pub fn today() -> NaiveDate {
        Local::now().date_naive()
    }

    /// Returns yesterday's local date.
    #[allow(dead_code)]
    pub fn yesterday() -> NaiveDate {
        Self::today() - Duration::days(1)
    }

    /// Returns tomorrow's local date.
    #[allow(dead_code)]
    pub fn tomorrow() -> NaiveDate {
        Self::today() + Duration::days(1)
    }

    /// Returns current local timestamp.
    pub fn now() -> DateTime<Local> {
        Local::now()
    }

    /// Returns the day before `date`.
    pub fn prev_day(date: NaiveDate) -> NaiveDate {
        date - Duration::days(1)
    }

    /// Returns the day after `date`.
    pub fn next_day(date: NaiveDate) -> NaiveDate {
        date + Duration::days(1)
    }

    /// Checks whether `date` is today.
    pub fn is_today(date: NaiveDate) -> bool {
        date == Self::today()
    }

    /// Returns a human-friendly display label (e.g. "Tuesday, 18 Aug 2026").
    pub fn format_daily_display(date: NaiveDate) -> String {
        date.format("%A, %d %b %Y").to_string()
    }

    /// Attempts to extract a `NaiveDate` from a note's title or path.
    pub fn parse_date_from_note(title: &str, relative_path: &std::path::Path) -> Option<NaiveDate> {
        let trimmed_title = title.trim();
        // 1. Try standard ISO %Y-%m-%d
        if let Ok(d) = NaiveDate::parse_from_str(trimmed_title, "%Y-%m-%d") {
            return Some(d);
        }
        // 2. Try other common formats
        if let Ok(d) = NaiveDate::parse_from_str(trimmed_title, "%d-%m-%Y") {
            return Some(d);
        }
        if let Ok(d) = NaiveDate::parse_from_str(trimmed_title, "%Y.%m.%d") {
            return Some(d);
        }

        // 3. Try reconstructing from relative path components (e.g., Daily/2026/08/18.md or 2026-08/18.md)
        let normalized = relative_path.to_string_lossy().replace('\\', "/");
        let path_without_ext = normalized.trim_end_matches(".md");
        let parts: Vec<&str> = path_without_ext.split('/').collect();

        if parts.len() >= 3 {
            let last = parts[parts.len() - 1];
            let prev = parts[parts.len() - 2];
            let prev2 = parts[parts.len() - 3];
            let combined = format!("{prev2}-{prev}-{last}");
            if let Ok(d) = NaiveDate::parse_from_str(&combined, "%Y-%m-%d") {
                return Some(d);
            }
        }
        if parts.len() >= 2 {
            let last = parts[parts.len() - 1];
            let prev = parts[parts.len() - 2];
            let combined = format!("{prev}-{last}");
            if let Ok(d) = NaiveDate::parse_from_str(&combined, "%Y-%m-%d") {
                return Some(d);
            }
        }

        None
    }

    /// Formats a daily note title/path according to `format_str`.
    /// Supports nested subdirectories like `"%Y/%m/%d"` or `"%Y-%m/%d"`.
    pub fn format_daily_path(
        format_str: &str,
        date: NaiveDate,
    ) -> Result<(PathBuf, String), String> {
        let trimmed_format = if format_str.trim().is_empty() {
            "%Y-%m-%d"
        } else {
            format_str.trim()
        };

        // Format date string
        let formatted = date.format(trimmed_format).to_string();
        let normalized = formatted.replace('\\', "/");
        let raw_components: Vec<&str> = normalized
            .split('/')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();

        if raw_components.is_empty() {
            return Err("Formatted daily note path is empty".to_owned());
        }

        let mut subfolder = PathBuf::new();
        for &comp in &raw_components[..raw_components.len() - 1] {
            validate_path_segment(comp)?;
            subfolder.push(comp);
        }

        let note_title = raw_components.last().unwrap().to_string();
        validate_path_segment(&note_title)?;

        Ok((subfolder, note_title))
    }
}

/// Validates that a single path component is safe for Windows and Linux filesystems.
pub fn validate_path_segment(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return Err("Path component cannot be empty or relative traversal".to_owned());
    }
    if trimmed.ends_with(['.', ' '])
        || trimmed.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        })
    {
        return Err(format!(
            "Path component '{name}' contains illegal characters (< > : \" / \\ | ? * or control characters)"
        ));
    }

    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|reserved_name| trimmed.eq_ignore_ascii_case(reserved_name))
    {
        return Err(format!(
            "Path component '{name}' is a reserved Windows device name"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn local_date_service_provides_consecutive_days() {
        let today = LocalDateService::today();
        let yesterday = LocalDateService::yesterday();
        let tomorrow = LocalDateService::tomorrow();

        assert_eq!(yesterday + Duration::days(1), today);
        assert_eq!(today + Duration::days(1), tomorrow);
    }

    #[test]
    fn formats_flat_daily_note_path() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 18).unwrap();
        let (subfolder, title) = LocalDateService::format_daily_path("%Y-%m-%d", date).unwrap();
        assert_eq!(subfolder, PathBuf::new());
        assert_eq!(title, "2026-08-18");
    }

    #[test]
    fn formats_nested_daily_note_subdirectories() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 18).unwrap();
        let (subfolder, title) = LocalDateService::format_daily_path("%Y/%m/%d", date).unwrap();
        assert_eq!(subfolder, PathBuf::from("2026/08"));
        assert_eq!(title, "18");

        let (subfolder2, title2) = LocalDateService::format_daily_path("%Y-%m/%d", date).unwrap();
        assert_eq!(subfolder2, PathBuf::from("2026-08"));
        assert_eq!(title2, "18");
    }

    #[test]
    fn rejects_illegal_characters_and_reserved_names() {
        assert!(validate_path_segment("2026:08:18").is_err());
        assert!(validate_path_segment("2026*08").is_err());
        assert!(validate_path_segment("CON").is_err());
        assert!(validate_path_segment("..").is_err());
        assert!(validate_path_segment("valid_name-2026").is_ok());
    }

    #[test]
    fn relative_day_arithmetic_and_display() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 18).unwrap();
        let prev = LocalDateService::prev_day(date);
        let next = LocalDateService::next_day(date);

        assert_eq!(prev, NaiveDate::from_ymd_opt(2026, 8, 17).unwrap());
        assert_eq!(next, NaiveDate::from_ymd_opt(2026, 8, 19).unwrap());
        assert_eq!(
            LocalDateService::format_daily_display(date),
            "Tuesday, 18 Aug 2026"
        );
    }

    #[test]
    fn parses_date_from_title_and_nested_paths() {
        let date =
            LocalDateService::parse_date_from_note("2026-08-18", Path::new("Daily/2026-08-18.md"))
                .unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 8, 18).unwrap());

        let date_nested =
            LocalDateService::parse_date_from_note("18", Path::new("Daily/2026/08/18.md")).unwrap();
        assert_eq!(date_nested, NaiveDate::from_ymd_opt(2026, 8, 18).unwrap());

        let invalid = LocalDateService::parse_date_from_note("Projects", Path::new("Projects.md"));
        assert!(invalid.is_none());
    }
}
