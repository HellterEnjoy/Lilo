//! Grouped transient state owned by [`super::LiloApp`].
//!
//! Each struct collects fields that belong to one workflow so the root
//! application type stays readable and new features do not keep widening it.

use super::RecoveryTab;
use crate::analytics::AnalyticsClient;
use std::path::PathBuf;
use std::time::Instant;
use uuid::Uuid;

/// Opt-in analytics delivery state. All network work runs outside the UI thread.
pub(super) struct AnalyticsRuntime {
    pub(super) client: AnalyticsClient,
    pub(super) dirty: bool,
    pub(super) report_in_flight: bool,
    pub(super) deletion_in_flight: bool,
    pub(super) next_send: Instant,
    pub(super) next_delete_attempt: Instant,
    pub(super) status: Option<String>,
    pub(super) details_open: bool,
}

impl AnalyticsRuntime {
    pub(super) fn new(client: AnalyticsClient, dirty: bool, first_send: Instant) -> Self {
        Self {
            client,
            dirty,
            report_in_flight: false,
            deletion_in_flight: false,
            next_send: first_send,
            next_delete_attempt: Instant::now(),
            status: None,
            details_open: false,
        }
    }
}

/// Trash, backups, diagnostics, import/export and attachment inspection.
#[derive(Default)]
pub(super) struct RecoveryState {
    pub(super) tab: RecoveryTab,
    pub(super) selected_backup: Option<PathBuf>,
    pub(super) backup_preview: String,
    pub(super) diagnostics: Vec<String>,
    pub(super) import_path_buffer: String,
    pub(super) export_path_buffer: String,
    pub(super) attachment_orphans: Vec<PathBuf>,
    pub(super) attachments_inspected: bool,
}

/// Back/forward note navigation.
#[derive(Default)]
pub(super) struct NoteHistory {
    pub(super) back: Vec<Uuid>,
    pub(super) forward: Vec<Uuid>,
    pub(super) navigating: bool,
}

/// Folder creation, renaming and deletion dialogs.
#[derive(Default)]
pub(super) struct FolderUiState {
    pub(super) show_new_input: bool,
    pub(super) new_name: String,
    pub(super) editing: Option<PathBuf>,
    pub(super) name_buffer: String,
    pub(super) pending_delete: Option<PathBuf>,
    pub(super) pending_delete_notes_count: usize,
}

/// Tag and alias editing, tag renaming and saved-search creation.
#[derive(Default)]
pub(super) struct TagUiState {
    pub(super) rename_dialog_open: bool,
    pub(super) to_rename: String,
    pub(super) new_name_buffer: String,
    pub(super) show_new_preset_input: bool,
    pub(super) new_preset_name_buffer: String,
    pub(super) new_tag: String,
    pub(super) new_alias: String,
}

/// In-note find and replace bar.
#[derive(Default)]
pub(super) struct FindState {
    pub(super) open: bool,
    pub(super) show_replace: bool,
    pub(super) query: String,
    pub(super) replacement: String,
    pub(super) case_sensitive: bool,
    /// Index of the highlighted match in the current result list.
    pub(super) current: usize,
    pub(super) focus_query: bool,
    /// Scroll the editor to the current match on the next layout.
    pub(super) scroll_to_current: bool,
    pub(super) cache: Option<(u64, Vec<std::ops::Range<usize>>)>,
}

/// `[[wiki link]]` and `#tag` suggestions shown under the caret.
#[derive(Default)]
pub(super) struct CompletionState {
    pub(super) note_id: Option<Uuid>,
    pub(super) context: Option<crate::editor_tools::CompletionContext>,
    pub(super) items: Vec<String>,
    pub(super) selected: usize,
    /// Screen position just below the caret, where the popup is anchored.
    pub(super) anchor: Option<eframe::egui::Pos2>,
    /// A context the user dismissed with Escape, as (note, query start).
    pub(super) dismissed: Option<(Uuid, usize)>,
    /// Item chosen with the keyboard, applied after the editor is laid out.
    pub(super) accept: Option<usize>,
    /// The context changed and `items` must be recomputed.
    pub(super) stale: bool,
}

impl CompletionState {
    pub(super) fn is_visible(&self) -> bool {
        self.context.is_some() && !self.items.is_empty()
    }

    pub(super) fn dismiss(&mut self) {
        if let (Some(note_id), Some(context)) = (self.note_id, &self.context) {
            self.dismissed = Some((note_id, context.start));
        }
        self.clear();
    }

    pub(super) fn clear(&mut self) {
        self.context = None;
        self.items.clear();
        self.selected = 0;
        self.anchor = None;
        self.accept = None;
    }
}
