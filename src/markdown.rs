//! Cursor-aware Markdown layout for the live editor.

use eframe::egui::{
    Color32, Context, FontFamily, FontId, Id, Stroke, TextEdit, TextFormat, Ui, Visuals,
    text::{CCursor, CCursorRange, LayoutJob},
    text_edit::TextEditOutput,
    text_edit::TextEditState,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

const HIDDEN_MARKER_SIZE: f32 = 0.1;
const MAX_HIGHLIGHT_BYTES: usize = 512 * 1024;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownCommand {
    Bold,
    Italic,
    InlineCode,
    CodeBlock,
    WikiLink,
    Heading,
    Bullet,
    Task,
    Indent,
    Outdent,
}

#[derive(Clone)]
struct CachedLayout {
    source_hash: u64,
    active_line: Option<usize>,
    visual_key: u64,
    body_size_bits: u32,
    job: LayoutJob,
}

/// Shows one live Markdown editor. `id` owns its cursor and undo state.
pub fn show_editor(ui: &mut Ui, text: &mut String, id: Id, body_size: f32) -> TextEditOutput {
    let editor_width = ui.available_width();
    let active_line = ui
        .memory(|memory| memory.has_focus(id))
        .then(|| TextEditState::load(ui.ctx(), id))
        .flatten()
        .and_then(|state| state.cursor.char_range())
        .map(|range| line_at_character(text, range.primary.index.into()));

    let cache_id = id.with("markdown_layout");
    let cached = ui
        .ctx()
        .data(|data| data.get_temp::<Arc<CachedLayout>>(cache_id));
    let mut rendered_cache = None;
    let mut layouter = |ui: &Ui, buffer: &dyn eframe::egui::TextBuffer, wrap_width: f32| {
        let source = buffer.as_str();
        let source_hash = text_hash(source);
        let visual_key = visual_hash(ui.visuals());
        let mut layout_job = cached
            .as_ref()
            .filter(|cached| {
                cached.source_hash == source_hash
                    && cached.active_line == active_line
                    && cached.visual_key == visual_key
                    && cached.body_size_bits == body_size.to_bits()
            })
            .map_or_else(
                || highlight(source, ui.visuals(), active_line, body_size),
                |cached| cached.job.clone(),
            );
        layout_job.wrap.max_width = wrap_width;
        rendered_cache = Some(Arc::new(CachedLayout {
            source_hash,
            active_line,
            visual_key,
            body_size_bits: body_size.to_bits(),
            job: layout_job.clone(),
        }));
        ui.fonts_mut(|fonts| fonts.layout_job(layout_job))
    };

    let output = TextEdit::multiline(text)
        .id(id)
        .frame(eframe::egui::Frame::NONE)
        .desired_width(editor_width)
        .desired_rows(20)
        .hint_text("Enter Markdown here...")
        .layouter(&mut layouter)
        .show(ui);
    if let Some(cache) = rendered_cache {
        ui.ctx().data_mut(|data| data.insert_temp(cache_id, cache));
    }
    output
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineItem {
    pub level: usize,
    pub title: String,
    pub char_index: usize,
    pub line_index: usize,
}

/// Extracts markdown outline headers (#, ##, ###) while strictly ignoring code blocks.
pub fn extract_outline(text: &str) -> Vec<OutlineItem> {
    let mut items = Vec::new();
    let mut inside_code_block = false;
    let mut char_acc = 0;

    for (line_index, raw_line) in text.split_inclusive('\n').enumerate() {
        let line = raw_line
            .strip_suffix('\n')
            .unwrap_or(raw_line)
            .trim_end_matches('\r');
        let trimmed = line.trim();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            inside_code_block = !inside_code_block;
            char_acc += raw_line.chars().count();
            continue;
        }

        if !inside_code_block {
            let hash_count = trimmed.bytes().take_while(|&b| b == b'#').count();
            if (1..=6).contains(&hash_count) {
                let rest = &trimmed[hash_count..];
                if rest.starts_with(' ') || rest.is_empty() {
                    let title = rest.trim().to_owned();
                    if !title.is_empty() {
                        items.push(OutlineItem {
                            level: hash_count,
                            title,
                            char_index: char_acc,
                            line_index,
                        });
                    }
                }
            }
        }

        char_acc += raw_line.chars().count();
    }

    items
}

/// Counts total words and characters in text.
pub fn count_words_and_chars(text: &str) -> (usize, usize) {
    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    (words, chars)
}

pub fn set_cursor_char_index(ctx: &Context, id: Id, char_index: usize) {
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(char_index))));
    state.store(ctx, id);
    ctx.memory_mut(|memory| memory.request_focus(id));
}

pub fn apply_command(ctx: &Context, id: Id, text: &mut String, command: MarkdownCommand) -> bool {
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    let range = state.cursor.char_range().unwrap_or_else(|| {
        let end = CCursor::new(text.chars().count());
        CCursorRange::one(end)
    });
    let selected = range.as_sorted_char_range();
    let selected = usize::from(selected.start)..usize::from(selected.end);
    let new_selection = match command {
        MarkdownCommand::Bold => wrap_selection(text, selected, "**", "**", "bold text"),
        MarkdownCommand::Italic => wrap_selection(text, selected, "*", "*", "italic text"),
        MarkdownCommand::InlineCode => wrap_selection(text, selected, "`", "`", "code"),
        MarkdownCommand::CodeBlock => wrap_selection(text, selected, "```\n", "\n```", "code"),
        MarkdownCommand::WikiLink => wrap_selection(text, selected, "[[", "]]", "Note"),
        MarkdownCommand::Heading => edit_selected_lines(text, selected, LineEdit::Toggle("# ")),
        MarkdownCommand::Bullet => edit_selected_lines(text, selected, LineEdit::Toggle("- ")),
        MarkdownCommand::Task => edit_selected_lines(text, selected, LineEdit::Toggle("- [ ] ")),
        MarkdownCommand::Indent => edit_selected_lines(text, selected, LineEdit::Indent),
        MarkdownCommand::Outdent => edit_selected_lines(text, selected, LineEdit::Outdent),
    };
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(new_selection.start),
        CCursor::new(new_selection.end),
    )));
    state.store(ctx, id);
    ctx.memory_mut(|memory| memory.request_focus(id));
    true
}

pub fn continue_list_at_cursor(ctx: &Context, id: Id, text: &mut String) -> bool {
    let Some(mut state) = TextEditState::load(ctx, id) else {
        return false;
    };
    let Some(range) = state.cursor.char_range() else {
        return false;
    };
    let Some(cursor) = range.single() else {
        return false;
    };
    let cursor_index = usize::from(cursor.index);
    let cursor_byte = char_to_byte(text, cursor_index);
    let line_start = text[..cursor_byte].rfind('\n').map_or(0, |index| index + 1);
    let line = &text[line_start..cursor_byte];
    let indent_bytes = line.len() - line.trim_start().len();
    let indent = &line[..indent_bytes];
    let trimmed = &line[indent_bytes..];
    let Some(marker) = continuation_marker(trimmed) else {
        return false;
    };

    if trimmed.trim_end() == marker.trim_end() {
        text.replace_range(line_start..cursor_byte, "\n");
        let next = text[..line_start + 1].chars().count();
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(next))));
    } else {
        let insertion = format!("\n{indent}{}", next_marker(&marker));
        text.insert_str(cursor_byte, &insertion);
        let next = cursor_index + insertion.chars().count();
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(next))));
    }
    state.store(ctx, id);
    true
}

/// Inserts an attachment markdown link at the current cursor position (or at end of text if unselected).
pub fn insert_attachment_link(
    ctx: &Context,
    id: Id,
    text: &mut String,
    filename: &str,
    rel_path: &str,
    is_img: bool,
) {
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    let char_count = text.chars().count();
    let char_index = state
        .cursor
        .char_range()
        .map(|r| usize::from(r.primary.index))
        .unwrap_or(char_count)
        .min(char_count);

    let tag = if is_img {
        format!("\n![{filename}]({rel_path})\n")
    } else {
        format!("\n[{filename}]({rel_path})\n")
    };

    let byte_index = char_to_byte(text, char_index);
    text.insert_str(byte_index, &tag);

    let new_cursor = char_index + tag.chars().count();
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(new_cursor))));
    state.store(ctx, id);
    ctx.memory_mut(|memory| memory.request_focus(id));
}

#[derive(Clone, Copy)]
enum LineEdit {
    Toggle(&'static str),
    Indent,
    Outdent,
}

fn wrap_selection(
    text: &mut String,
    selected: std::ops::Range<usize>,
    opening: &str,
    closing: &str,
    placeholder: &str,
) -> std::ops::Range<usize> {
    let start = char_to_byte(text, selected.start);
    let end = char_to_byte(text, selected.end);
    if selected.is_empty() {
        let insertion = format!("{opening}{placeholder}{closing}");
        text.insert_str(start, &insertion);
        let selection_start = selected.start + opening.chars().count();
        return selection_start..selection_start + placeholder.chars().count();
    }

    if start >= opening.len()
        && text[..start].ends_with(opening)
        && text[end..].starts_with(closing)
    {
        text.replace_range(end..end + closing.len(), "");
        text.replace_range(start - opening.len()..start, "");
        let opening_chars = opening.chars().count();
        return selected.start - opening_chars..selected.end - opening_chars;
    }

    text.insert_str(end, closing);
    text.insert_str(start, opening);
    let opening_chars = opening.chars().count();
    selected.start + opening_chars..selected.end + opening_chars
}

fn edit_selected_lines(
    text: &mut String,
    selected: std::ops::Range<usize>,
    edit: LineEdit,
) -> std::ops::Range<usize> {
    let start_byte = char_to_byte(text, selected.start);
    let end_byte = char_to_byte(text, selected.end);
    let line_start = text[..start_byte].rfind('\n').map_or(0, |index| index + 1);
    let line_end = text[end_byte..]
        .find('\n')
        .map_or(text.len(), |index| end_byte + index + 1);
    let replacement = text[line_start..line_end]
        .split_inclusive('\n')
        .map(|line| edit_line(line, edit))
        .collect::<String>();
    let selection_start = text[..line_start].chars().count();
    let selection_end = selection_start + replacement.chars().count();
    text.replace_range(line_start..line_end, &replacement);
    selection_start..selection_end
}

fn edit_line(line: &str, edit: LineEdit) -> String {
    let (content, newline) = line
        .strip_suffix('\n')
        .map_or((line, ""), |content| (content, "\n"));
    match edit {
        LineEdit::Indent => format!("    {content}{newline}"),
        LineEdit::Outdent => {
            let trimmed = content.strip_prefix('\t').unwrap_or_else(|| {
                let spaces = content
                    .bytes()
                    .take_while(|byte| *byte == b' ')
                    .count()
                    .min(4);
                &content[spaces..]
            });
            format!("{trimmed}{newline}")
        }
        LineEdit::Toggle(marker) => {
            let indent_bytes = content.len() - content.trim_start().len();
            let (indent, body) = content.split_at(indent_bytes);
            if let Some(without_marker) = body.strip_prefix(marker) {
                format!("{indent}{without_marker}{newline}")
            } else {
                format!("{indent}{marker}{body}{newline}")
            }
        }
    }
}

fn continuation_marker(line: &str) -> Option<String> {
    for marker in ["- [ ] ", "- [x] ", "- [X] ", "- ", "* ", "+ "] {
        if line.starts_with(marker) {
            return Some(marker.to_owned());
        }
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0 && line[digits..].starts_with(". ")).then(|| line[..digits + 2].to_owned())
}

fn next_marker(marker: &str) -> String {
    let digits = marker.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 {
        let number = marker[..digits].parse::<usize>().unwrap_or(0) + 1;
        format!("{number}. ")
    } else if marker == "- [x] " || marker == "- [X] " {
        "- [ ] ".to_owned()
    } else {
        marker.to_owned()
    }
}

fn char_to_byte(text: &str, character_index: usize) -> usize {
    text.char_indices()
        .nth(character_index)
        .map_or(text.len(), |(index, _)| index)
}

fn text_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn visual_hash(visuals: &Visuals) -> u64 {
    let mut hasher = DefaultHasher::new();
    visuals.text_color().to_array().hash(&mut hasher);
    visuals.weak_text_color().to_array().hash(&mut hasher);
    visuals.hyperlink_color.to_array().hash(&mut hasher);
    visuals.code_bg_color.to_array().hash(&mut hasher);
    hasher.finish()
}

/// Returns the source character under the pointer.
pub fn hovered_character(ui: &Ui, output: &TextEditOutput) -> Option<usize> {
    let pointer = ui.input(|input| input.pointer.hover_pos())?;
    if !output.text_clip_rect.contains(pointer) || !output.response.response.rect.contains(pointer)
    {
        return None;
    }

    let local_position = pointer - output.galley_pos;
    Some(output.galley.cursor_from_pos(local_position).index.into())
}

/// Toggles a task marker only when the pointer is over that marker.
pub fn toggle_checkbox_at_character(text: &mut String, character_index: usize) -> bool {
    let byte_index = text
        .char_indices()
        .nth(character_index)
        .map_or(text.len(), |(index, _)| index);
    let line_start = text[..byte_index].rfind('\n').map_or(0, |index| index + 1);
    let line_end = text[byte_index..]
        .find('\n')
        .map_or(text.len(), |index| byte_index + index);
    let line = &text[line_start..line_end];
    let indent = line.len() - line.trim_start().len();
    let marker_start = line_start + indent;
    let marker_end = marker_start + 6;
    if byte_index < marker_start || byte_index >= marker_end || marker_end > text.len() {
        return false;
    }

    let marker = &text[marker_start..marker_end];
    let replacement = match marker {
        "- [ ] " => "- [x] ",
        "- [x] " | "- [X] " => "- [ ] ",
        _ => return false,
    };
    text.replace_range(marker_start..marker_end, replacement);
    true
}

fn line_at_character(text: &str, character_index: usize) -> usize {
    text.chars()
        .take(character_index)
        .filter(|character| *character == '\n')
        .count()
}

fn highlight(
    source: &str,
    visuals: &Visuals,
    active_line: Option<usize>,
    body_size: f32,
) -> LayoutJob {
    let palette = Palette::new(visuals, body_size);
    let mut job = LayoutJob::default();
    if source.len() > MAX_HIGHLIGHT_BYTES {
        append(&mut job, source, palette.body);
        return job;
    }
    let mut inside_code_block = false;

    // Galley text must exactly match the editable source.
    for (line_index, source_line) in source.split_inclusive('\n').enumerate() {
        let (line, newline) = source_line
            .strip_suffix('\n')
            .map_or((source_line, ""), |line| (line, "\n"));

        append_line(
            &mut job,
            line,
            &palette,
            &mut inside_code_block,
            active_line == Some(line_index),
        );
        append(&mut job, newline, palette.body.clone());
    }

    job
}

struct Palette {
    body: TextFormat,
    marker: TextFormat,
    hidden_marker: TextFormat,
    accent: TextFormat,
    inline_code: TextFormat,
    code_block: TextFormat,
}

impl Palette {
    fn new(visuals: &Visuals, body_size: f32) -> Self {
        let body = format(FontFamily::Proportional, body_size, visuals.text_color());
        let marker = format(
            FontFamily::Monospace,
            body_size - 1.0,
            visuals.weak_text_color(),
        );
        let hidden_marker = format(
            FontFamily::Monospace,
            HIDDEN_MARKER_SIZE,
            Color32::TRANSPARENT,
        );

        let mut accent = body.clone();
        accent.color = visuals.hyperlink_color;

        let mut inline_code = format(
            FontFamily::Monospace,
            body_size - 0.5,
            visuals.hyperlink_color,
        );
        inline_code.background = visuals.code_bg_color;

        let mut code_block = format(FontFamily::Monospace, body_size - 0.5, visuals.text_color());
        code_block.background = visuals.code_bg_color;

        Self {
            body,
            marker,
            hidden_marker,
            accent,
            inline_code,
            code_block,
        }
    }

    fn source_marker(&self, visible: bool) -> TextFormat {
        if visible {
            self.marker.clone()
        } else {
            self.hidden_marker.clone()
        }
    }
}

fn format(family: FontFamily, size: f32, color: Color32) -> TextFormat {
    TextFormat {
        font_id: FontId::new(size, family),
        color,
        line_height: Some(size * 1.55),
        ..Default::default()
    }
}

fn append_line(
    job: &mut LayoutJob,
    line: &str,
    palette: &Palette,
    inside_code_block: &mut bool,
    show_source: bool,
) {
    let trimmed = line.trim_start();
    let indent_length = line.len() - trimmed.len();

    if trimmed.starts_with("```") {
        append(job, line, palette.source_marker(show_source));
        *inside_code_block = !*inside_code_block;
        return;
    }

    if *inside_code_block {
        append(job, line, palette.code_block.clone());
        return;
    }

    if let Some(level) = heading_level(trimmed) {
        append(job, &line[..indent_length], palette.body.clone());

        let marker_length = level + 1;
        let heading_size = palette.body.font_id.size
            * match level {
                1 => 1.73,
                2 => 1.47,
                3 => 1.27,
                _ => 1.13,
            };
        let heading = format(FontFamily::Proportional, heading_size, palette.body.color);
        let mut heading_marker = heading.clone();
        heading_marker.color = palette.marker.color;

        append(
            job,
            &trimmed[..marker_length],
            if show_source {
                heading_marker
            } else {
                palette.hidden_marker.clone()
            },
        );
        append(job, &trimmed[marker_length..], heading);
        return;
    }

    if let Some(rest) = trimmed.strip_prefix('>') {
        append(job, &line[..indent_length], palette.body.clone());
        append(
            job,
            ">",
            if show_source {
                palette.accent.clone()
            } else {
                palette.hidden_marker.clone()
            },
        );

        let mut quote = palette.body.clone();
        quote.italics = true;
        append_inline(job, rest, &quote, palette, show_source);
        return;
    }

    // TextEdit cannot embed a painted separator in its galley.
    if is_horizontal_rule(trimmed) {
        append(job, line, palette.marker.clone());
        return;
    }

    if let Some(marker_length) = list_marker_length(trimmed) {
        append(job, &line[..indent_length], palette.body.clone());

        // List markers are rendered content, not hidden syntax.
        append(job, &trimmed[..marker_length], palette.accent.clone());
        append_inline(
            job,
            &trimmed[marker_length..],
            &palette.body,
            palette,
            show_source,
        );
        return;
    }

    append_inline(job, line, &palette.body, palette, show_source);
}

fn heading_level(line: &str) -> Option<usize> {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    (1..=6)
        .contains(&hashes)
        .then(|| line.as_bytes().get(hashes))
        .flatten()
        .filter(|byte| **byte == b' ')
        .map(|_| hashes)
}

fn is_horizontal_rule(line: &str) -> bool {
    let compact: String = line.chars().filter(|character| *character != ' ').collect();
    compact.len() >= 3
        && (compact.chars().all(|character| character == '-')
            || compact.chars().all(|character| character == '*')
            || compact.chars().all(|character| character == '_'))
}

fn list_marker_length(line: &str) -> Option<usize> {
    if line.starts_with("- [ ] ") || line.starts_with("- [x] ") || line.starts_with("- [X] ") {
        return Some(6);
    }
    if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
        return Some(2);
    }

    let digit_count = line.bytes().take_while(u8::is_ascii_digit).count();
    (digit_count > 0 && line[digit_count..].starts_with(". ")).then_some(digit_count + 2)
}

fn append_inline(
    job: &mut LayoutJob,
    mut source: &str,
    base: &TextFormat,
    palette: &Palette,
    show_source: bool,
) {
    while !source.is_empty() {
        let marker_position = source
            .char_indices()
            .find(|(_, character)| matches!(character, '`' | '*' | '_' | '~' | '['))
            .map_or(source.len(), |(position, _)| position);

        if marker_position > 0 {
            append(job, &source[..marker_position], base.clone());
            source = &source[marker_position..];
            continue;
        }

        if let Some(consumed) = append_inline_token(job, source, base, palette, show_source) {
            source = &source[consumed..];
            continue;
        }

        let character_length = source.chars().next().map_or(0, char::len_utf8);
        append(job, &source[..character_length], base.clone());
        source = &source[character_length..];
    }
}

fn append_inline_token(
    job: &mut LayoutJob,
    source: &str,
    base: &TextFormat,
    palette: &Palette,
    show_source: bool,
) -> Option<usize> {
    let marker = || palette.source_marker(show_source);

    if let Some(without_opening) = source.strip_prefix("[[") {
        let closing = without_opening.find("]]")? + 2;
        let end = closing + 2;
        let mut link = palette.accent.clone();
        link.underline = Stroke::new(1.0, link.color);
        append(job, "[[", marker());

        let inner = &source[2..closing];
        if show_source {
            append(job, inner, link);
        } else if let Some(separator) = inner.find('|') {
            // Keep hidden target bytes to preserve cursor indices.
            append(job, &inner[..separator + 1], palette.hidden_marker.clone());
            append(job, &inner[separator + 1..], link);
        } else {
            append(job, inner, link);
        }
        append(job, "]]", marker());
        return Some(end);
    }

    if source.starts_with('[') {
        let label_end = source.find("](")?;
        let target_end = source[label_end + 2..].find(')')? + label_end + 2;
        let mut link = palette.accent.clone();
        link.underline = Stroke::new(1.0, link.color);
        append(job, "[", marker());
        append(job, &source[1..label_end], link);
        append(job, "](", marker());
        append(job, &source[label_end + 2..target_end], marker());
        append(job, ")", marker());
        return Some(target_end + 1);
    }

    if let Some(without_opening) = source.strip_prefix('`') {
        let closing = without_opening.find('`')? + 1;
        append(job, "`", marker());
        append(job, &source[1..closing], palette.inline_code.clone());
        append(job, "`", marker());
        return Some(closing + 1);
    }

    for delimiter in ["**", "__"] {
        if source.starts_with(delimiter) {
            let closing = source[2..].find(delimiter)? + 2;
            let mut strong = base.clone();
            strong.color = palette.accent.color;
            strong.extra_letter_spacing = 0.25;
            append(job, delimiter, marker());
            append(job, &source[2..closing], strong);
            append(job, delimiter, marker());
            return Some(closing + 2);
        }
    }

    if let Some(without_opening) = source.strip_prefix("~~") {
        let closing = without_opening.find("~~")? + 2;
        let mut struck = base.clone();
        struck.strikethrough = Stroke::new(1.0, base.color);
        append(job, "~~", marker());
        append(job, &source[2..closing], struck);
        append(job, "~~", marker());
        return Some(closing + 2);
    }

    for delimiter in ['*', '_'] {
        if source.starts_with(delimiter) {
            let delimiter_length = delimiter.len_utf8();
            let closing = source[delimiter_length..].find(delimiter)? + delimiter_length;
            let mut italic = base.clone();
            italic.italics = true;
            append(job, &source[..delimiter_length], marker());
            append(job, &source[delimiter_length..closing], italic);
            append(job, &source[closing..closing + delimiter_length], marker());
            return Some(closing + delimiter_length);
        }
    }

    None
}

fn append(job: &mut LayoutJob, text: &str, text_format: TextFormat) {
    job.append(text, 0.0, text_format);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighting_never_changes_the_source_text() {
        let source = "# Заголовок\n\n- **жирный** и `код`\n[[Другая заметка|ссылка]] 🦀\n";
        let job = highlight(source, &Visuals::dark(), None, 15.0);

        assert_eq!(job.text, source);
    }

    #[test]
    fn incomplete_markers_remain_editable() {
        let source = "Незакрытые **звёздочки и [ссылка";
        let job = highlight(source, &Visuals::dark(), Some(0), 15.0);

        assert_eq!(job.text, source);
    }

    #[test]
    fn cursor_line_is_counted_by_characters_not_utf8_bytes() {
        let source = "ёж 🦀\nвторая строка";
        let first_line_character_count = "ёж 🦀\n".chars().count();

        assert_eq!(line_at_character(source, first_line_character_count), 1);
    }

    #[test]
    fn inactive_line_contains_collapsed_marker_sections() {
        let job = highlight("**text**", &Visuals::dark(), None, 15.0);

        assert!(
            job.sections
                .iter()
                .any(|section| section.format.font_id.size == HIDDEN_MARKER_SIZE)
        );
    }

    #[test]
    fn checkbox_toggles_only_from_its_marker() {
        let mut text = "  - [ ] task".to_owned();
        assert!(toggle_checkbox_at_character(&mut text, 4));
        assert_eq!(text, "  - [x] task");
        assert!(!toggle_checkbox_at_character(&mut text, 10));
    }

    #[test]
    fn formatting_wraps_unicode_selection_by_character_index() {
        let mut text = "ёжик text".to_owned();

        let selected = wrap_selection(&mut text, 0..4, "**", "**", "bold text");

        assert_eq!(text, "**ёжик** text");
        assert_eq!(selected, 2..6);
    }

    #[test]
    fn line_actions_indent_and_toggle_tasks() {
        let mut text = "first\nsecond\n".to_owned();
        let selection = edit_selected_lines(&mut text, 0..12, LineEdit::Toggle("- [ ] "));
        assert_eq!(text, "- [ ] first\n- [ ] second\n");

        edit_selected_lines(&mut text, selection, LineEdit::Indent);
        assert_eq!(text, "    - [ ] first\n    - [ ] second\n");
    }

    #[test]
    fn numbered_lists_continue_with_the_next_number() {
        assert_eq!(continuation_marker("9. item").as_deref(), Some("9. "));
        assert_eq!(next_marker("9. "), "10. ");
        assert_eq!(next_marker("- [x] "), "- [ ] ");
    }

    #[test]
    fn very_large_notes_keep_source_text_intact() {
        let source = "**large note**\n".repeat(40_000);

        let job = highlight(&source, &Visuals::dark(), None, 15.0);

        assert_eq!(job.text, source);
        assert_eq!(job.sections.len(), 1);
    }

    #[test]
    fn extract_outline_ignores_code_blocks_and_tracks_levels() {
        let text = r#"# Main Header

Some intro text here.

```rust
# This is a comment in code, not a header
fn main() {}
```

## Section 1: Intro
Content

### Subsection 1.1
More content

~~~python
# Another code comment
~~~

## Section 2: Details
Final thoughts"#;

        let outline = extract_outline(text);
        assert_eq!(outline.len(), 4);
        assert_eq!(outline[0].level, 1);
        assert_eq!(outline[0].title, "Main Header");
        assert_eq!(outline[1].level, 2);
        assert_eq!(outline[1].title, "Section 1: Intro");
        assert_eq!(outline[2].level, 3);
        assert_eq!(outline[2].title, "Subsection 1.1");
        assert_eq!(outline[3].level, 2);
        assert_eq!(outline[3].title, "Section 2: Details");
    }

    #[test]
    fn count_words_and_chars_handles_unicode() {
        let text = "Привет, мир! 🦀\nTwo words.";
        let (words, chars) = count_words_and_chars(text);
        assert_eq!(words, 5);
        assert_eq!(chars, text.chars().count());
    }
}
