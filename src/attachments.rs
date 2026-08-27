//! Attachments management, drag-and-drop import, clipboard image saving, and orphan cleanup.

use crate::storage::Note;
use chrono::Local;
use std::collections::HashSet;
use std::fs;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

pub struct AttachmentManager;

impl AttachmentManager {
    /// Creates a relative attachments folder and verifies that it remains inside the vault.
    pub fn ensure_attachments_dir(notes_dir: &Path, folder_name: &Path) -> Result<PathBuf, String> {
        validate_attachments_folder(folder_name)?;

        let notes_root = notes_dir
            .canonicalize()
            .map_err(|error| format!("Failed to resolve notes folder: {error}"))?;
        let path = notes_dir.join(folder_name);
        fs::create_dir_all(&path)
            .map_err(|error| format!("Failed to create attachments folder: {error}"))?;
        let resolved = path
            .canonicalize()
            .map_err(|error| format!("Failed to resolve attachments folder: {error}"))?;

        if !resolved.starts_with(&notes_root) || resolved == notes_root {
            return Err("Attachments folder must stay inside the Notes folder".to_owned());
        }

        Ok(resolved)
    }

    /// Imports an external file into the vault's attachments folder and returns the relative path string.
    pub fn import_file(
        source_path: &Path,
        notes_dir: &Path,
        folder_name: &Path,
    ) -> Result<String, String> {
        let attachments_dir = Self::ensure_attachments_dir(notes_dir, folder_name)?;

        let file_stem = source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("attachment");
        let extension = source_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin");

        let sanitized_stem = sanitize_attachment_stem(file_stem);
        let target_filename = format!("{sanitized_stem}.{extension}");
        let target_path = unique_attachment_path(
            &attachments_dir,
            &target_filename,
            &sanitized_stem,
            extension,
        );

        fs::copy(source_path, &target_path)
            .map_err(|e| format!("Failed to copy attachment file: {e}"))?;

        let filename = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| "Invalid filename".to_string())?;

        let folder_str = folder_name.to_string_lossy().replace('\\', "/");
        let folder_clean = folder_str.trim_matches('/');
        if folder_clean.is_empty() {
            Ok(filename.to_string())
        } else {
            Ok(format!("{folder_clean}/{filename}"))
        }
    }

    /// Saves RGBA image bytes from clipboard as a PNG file inside the attachments folder.
    pub fn save_clipboard_image(
        image_data: &arboard::ImageData,
        notes_dir: &Path,
        folder_name: &Path,
    ) -> Result<String, String> {
        let attachments_dir = Self::ensure_attachments_dir(notes_dir, folder_name)?;

        let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
        let preferred_filename = format!("Pasted Image {timestamp}.png");
        let target_path = unique_attachment_path(
            &attachments_dir,
            &preferred_filename,
            &format!("Pasted Image {timestamp}"),
            "png",
        );
        let filename = target_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "Invalid attachment filename".to_owned())?
            .to_owned();

        let width = image_data.width as u32;
        let height = image_data.height as u32;

        let img_buffer: image::RgbaImage =
            image::ImageBuffer::from_raw(width, height, image_data.bytes.clone().into_owned())
                .ok_or_else(|| "Failed to decode clipboard image buffer".to_string())?;

        let mut png_bytes = Vec::new();
        img_buffer
            .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            .map_err(|e| format!("Failed to encode image to PNG: {e}"))?;

        fs::write(&target_path, png_bytes)
            .map_err(|e| format!("Failed to write pasted image file: {e}"))?;

        let folder_str = folder_name.to_string_lossy().replace('\\', "/");
        let folder_clean = folder_str.trim_matches('/');
        if folder_clean.is_empty() {
            Ok(filename)
        } else {
            Ok(format!("{folder_clean}/{filename}"))
        }
    }

    /// Checks system clipboard for an image (raw pixel buffer or image file path) and saves it to the attachments folder.
    pub fn try_save_clipboard_image(
        notes_dir: &Path,
        folder_name: &Path,
    ) -> Result<Option<String>, String> {
        let mut clipboard = open_clipboard_with_retry()?;

        if let Ok(image_data) = clipboard.get_image() {
            let relative = Self::save_clipboard_image(&image_data, notes_dir, folder_name)?;
            return Ok(Some(relative));
        }

        if let Ok(text) = clipboard.get_text() {
            let trimmed = text.trim().trim_matches('"');
            let path = Path::new(trimmed);
            if path.is_file()
                && let Some(extension) = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_lowercase)
                && matches!(
                    extension.as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg"
                )
            {
                return Self::import_file(path, notes_dir, folder_name).map(Some);
            }
        }

        Ok(None)
    }

    /// Extracts all attachment paths referenced in Markdown notes across the vault.
    pub fn extract_referenced_attachments(notes: &[Note]) -> HashSet<String> {
        let mut references = HashSet::new();

        for note in notes {
            for ref_path in extract_attachments_from_markdown(&note.content) {
                references.insert(ref_path);
            }
        }

        references
    }

    /// Scans the attachments directory and returns a list of files not referenced in any note.
    pub fn find_orphaned_attachments(
        notes: &[Note],
        notes_dir: &Path,
        folder_name: &Path,
    ) -> Result<Vec<PathBuf>, String> {
        validate_attachments_folder(folder_name)?;
        let attachments_dir = notes_dir.join(folder_name);
        if !attachments_dir.is_dir() {
            return Ok(Vec::new());
        }

        let notes_root = notes_dir
            .canonicalize()
            .map_err(|error| format!("Failed to resolve notes folder: {error}"))?;
        let attachments_dir = attachments_dir
            .canonicalize()
            .map_err(|error| format!("Failed to resolve attachments folder: {error}"))?;
        if !attachments_dir.starts_with(&notes_root) || attachments_dir == notes_root {
            return Err("Attachments folder must stay inside the Notes folder".to_owned());
        }

        let referenced = Self::extract_referenced_attachments(notes);
        let mut orphans = Vec::new();

        if let Ok(entries) = fs::read_dir(&attachments_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    // Check both bare file name and relative path "Attachments/file.png"
                    let folder_prefix = folder_name.to_string_lossy().replace('\\', "/");
                    let rel_ref = format!("{folder_prefix}/{file_name}");

                    let is_referenced = referenced.iter().any(|r| {
                        let decoded = decode_url_path(r);
                        r.eq_ignore_ascii_case(&file_name)
                            || r.eq_ignore_ascii_case(&rel_ref)
                            || r.ends_with(&format!("/{file_name}"))
                            || r.ends_with(&file_name)
                            || decoded.eq_ignore_ascii_case(&file_name)
                            || decoded.eq_ignore_ascii_case(&rel_ref)
                            || decoded.ends_with(&format!("/{file_name}"))
                            || decoded.ends_with(&file_name)
                    });

                    if !is_referenced {
                        orphans.push(path);
                    }
                }
            }
        }

        orphans.sort();
        Ok(orphans)
    }
}

fn open_clipboard_with_retry() -> Result<arboard::Clipboard, String> {
    let mut last_error = None;
    for attempt in 0..5 {
        match arboard::Clipboard::new() {
            Ok(clipboard) => return Ok(clipboard),
            Err(error) => last_error = Some(error),
        }

        if attempt < 4 {
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
    }

    Err(format!(
        "Could not open system clipboard: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "unknown clipboard error".to_owned())
    ))
}

fn validate_attachments_folder(folder_name: &Path) -> Result<(), String> {
    if folder_name.as_os_str().is_empty()
        || folder_name.is_absolute()
        || !folder_name
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("Attachments folder must be a non-empty relative path".to_owned());
    }

    Ok(())
}

fn unique_attachment_path(
    directory: &Path,
    preferred_filename: &str,
    stem: &str,
    extension: &str,
) -> PathBuf {
    let preferred = directory.join(preferred_filename);
    if !preferred.exists() {
        return preferred;
    }

    let timestamp = Local::now().format("%Y%m%d%H%M%S");
    for counter in 1_u32.. {
        let candidate = directory.join(format!("{stem}-{timestamp}-{counter}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("attachment suffix counter is unbounded")
}

/// Decodes common URL escapes (such as %20 for spaces).
pub fn decode_url_path(path: &str) -> String {
    path.replace("%20", " ")
        .replace("%28", "(")
        .replace("%29", ")")
        .replace("%2B", "+")
        .replace("%2b", "+")
}

/// Sanitizes a file stem for safe cross-platform saving.
pub fn sanitize_attachment_stem(stem: &str) -> String {
    let sanitized: String = stem
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            other if other.is_control() => '_',
            other => other,
        })
        .collect();

    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        "attachment".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Parses Markdown content for images and file embeds:
/// `![alt](path)` and `![[filename.ext]]`
pub fn extract_attachments_from_markdown(source: &str) -> Vec<String> {
    let mut results = Vec::new();

    // 1. Standard markdown images: ![alt](path)
    let mut search_from = 0;
    while let Some(rel_start) = source[search_from..].find("![") {
        let start = search_from + rel_start;
        let Some(rel_mid) = source[start..].find("](") else {
            break;
        };
        let mid = start + rel_mid;
        let Some(rel_end) = source[mid + 2..].find(')') else {
            break;
        };
        let end = mid + 2 + rel_end;

        let link_target = source[mid + 2..end].trim();
        if !link_target.is_empty()
            && !link_target.starts_with("http://")
            && !link_target.starts_with("https://")
        {
            let clean_path = link_target.replace('\\', "/");
            let decoded = decode_url_path(&clean_path);
            if decoded != clean_path {
                results.push(decoded);
            }
            results.push(clean_path);
        }

        search_from = end + 1;
    }

    // 2. Obsidian-style embeds: ![[filename.ext]] or ![[filename.ext|100]]
    let mut search_from = 0;
    while let Some(rel_start) = source[search_from..].find("![[") {
        let start = search_from + rel_start;
        let content_start = start + 3;
        let Some(rel_end) = source[content_start..].find("]]") else {
            break;
        };
        let content_end = content_start + rel_end;
        let inner = source[content_start..content_end].trim();
        let target = inner.split('|').next().unwrap_or(inner).trim();
        if !target.is_empty() {
            let clean_path = target.replace('\\', "/");
            let decoded = decode_url_path(&clean_path);
            if decoded != clean_path {
                results.push(decoded);
            }
            results.push(clean_path);
        }

        search_from = content_end + 2;
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn extracts_markdown_images_and_embeds() {
        let content = r#"
Here is an image: ![Diagram](Attachments/diagram.png)
And an embed: ![[screenshot.jpg|300]]
And external: ![Web](https://example.com/logo.png)
"#;
        let attachments = extract_attachments_from_markdown(content);
        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0], "Attachments/diagram.png");
        assert_eq!(attachments[1], "screenshot.jpg");
    }

    #[test]
    fn imports_file_into_attachments_folder() {
        let dir = tempdir().unwrap();
        let notes_dir = dir.path();
        let folder_name = Path::new("Attachments");

        let source_file = notes_dir.join("temp_source.png");
        fs::write(&source_file, b"fake png data").unwrap();

        let rel_link =
            AttachmentManager::import_file(&source_file, notes_dir, folder_name).unwrap();
        assert_eq!(rel_link, "Attachments/temp_source.png");

        let target_file = notes_dir.join("Attachments/temp_source.png");
        assert!(target_file.exists());
    }

    #[test]
    fn finds_orphaned_attachments() {
        let dir = tempdir().unwrap();
        let notes_dir = dir.path();
        let folder_name = Path::new("Attachments");

        let attachments_dir = notes_dir.join("Attachments");
        fs::create_dir_all(&attachments_dir).unwrap();

        fs::write(attachments_dir.join("used.png"), b"123").unwrap();
        fs::write(attachments_dir.join("orphan.png"), b"456").unwrap();

        let mut note = Note::new(notes_dir);
        note.content = "Look at this: ![Image](Attachments/used.png)".to_owned();

        let orphans =
            AttachmentManager::find_orphaned_attachments(&[note], notes_dir, folder_name).unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].file_name().unwrap(), "orphan.png");
    }

    #[test]
    fn handles_url_encoded_spaces_in_attachments() {
        let dir = tempdir().unwrap();
        let notes_dir = dir.path();
        let folder_name = Path::new("Attachments");

        let attachments_dir = notes_dir.join("Attachments");
        fs::create_dir_all(&attachments_dir).unwrap();

        fs::write(attachments_dir.join("Pasted Image 2026.png"), b"123").unwrap();

        let mut note = Note::new(notes_dir);
        note.content = "Look at this: ![Image](Attachments/Pasted%20Image%202026.png)".to_owned();

        let orphans =
            AttachmentManager::find_orphaned_attachments(&[note], notes_dir, folder_name).unwrap();
        assert!(orphans.is_empty());
    }

    #[test]
    fn rejects_attachment_folders_outside_notes() {
        let dir = tempdir().unwrap();
        let error = AttachmentManager::ensure_attachments_dir(dir.path(), Path::new("../outside"))
            .unwrap_err();
        assert!(error.contains("relative path"));
    }

    #[test]
    fn repeated_imports_never_overwrite_existing_files() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("image.png");
        fs::write(&source, b"first").unwrap();

        let first =
            AttachmentManager::import_file(&source, dir.path(), Path::new("Attachments")).unwrap();
        fs::write(&source, b"second").unwrap();
        let second =
            AttachmentManager::import_file(&source, dir.path(), Path::new("Attachments")).unwrap();

        assert_ne!(first, second);
        assert_eq!(fs::read(dir.path().join(first)).unwrap(), b"first");
        assert_eq!(fs::read(dir.path().join(second)).unwrap(), b"second");
    }

    #[test]
    fn saves_clipboard_image_data_to_png() {
        let dir = tempdir().unwrap();
        let notes_dir = dir.path();
        let folder_name = Path::new("Attachments");

        let raw_pixels = vec![255u8; 10 * 10 * 4];
        let image_data = arboard::ImageData {
            width: 10,
            height: 10,
            bytes: std::borrow::Cow::Owned(raw_pixels),
        };

        let rel =
            AttachmentManager::save_clipboard_image(&image_data, notes_dir, folder_name).unwrap();
        assert!(rel.starts_with("Attachments/Pasted Image "));
        assert!(notes_dir.join(&rel).exists());
    }
}
