//! Opt-in, privacy-preserving usage analytics.
//!
//! Only a random installation identifier, local calendar date, application
//! version and counters from the fixed feature whitelist leave the device.

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use uuid::Uuid;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DAILY_COUNT: u16 = 1_000;

pub const FEATURE_NAMES: [&str; 21] = [
    "note_created",
    "daily_note_opened",
    "quick_capture_saved",
    "template_note_created",
    "template_inserted",
    "markdown_formatting_used",
    "search_used",
    "saved_search_created",
    "command_palette_used",
    "wiki_link_opened",
    "graph_opened",
    "tag_filter_used",
    "attachment_added",
    "note_pinned",
    "folder_created",
    "trash_restored",
    "backup_restored",
    "markdown_imported",
    "vault_exported",
    "zen_mode_enabled",
    "always_on_top_enabled",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalyticsFeature {
    NoteCreated,
    DailyNoteOpened,
    QuickCaptureSaved,
    TemplateNoteCreated,
    TemplateInserted,
    MarkdownFormattingUsed,
    SearchUsed,
    SavedSearchCreated,
    CommandPaletteUsed,
    WikiLinkOpened,
    GraphOpened,
    TagFilterUsed,
    AttachmentAdded,
    NotePinned,
    FolderCreated,
    TrashRestored,
    BackupRestored,
    MarkdownImported,
    VaultExported,
    ZenModeEnabled,
    AlwaysOnTopEnabled,
}

impl AnalyticsFeature {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoteCreated => "note_created",
            Self::DailyNoteOpened => "daily_note_opened",
            Self::QuickCaptureSaved => "quick_capture_saved",
            Self::TemplateNoteCreated => "template_note_created",
            Self::TemplateInserted => "template_inserted",
            Self::MarkdownFormattingUsed => "markdown_formatting_used",
            Self::SearchUsed => "search_used",
            Self::SavedSearchCreated => "saved_search_created",
            Self::CommandPaletteUsed => "command_palette_used",
            Self::WikiLinkOpened => "wiki_link_opened",
            Self::GraphOpened => "graph_opened",
            Self::TagFilterUsed => "tag_filter_used",
            Self::AttachmentAdded => "attachment_added",
            Self::NotePinned => "note_pinned",
            Self::FolderCreated => "folder_created",
            Self::TrashRestored => "trash_restored",
            Self::BackupRestored => "backup_restored",
            Self::MarkdownImported => "markdown_imported",
            Self::VaultExported => "vault_exported",
            Self::ZenModeEnabled => "zen_mode_enabled",
            Self::AlwaysOnTopEnabled => "always_on_top_enabled",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalyticsSettings {
    /// `None` means the user has not answered the first-run consent prompt.
    pub consent: Option<bool>,
    pub installation_id: Option<Uuid>,
    pub counter_date: String,
    pub feature_counts: BTreeMap<String, u16>,
    pub pending_deletion_id: Option<Uuid>,
}

impl Default for AnalyticsSettings {
    fn default() -> Self {
        Self {
            consent: None,
            installation_id: None,
            counter_date: local_date(),
            feature_counts: BTreeMap::new(),
            pending_deletion_id: None,
        }
    }
}

impl AnalyticsSettings {
    pub fn enabled(&self) -> bool {
        self.consent == Some(true)
    }

    pub fn enable(&mut self) {
        self.consent = Some(true);
        self.installation_id.get_or_insert_with(Uuid::new_v4);
        self.prepare_current_date();
    }

    pub fn decline(&mut self) {
        self.consent = Some(false);
        self.installation_id = None;
        self.feature_counts.clear();
        self.counter_date = local_date();
    }

    pub fn disable_and_queue_deletion(&mut self) {
        self.consent = Some(false);
        if let Some(installation_id) = self.installation_id.take() {
            self.pending_deletion_id = Some(installation_id);
        }
        self.feature_counts.clear();
        self.counter_date = local_date();
    }

    pub fn finish_deletion(&mut self, installation_id: Uuid) {
        if self.pending_deletion_id == Some(installation_id) {
            self.pending_deletion_id = None;
        }
    }

    pub fn record(&mut self, feature: AnalyticsFeature) -> bool {
        if !self.enabled() {
            return false;
        }
        self.prepare_current_date();
        let count = self
            .feature_counts
            .entry(feature.as_str().to_owned())
            .or_default();
        *count = count.saturating_add(1).min(MAX_DAILY_COUNT);
        true
    }

    pub fn daily_payload(&mut self) -> Option<DailyPayload> {
        if !self.enabled() {
            return None;
        }
        self.prepare_current_date();
        Some(DailyPayload {
            installation_id: self.installation_id?,
            date: self.counter_date.clone(),
            app_version: env!("CARGO_PKG_VERSION"),
            features: self.feature_counts.clone(),
        })
    }

    fn prepare_current_date(&mut self) {
        let today = local_date();
        if self.counter_date != today {
            self.counter_date = today;
            self.feature_counts.clear();
        }
    }
}

fn local_date() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

#[derive(Clone, Debug, Serialize)]
pub struct DailyPayload {
    installation_id: Uuid,
    date: String,
    app_version: &'static str,
    features: BTreeMap<String, u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalyticsOperation {
    DailyReport,
    DeleteInstallation,
}

#[derive(Clone, Debug)]
pub enum AnalyticsResult {
    Delivered(AnalyticsOperation, Option<Uuid>),
    Failed(AnalyticsOperation),
}

enum AnalyticsRequest {
    Daily(DailyPayload),
    Delete(Uuid),
}

pub struct AnalyticsClient {
    request_sender: Sender<AnalyticsRequest>,
    result_receiver: Receiver<AnalyticsResult>,
}

impl AnalyticsClient {
    pub fn new(endpoint: Option<&'static str>) -> Self {
        let endpoint = endpoint
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.trim_end_matches('/').to_owned());
        let (request_sender, request_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();

        std::thread::spawn(move || {
            let config = ureq::Agent::config_builder()
                .timeout_global(Some(REQUEST_TIMEOUT))
                .build();
            let agent: ureq::Agent = config.into();

            while let Ok(request) = request_receiver.recv() {
                let result = match request {
                    AnalyticsRequest::Daily(payload) => {
                        let delivered = endpoint.as_ref().is_some_and(|base| {
                            agent
                                .post(format!("{base}/v1/daily"))
                                .send_json(&payload)
                                .is_ok()
                        });
                        if delivered {
                            AnalyticsResult::Delivered(AnalyticsOperation::DailyReport, None)
                        } else {
                            AnalyticsResult::Failed(AnalyticsOperation::DailyReport)
                        }
                    }
                    AnalyticsRequest::Delete(installation_id) => {
                        #[derive(Serialize)]
                        struct DeletePayload {
                            installation_id: Uuid,
                        }

                        let delivered = endpoint.as_ref().is_some_and(|base| {
                            agent
                                .delete(format!("{base}/v1/data"))
                                .force_send_body()
                                .send_json(&DeletePayload { installation_id })
                                .is_ok()
                        });
                        if delivered {
                            AnalyticsResult::Delivered(
                                AnalyticsOperation::DeleteInstallation,
                                Some(installation_id),
                            )
                        } else {
                            AnalyticsResult::Failed(AnalyticsOperation::DeleteInstallation)
                        }
                    }
                };
                let _ = result_sender.send(result);
            }
        });

        Self {
            request_sender,
            result_receiver,
        }
    }

    pub fn send_daily(&self, payload: DailyPayload) -> bool {
        self.request_sender
            .send(AnalyticsRequest::Daily(payload))
            .is_ok()
    }

    pub fn delete_installation(&self, installation_id: Uuid) -> bool {
        self.request_sender
            .send(AnalyticsRequest::Delete(installation_id))
            .is_ok()
    }

    pub fn try_result(&self) -> Option<AnalyticsResult> {
        self.result_receiver.try_recv().ok()
    }
}

pub const fn configured_endpoint() -> Option<&'static str> {
    match option_env!("LILO_ANALYTICS_ENDPOINT") {
        Some(endpoint) => Some(endpoint),
        None => Some("https://lilo-analytics.miaccu23.workers.dev"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytics_is_opt_in_and_creates_id_only_after_consent() {
        let mut settings = AnalyticsSettings::default();
        assert_eq!(settings.consent, None);
        assert!(settings.installation_id.is_none());
        assert!(!settings.record(AnalyticsFeature::NoteCreated));

        settings.enable();
        assert!(settings.enabled());
        assert!(settings.installation_id.is_some());
        assert!(settings.record(AnalyticsFeature::NoteCreated));
    }

    #[test]
    fn counters_are_bounded_and_only_use_whitelisted_names() {
        let mut settings = AnalyticsSettings::default();
        settings.enable();
        for _ in 0..1_100 {
            settings.record(AnalyticsFeature::GraphOpened);
        }
        assert_eq!(settings.feature_counts["graph_opened"], MAX_DAILY_COUNT);
        assert!(
            settings
                .feature_counts
                .keys()
                .all(|name| FEATURE_NAMES.contains(&name.as_str()))
        );
    }

    #[test]
    fn disabling_queues_deletion_and_forgets_local_counters() {
        let mut settings = AnalyticsSettings::default();
        settings.enable();
        let installation_id = settings.installation_id.unwrap();
        settings.record(AnalyticsFeature::SearchUsed);

        settings.disable_and_queue_deletion();

        assert_eq!(settings.consent, Some(false));
        assert_eq!(settings.pending_deletion_id, Some(installation_id));
        assert!(settings.installation_id.is_none());
        assert!(settings.feature_counts.is_empty());
    }
}
