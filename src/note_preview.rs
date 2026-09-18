//! Finds local image blocks for display at their Markdown positions in the live editor.
use crate::{attachments, storage::Note};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct BlockImage {
    pub line_index: usize,
    pub char_index: usize,
    pub uri: String,
    pub alt: String,
}

struct NoteImages {
    updated: chrono::DateTime<chrono::Local>,
    path: PathBuf,
    content_len: usize,
    images: Vec<BlockImage>,
}

#[derive(Default)]
pub struct PreviewCache {
    notes: HashMap<Uuid, NoteImages>,
}

pub fn file_uri(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.starts_with("//") {
        format!("file:{value}")
    } else if value.starts_with('/') {
        format!("file://{value}")
    } else {
        format!("file:///{value}")
    }
}

impl PreviewCache {
    pub fn clear(&mut self) {
        self.notes.clear();
    }

    pub fn blocks(&mut self, note: &Note, root: &Path) -> &[BlockImage] {
        let stale = self.notes.get(&note.id).is_none_or(|entry| {
            entry.updated != note.updated_at
                || entry.path != note.file_path
                || entry.content_len != note.content.len()
        });
        if stale {
            self.notes.insert(
                note.id,
                NoteImages {
                    updated: note.updated_at,
                    path: note.file_path.clone(),
                    content_len: note.content.len(),
                    images: find_block_images(note, root),
                },
            );
        }
        &self.notes[&note.id].images
    }
}

fn find_block_images(note: &Note, root: &Path) -> Vec<BlockImage> {
    let Some(canonical_root) = root.canonicalize().ok() else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut char_index = 0;
    let mut in_code_block = false;
    for (line_index, raw_line) in note.content.split_inclusive('\n').enumerate() {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
        } else if !in_code_block
            && line.len() - line.trim_start().len() <= 3
            && let Some((alt, reference)) = parse_image_line(trimmed)
            && let Some(path) =
                resolve_local_image(root, &canonical_root, &note.file_path, reference)
        {
            let alt = if alt.is_empty() {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                alt.to_owned()
            };
            result.push(BlockImage {
                line_index,
                char_index,
                uri: file_uri(&path),
                alt,
            });
        }
        char_index += raw_line.chars().count();
    }
    result
}

fn parse_image_line(line: &str) -> Option<(&str, &str)> {
    if let Some(inner) = line.strip_prefix("![[") {
        let inner = inner.strip_suffix("]]")?;
        let reference = inner.split('|').next()?.trim();
        return (!reference.is_empty()).then_some((reference, reference));
    }
    let inner = line.strip_prefix("![")?;
    let (alt, rest) = inner.split_once("](")?;
    let reference = rest.strip_suffix(')')?.trim();
    (!reference.is_empty()).then_some((alt, reference))
}

fn resolve_local_image(
    root: &Path,
    canonical_root: &Path,
    note_path: &Path,
    reference: &str,
) -> Option<PathBuf> {
    let decoded = attachments::decode_url_path(&reference.replace('\\', "/"));
    if decoded.contains("://") || Path::new(&decoded).is_absolute() {
        return None;
    }
    let relative = Path::new(&decoded);
    [root.join(relative), note_path.parent()?.join(relative)]
        .into_iter()
        .find(|candidate| {
            candidate
                .canonicalize()
                .is_ok_and(|path| path.starts_with(canonical_root) && path.is_file())
                && candidate
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        matches!(
                            extension.to_ascii_lowercase().as_str(),
                            "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp"
                        )
                    })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_keep_unicode_and_spaces_and_support_both_platforms() {
        assert_eq!(
            file_uri(Path::new("C:/Notes/Мой файл.png")),
            "file:///C:/Notes/Мой файл.png"
        );
        assert_eq!(
            file_uri(Path::new("/home/me/Мой файл.png")),
            "file:///home/me/Мой файл.png"
        );
    }

    #[test]
    fn finds_images_in_document_order_but_not_in_code_or_outside_the_vault() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let attachments = root.join("Attachments");
        std::fs::create_dir_all(&attachments).unwrap();
        std::fs::write(attachments.join("one.png"), b"image").unwrap();
        std::fs::write(attachments.join("two.png"), b"image").unwrap();
        let mut note = Note::new(root);
        note.content = "Before\n![One](Attachments/one.png)\nAfter\n```\n![No](Attachments/two.png)\n```\n![[Attachments/two.png]]\n![Outside](../outside.png)".to_owned();
        let images = find_block_images(&note, root);
        assert_eq!(images.len(), 2);
        assert_eq!((images[0].line_index, images[0].char_index), (1, 7));
        assert_eq!(images[0].alt, "One");
        assert_eq!(images[1].line_index, 6);
    }
}
