use super::*;

#[derive(Serialize, Deserialize)]
struct NoteFrontmatter {
    id: Uuid,
    title: String,
    created_at: DateTime<Local>,
    updated_at: DateTime<Local>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    aliases: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,

    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pinned: bool,
}

#[derive(Clone)]
pub struct Note {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub file_path: PathBuf,
    pub search_text: String,
}

impl Note {
    pub fn new(notes_dir: &Path) -> Self {
        Self::new_named(notes_dir, "")
    }

    pub fn new_named(notes_dir: &Path, title: &str) -> Self {
        let id = Uuid::new_v4();
        let now = Local::now();
        let title = title.trim().to_owned();
        let file_title = if title.is_empty() {
            "Untitled"
        } else {
            title.as_str()
        };
        let file_path = notes_dir.join(note_file_name(file_title, id));

        let mut note = Self {
            id,
            title,
            content: String::new(),
            created_at: now,
            updated_at: now,
            aliases: Vec::new(),
            tags: Vec::new(),
            pinned: false,
            file_path,
            search_text: String::new(),
        };
        note.refresh_search_text();
        note
    }

    pub fn mark_as_updated(&mut self) {
        self.updated_at = Local::now();
        self.refresh_search_text();
    }

    pub fn refresh_search_text(&mut self) {
        self.search_text = format!("{}\n{}", self.title, self.content).to_lowercase();
    }
}

#[derive(Default)]
pub struct AppData {
    pub notes: Vec<Note>,
    pub selected_note_id: Option<Uuid>,
}

impl AppData {
    pub fn create_note(&mut self, notes_dir: &Path) -> Uuid {
        let note = Note::new(notes_dir);
        let id = note.id;
        self.notes.push(note);
        self.selected_note_id = Some(id);
        id
    }

    pub fn create_note_named(&mut self, notes_dir: &Path, title: &str) -> Uuid {
        let note = Note::new_named(notes_dir, title);
        let id = note.id;
        self.notes.push(note);
        self.selected_note_id = Some(id);
        id
    }

    pub fn remove_note(&mut self, id: Uuid) -> Option<Note> {
        let index = self.notes.iter().position(|note| note.id == id)?;
        let note = self.notes.remove(index);

        if self.selected_note_id == Some(id) {
            self.selected_note_id = self
                .notes
                .get(index)
                .or_else(|| self.notes.last())
                .map(|note| note.id);
        }

        Some(note)
    }

    pub fn selected_note(&self) -> Option<&Note> {
        let selected_id = self.selected_note_id?;
        self.notes.iter().find(|note| note.id == selected_id)
    }

    pub fn selected_note_mut(&mut self) -> Option<&mut Note> {
        let selected_id = self.selected_note_id?;
        self.notes.iter_mut().find(|note| note.id == selected_id)
    }

    pub(super) fn normalize_selection(&mut self) {
        let selection_exists = self
            .selected_note_id
            .is_some_and(|id| self.notes.iter().any(|note| note.id == id));

        if !selection_exists {
            self.selected_note_id = self.notes.first().map(|note| note.id);
        }
    }
}

pub fn save_note(note: &Note) -> StorageResult<()> {
    if note.file_path.exists() && fs::metadata(&note.file_path)?.permissions().readonly() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is read-only", note.file_path.display()),
        )
        .into());
    }
    if let Some(parent) = note.file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut aliases = note.aliases.clone();
    if !note.title.trim().is_empty()
        && !aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(note.title.trim()))
    {
        aliases.push(note.title.trim().to_owned());
    }

    let metadata = NoteFrontmatter {
        id: note.id,
        title: note.title.clone(),
        created_at: note.created_at,
        updated_at: note.updated_at,
        aliases,
        tags: note.tags.clone(),
        pinned: note.pinned,
    };
    let yaml = serde_saphyr::to_string(&metadata)?;
    let markdown = format!("---\n{}\n---\n\n{}", yaml.trim_end(), note.content);
    atomic_write(&note.file_path, markdown.as_bytes())?;
    Ok(())
}

pub fn move_note_to_folder(
    note: &mut Note,
    paths: &StoragePaths,
    target_relative: &Path,
) -> StorageResult<()> {
    let source_relative = note
        .file_path
        .strip_prefix(&paths.notes_dir)
        .map_err(|_| io::Error::other("Refusing to move a note outside the Notes directory"))?;
    if !is_safe_relative_path(source_relative) || source_relative.as_os_str().is_empty() {
        return Err(io::Error::other("Note has an unsafe relative path").into());
    }

    let target_directory = ensure_note_folder(&paths.notes_dir, target_relative)?;
    let file_name = note
        .file_path
        .file_name()
        .ok_or_else(|| io::Error::other("Note path has no file name"))?;
    let destination = target_directory.join(file_name);
    if destination == note.file_path {
        return Ok(());
    }
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "A note file with this name already exists in the target folder",
        )
        .into());
    }

    fs::rename(&note.file_path, &destination)?;
    note.file_path = destination;
    Ok(())
}

/// Creates a safe folder path below `Notes`.
pub fn ensure_note_folder(notes_dir: &Path, relative: &Path) -> StorageResult<PathBuf> {
    if !is_safe_relative_path(relative) {
        return Err(io::Error::other("Folder path must stay inside Notes").into());
    }

    fs::create_dir_all(notes_dir)?;
    let mut current = notes_dir.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(io::Error::other("Invalid folder path component").into());
        };
        validate_folder_name(name.to_string_lossy().as_ref())?;
        current.push(name);

        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::other("Links are not allowed inside Notes folders").into());
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(io::Error::other("A folder path component is not a directory").into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
            }
            Err(error) => return Err(error.into()),
        }
    }

    Ok(current)
}

pub fn create_note_folder(
    notes_dir: &Path,
    parent_relative: &Path,
    name: &str,
) -> StorageResult<PathBuf> {
    validate_folder_name(name)?;
    let parent = ensure_note_folder(notes_dir, parent_relative)?;
    let relative = parent_relative.join(name.trim());
    let target = parent.join(name.trim());

    match fs::create_dir(&target) {
        Ok(()) => Ok(relative),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && target.is_dir() => {
            Err(io::Error::new(io::ErrorKind::AlreadyExists, "Folder already exists").into())
        }
        Err(error) => Err(error.into()),
    }
}

pub fn rename_note_file(note: &mut Note) -> StorageResult<()> {
    let parent = note
        .file_path
        .parent()
        .ok_or_else(|| io::Error::other("Note path has no parent folder"))?;
    let title = if note.title.trim().is_empty() {
        "Untitled"
    } else {
        note.title.trim()
    };
    let destination = parent.join(note_file_name(title, note.id));
    if destination == note.file_path {
        return Ok(());
    }
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "A note with this file name already exists",
        )
        .into());
    }
    fs::rename(&note.file_path, &destination)?;
    note.file_path = destination;
    Ok(())
}

pub fn rename_folder(notes_dir: &Path, relative: &Path, new_name: &str) -> StorageResult<PathBuf> {
    if relative.as_os_str().is_empty() || !is_safe_relative_path(relative) {
        return Err(io::Error::other("The Notes root cannot be renamed").into());
    }
    validate_folder_name(new_name)?;
    let source = notes_dir.join(relative);
    let parent_relative = relative.parent().unwrap_or_else(|| Path::new(""));
    let destination_relative = parent_relative.join(new_name.trim());
    let destination = notes_dir.join(&destination_relative);
    if !source.is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Folder does not exist").into());
    }
    if destination.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Folder already exists").into());
    }
    fs::rename(source, destination)?;
    Ok(destination_relative)
}

#[cfg(test)]
pub(super) fn load_notes(
    notes_dir: &Path,
) -> StorageResult<(Vec<Note>, Vec<String>, Vec<PathBuf>)> {
    load_notes_excluding(notes_dir, &[])
}

pub(super) fn managed_note_exclusions(notes_dir: &Path, settings: &AppSettings) -> Vec<PathBuf> {
    let templates = if settings.templates_folder.as_os_str().is_empty() {
        Path::new("Templates")
    } else {
        &settings.templates_folder
    };
    let attachments = if settings.attachments_folder.as_os_str().is_empty() {
        Path::new("Attachments")
    } else {
        &settings.attachments_folder
    };

    [templates, attachments]
        .into_iter()
        .filter(|path| is_safe_relative_path(path))
        .map(|path| notes_dir.join(path))
        .collect()
}

pub(super) fn load_notes_excluding(
    notes_dir: &Path,
    excluded_directories: &[PathBuf],
) -> StorageResult<(Vec<Note>, Vec<String>, Vec<PathBuf>)> {
    let mut paths = Vec::new();
    let mut folder_paths = vec![PathBuf::new()];
    let mut pending_directories = vec![notes_dir.to_path_buf()];

    while let Some(directory) = pending_directories.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;

            // Never traverse links outside the vault.
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if excluded_directories.contains(&path) {
                    continue;
                }
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') || name_str.eq_ignore_ascii_case("cache") {
                    continue;
                }

                let relative = path
                    .strip_prefix(notes_dir)
                    .map_err(|_| io::Error::other("Folder escaped Notes root"))?
                    .to_path_buf();
                if is_safe_relative_path(&relative) {
                    folder_paths.push(relative);
                    pending_directories.push(path);
                }
                continue;
            }

            let is_markdown = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"));
            if file_type.is_file() && is_markdown {
                paths.push(path);
            }
        }
    }
    paths.sort();
    folder_paths.sort();
    folder_paths.dedup();

    let mut notes = Vec::new();
    let mut warnings = Vec::new();
    let mut ids = HashSet::new();
    for path in paths {
        match load_note(&path) {
            Ok(note) if ids.insert(note.id) => notes.push(note),
            Ok(note) => warnings.push(format!(
                "Skipped duplicate note UUID {} in {}",
                note.id,
                path.display()
            )),
            Err(error) => match recover_corrupt_note(&path) {
                Ok(note) if ids.insert(note.id) => {
                    warnings.push(format!(
                        "Recovered {} without valid frontmatter: {}",
                        path.display(),
                        error
                    ));
                    notes.push(note);
                }
                Ok(_) => warnings.push(format!(
                    "Skipped duplicate recovered note in {}",
                    path.display()
                )),
                Err(recovery_error) => warnings.push(format!(
                    "Failed to load {}: {}; recovery failed: {}",
                    path.display(),
                    error,
                    recovery_error
                )),
            },
        }
    }
    Ok((notes, warnings, folder_paths))
}

fn recover_corrupt_note(path: &Path) -> StorageResult<Note> {
    let raw = fs::read_to_string(path)?;
    let normalized = raw.replace("\r\n", "\n");
    let content = normalized.strip_prefix("---\n").map_or_else(
        || normalized.clone(),
        |after_opening| {
            after_opening
                .split_once("\n---\n")
                .map_or(normalized.clone(), |(_, body)| {
                    body.trim_start_matches('\n').to_owned()
                })
        },
    );
    let metadata = fs::metadata(path)?;
    let modified = metadata.modified().unwrap_or(SystemTime::now());
    let created_at = DateTime::<Local>::from(modified);
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let title = stem
        .rsplit_once("--")
        .map_or(stem.as_str(), |(title, _)| title)
        .replace('-', " ");
    let mut stable_id = 0xcbf29ce484222325_u128;
    for byte in path.to_string_lossy().bytes() {
        stable_id ^= u128::from(byte);
        stable_id = stable_id.wrapping_mul(0x100000001b3);
    }
    let id = Uuid::from_u128(stable_id);
    let mut note = Note {
        id,
        title,
        content,
        created_at,
        updated_at: created_at,
        aliases: Vec::new(),
        tags: Vec::new(),
        pinned: false,
        file_path: path.to_path_buf(),
        search_text: String::new(),
    };
    note.refresh_search_text();
    Ok(note)
}

pub(super) fn is_safe_relative_path(path: &Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
        || path.as_os_str().is_empty()
}

fn validate_folder_name(name: &str) -> StorageResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return Err(io::Error::other("Folder name cannot be empty").into());
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
        return Err(io::Error::other("Folder name contains characters invalid on Windows").into());
    }

    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|reserved_name| trimmed.eq_ignore_ascii_case(reserved_name))
    {
        return Err(io::Error::other("Folder name is reserved on Windows").into());
    }
    Ok(())
}

pub(super) fn load_note(path: &Path) -> StorageResult<Note> {
    let raw = fs::read_to_string(path)?;
    let normalized = raw.replace("\r\n", "\n");
    let after_opening = normalized
        .strip_prefix("---\n")
        .ok_or_else(|| io::Error::other("Markdown file has no YAML frontmatter"))?;
    let closing_position = after_opening
        .find("\n---\n")
        .ok_or_else(|| io::Error::other("YAML frontmatter is not closed"))?;
    let yaml = &after_opening[..closing_position];
    let content = after_opening[closing_position + "\n---\n".len()..]
        .strip_prefix('\n')
        .unwrap_or(&after_opening[closing_position + "\n---\n".len()..])
        .to_owned();
    let metadata: NoteFrontmatter = serde_saphyr::from_str(yaml)?;
    let mut note = Note {
        id: metadata.id,
        title: metadata.title,
        content,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        aliases: metadata.aliases,
        tags: metadata.tags,
        pinned: metadata.pinned,
        file_path: path.to_path_buf(),
        search_text: String::new(),
    };
    note.refresh_search_text();
    Ok(note)
}

pub(super) fn note_file_name(title: &str, id: Uuid) -> String {
    let sanitized = sanitize_file_stem(title);
    let id_text = id.simple().to_string();
    format!("{}--{}.md", sanitized, &id_text[..8])
}

pub(super) fn sanitize_file_stem(title: &str) -> String {
    let mut stem: String = title
        .chars()
        .filter(|character| !character.is_control())
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            other => other,
        })
        .take(80)
        .collect();
    stem = stem.trim().trim_end_matches(['.', ' ']).to_owned();

    if stem.is_empty() {
        stem = "Untitled".to_owned();
    }

    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|reserved_name| stem.eq_ignore_ascii_case(reserved_name))
    {
        stem.insert(0, '_');
    }
    stem
}
