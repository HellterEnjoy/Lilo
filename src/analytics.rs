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
/// Finished days that could not be delivered yet are kept for at most a week,
/// matching the window the collection service accepts.
const MAX_UNSENT_DAYS: usize = 7;

pub const FEATURE_NAMES: [&str; 25] = [
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
    "quick_switcher_used",
    "find_in_note_used",
    "text_replaced",
    "completion_accepted",
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
    QuickSwitcherUsed,
    FindInNoteUsed,
    TextReplaced,
    CompletionAccepted,
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
            Self::QuickSwitcherUsed => "quick_switcher_used",
            Self::FindInNoteUsed => "find_in_note_used",
            Self::TextReplaced => "text_replaced",
            Self::CompletionAccepted => "completion_accepted",
        }
    }
}

/// Counters of a finished day that have not been confirmed by the server yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsentReport {
    pub date: String,
    pub app_version: String,
    pub features: BTreeMap<String, u16>,
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
    /// Previous days whose counters still have to be delivered, oldest first.
    pub unsent_reports: Vec<UnsentReport>,
}

impl Default for AnalyticsSettings {
    fn default() -> Self {
        Self {
            consent: None,
            installation_id: None,
            counter_date: local_date(),
            feature_counts: BTreeMap::new(),
            pending_deletion_id: None,
            unsent_reports: Vec::new(),
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
        self.forget_counters();
    }

    pub fn disable_and_queue_deletion(&mut self) {
        self.consent = Some(false);
        if let Some(installation_id) = self.installation_id.take() {
            self.pending_deletion_id = Some(installation_id);
        }
        self.forget_counters();
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

    /// The next report to deliver: unfinished previous days first, then today.
    pub fn next_payload(&mut self) -> Option<DailyPayload> {
        if !self.enabled() {
            return None;
        }
        self.prepare_current_date();
        let installation_id = self.installation_id?;
        Some(match self.unsent_reports.first() {
            Some(report) => DailyPayload {
                installation_id,
                date: report.date.clone(),
                app_version: report.app_version.clone(),
                features: report.features.clone(),
            },
            None => DailyPayload {
                installation_id,
                date: self.counter_date.clone(),
                app_version: env!("CARGO_PKG_VERSION").to_owned(),
                features: self.feature_counts.clone(),
            },
        })
    }

    /// Marks a delivered day as done. Returns `true` when older days are still waiting.
    pub fn confirm_delivered(&mut self, date: &str) -> bool {
        self.unsent_reports.retain(|report| report.date != date);
        !self.unsent_reports.is_empty()
    }

    fn forget_counters(&mut self) {
        self.feature_counts.clear();
        self.unsent_reports.clear();
        self.counter_date = local_date();
    }

    /// Starts a new counting day. The finished day is kept until the server confirms it,
    /// so short sessions that end before the next delivery are no longer lost.
    fn prepare_current_date(&mut self) {
        let today = local_date();
        if self.counter_date == today {
            return;
        }
        let finished = std::mem::take(&mut self.feature_counts);
        let finished_date = std::mem::replace(&mut self.counter_date, today);
        if self.enabled() && !finished.is_empty() {
            self.unsent_reports
                .retain(|report| report.date != finished_date);
            self.unsent_reports.push(UnsentReport {
                date: finished_date,
                app_version: env!("CARGO_PKG_VERSION").to_owned(),
                features: finished,
            });
            self.unsent_reports.sort_by(|a, b| a.date.cmp(&b.date));
            let overflow = self.unsent_reports.len().saturating_sub(MAX_UNSENT_DAYS);
            self.unsent_reports.drain(..overflow);
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
    app_version: String,
    features: BTreeMap<String, u16>,
}

#[cfg(test)]
impl DailyPayload {
    pub fn date(&self) -> &str {
        &self.date
    }
}

#[derive(Clone, Debug)]
pub enum AnalyticsResult {
    /// The daily report for this local date was stored by the server.
    ReportDelivered(String),
    ReportFailed,
    DeletionDelivered(Uuid),
    DeletionFailed,
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
                            match agent.post(format!("{base}/v1/daily")).send_json(&payload) {
                                Ok(_) => true,
                                Err(error) => {
                                    eprintln!(
                                        "Analytics report for {} failed: {error}",
                                        payload.date
                                    );
                                    false
                                }
                            }
                        });
                        if delivered {
                            AnalyticsResult::ReportDelivered(payload.date)
                        } else {
                            AnalyticsResult::ReportFailed
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
                            AnalyticsResult::DeletionDelivered(installation_id)
                        } else {
                            AnalyticsResult::DeletionFailed
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

    #[test]
    fn finished_days_are_kept_until_the_server_confirms_them() {
        let mut settings = AnalyticsSettings::default();
        settings.enable();
        settings.counter_date = "2026-01-01".to_owned();
        settings.feature_counts.insert("note_created".to_owned(), 3);

        settings.record(AnalyticsFeature::SearchUsed);
        assert_eq!(settings.unsent_reports.len(), 1);
        assert_eq!(settings.unsent_reports[0].features["note_created"], 3);
        assert_eq!(settings.feature_counts["search_used"], 1);

        let first = settings.next_payload().expect("payload");
        assert_eq!(first.date(), "2026-01-01");
        assert!(!settings.confirm_delivered("2026-01-01"));
        let today = settings.next_payload().expect("payload");
        assert_eq!(today.date(), local_date());
    }

    #[test]
    fn unsent_days_are_bounded_and_cleared_on_opt_out() {
        let mut settings = AnalyticsSettings::default();
        settings.enable();
        for day in 1..=10 {
            settings.counter_date = format!("2026-01-{day:02}");
            settings.feature_counts.insert("graph_opened".to_owned(), 1);
            settings.record(AnalyticsFeature::GraphOpened);
            settings.feature_counts.clear();
        }
        assert_eq!(settings.unsent_reports.len(), MAX_UNSENT_DAYS);
        assert_eq!(settings.unsent_reports[0].date, "2026-01-04");

        settings.disable_and_queue_deletion();
        assert!(settings.unsent_reports.is_empty());
    }
}
