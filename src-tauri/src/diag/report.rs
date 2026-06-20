//! Privacy-safe diagnostics report for the most recent dictation session
//! and current app state.
//!
//! The report is intended for support and debugging. It must never contain
//! raw transcripts, raw audio, API keys, clipboard contents, or any other
//! sensitive material. Every field on `DiagnosticsReport` and its sub-types
//! is hand-picked to be safe to share.
//!
//! Collection is best-effort: a failure to gather any single field is
//! recorded as a stage error rather than aborting the whole report.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::providers::cleanup::RateLimitMetadata;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_NAME: &str = "Floe";

/// Stable report schema version. Bump on any breaking field change.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// Top-level diagnostics report. Always serializable, even when stages failed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DiagnosticsReport {
    pub schema_version: u32,
    pub app: &'static str,
    pub app_version: &'static str,
    pub generated_at: String,
    pub environment: &'static str,
    pub platform: PlatformInfo,
    pub hotkey: HotkeySnapshot,
    pub settings: SettingsSnapshot,
    pub last_session: LastSession,
    pub last_error: Option<LastError>,
    pub state_flags: StateFlags,
    pub provider_state: ProviderState,
    pub event_timeline: Vec<TimelineEvent>,
}

/// Static platform information safe to expose.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub family: String,
    pub tauri_version: Option<String>,
    pub os_version: Option<String>,
    pub cpu_model: Option<String>,
    pub cpu_logical_cores: Option<u32>,
    pub memory_total_mb: Option<u64>,
    pub memory_available_mb: Option<u64>,
    pub process_memory_mb: Option<u64>,
    pub uptime_secs: Option<u64>,
}

/// Hotkey configuration and registration state.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct HotkeySnapshot {
    pub accelerator: String,
    pub label: String,
    pub is_default: bool,
    pub is_registered: bool,
    pub error: Option<String>,
}

/// Non-secret application settings.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct SettingsSnapshot {
    pub api_key_configured: bool,
    pub api_key_masked_preview: Option<String>,
    pub start_at_login_enabled: Option<bool>,
    pub start_at_login_available: Option<bool>,
    pub keyring_migrated: bool,
    pub feature_flags: BTreeMap<String, bool>,
}

/// Provider availability state (no health checks, best-effort only).
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ProviderState {
    pub configured: bool,
    pub available: bool,
}

/// Last dictation session summary, plus per-stage details.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct LastSession {
    pub has_session: bool,
    pub trace_id: Option<String>,
    pub completed: bool,
    pub stage_summary: StageSummary,
    pub stages: SessionStages,
    pub audio: Option<AudioSnapshot>,
    pub stt_provider: Option<SttProviderSnapshot>,
    pub recovery_actions: Vec<RecoveryAction>,
    pub rate_limit: Option<RateLimitSnapshot>,
    pub retries: RetrySnapshot,
    pub pipeline_total_ms: Option<u64>,
    pub recording_started_at_unix_ms: Option<u64>,
    pub recording_ended_at_unix_ms: Option<u64>,
    pub detailed_timeline: Vec<DetailedEvent>,
    /// Character and word counts for the cleaned-up transcript, when available.
    pub cleanup_chars: Option<u32>,
    pub cleanup_words: Option<u32>,
}

/// Per-stage duration + status. Skipped stages still appear with status=skipped.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct SessionStages {
    pub hotkey_to_recording_start: StageRecord,
    pub recording_setup: StageRecord,
    pub audio_capture: StageRecord,
    pub buffering_to_encode: StageRecord,
    pub audio_encode: StageRecord,
    pub transcription: StageRecord,
    pub cleanup: StageRecord,
    pub cleanup_validation: StageRecord,
    pub clipboard_write: StageRecord,
    pub paste: StageRecord,
}

/// Per-stage summary flags (no per-stage duration details).
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct StageSummary {
    pub hotkey_ok: bool,
    pub recording_ok: bool,
    pub transcription_ok: bool,
    pub cleanup_ok: bool,
    pub cleanup_fallback_used: bool,
    pub clipboard_ok: bool,
    pub paste_ok: bool,
    pub copied_only: bool,
    pub error_stage: Option<String>,
    pub sanitized_error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StageRecord {
    pub status: StageStatus,
    pub duration_ms: u64,
    pub attempts: u32,
    pub model: Option<String>,
    pub error_code: Option<String>,
    pub skipped_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Succeeded,
    Failed,
    Skipped,
    NotRun,
    Cancelled,
    Timeout,
}

impl Default for StageRecord {
    fn default() -> Self {
        Self {
            status: StageStatus::NotRun,
            duration_ms: 0,
            attempts: 0,
            model: None,
            error_code: None,
            skipped_reason: None,
        }
    }
}

#[allow(dead_code)]
impl StageRecord {
    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            status: StageStatus::Skipped,
            duration_ms: 0,
            attempts: 0,
            model: None,
            error_code: None,
            skipped_reason: Some(reason.into()),
        }
    }

    pub fn not_run() -> Self {
        Self::default()
    }

    fn succeeded(duration_ms: u64, attempts: u32, model: Option<String>) -> Self {
        Self {
            status: StageStatus::Succeeded,
            duration_ms,
            attempts,
            model,
            error_code: None,
            skipped_reason: None,
        }
    }

    fn failed(duration_ms: u64, attempts: u32, error_code: Option<String>) -> Self {
        Self {
            status: StageStatus::Failed,
            duration_ms,
            attempts,
            model: None,
            error_code,
            skipped_reason: None,
        }
    }

    fn timed_out(duration_ms: u64, attempts: u32, error_code: Option<String>) -> Self {
        Self {
            status: StageStatus::Timeout,
            duration_ms,
            attempts,
            model: None,
            error_code,
            skipped_reason: None,
        }
    }

    fn cancelled(duration_ms: u64, reason: Option<String>) -> Self {
        Self {
            status: StageStatus::Cancelled,
            duration_ms,
            attempts: 0,
            model: None,
            error_code: None,
            skipped_reason: reason,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AudioSnapshot {
    pub format: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub bytes: u64,
    pub duration_ms: u64,
    pub ended_reason: String,
    pub max_duration_reached: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SttProviderSnapshot {
    pub provider_name: String,
    pub model: String,
    pub audio_duration_ms: u64,
    pub transcription_ms: u64,
    pub realtime_factor: f64,
    pub fallback_used: bool,
    pub transcript_chars: Option<u32>,
    pub transcript_words: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RateLimitSnapshot {
    pub stt: Option<BTreeMap<String, String>>,
    pub cleanup: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RetrySnapshot {
    pub stt: u32,
    pub cleanup: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RecoveryAction {
    pub stage: String,
    pub action: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LastError {
    pub stage: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct StateFlags {
    pub api_key_configured: bool,
    pub hotkey_registered: bool,
    pub recording_active: bool,
    pub processing_active: bool,
    pub background_launch: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TimelineEvent {
    pub stage: String,
    pub status: StageStatus,
    pub duration_ms: u64,
    pub attempts: u32,
    pub error_code: Option<String>,
}

/// A detailed chronological event recorded during pipeline execution.
/// These provide higher fidelity than the stage-based summary — retries,
/// fallback activations, and intermediate attempts are preserved.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DetailedEvent {
    pub stage: String,
    pub event_type: String,
    pub duration_ms: u64,
    pub error_code: Option<String>,
    pub retry_count: Option<u32>,
}

/// Snapshot of one Floe dictation session, fed to the report builder.
///
/// Every field is optional so the snapshot can be assembled even when only
/// partial information is available (e.g. STT failed before producing a
/// model name).
#[derive(Debug, Clone, Default)]
pub struct SessionSnapshot {
    pub trace_id: Option<String>,
    pub completed: bool,
    pub hotkey_to_recording_start_ms: u64,
    pub recording_setup_ms: u64,
    pub audio_capture_ms: u64,
    pub buffering_to_encode_ms: u64,
    pub audio_encode_ms: u64,
    pub transcription_ms: u64,
    pub transcription_attempts: u32,
    pub stt_model: Option<String>,
    pub transcription_error_code: Option<String>,
    pub cleanup_ms: u64,
    pub cleanup_validation_ms: u64,
    pub cleanup_attempts: u32,
    pub cleanup_model: Option<String>,
    pub cleanup_error_code: Option<String>,
    pub cleanup_fallback_used: bool,
    pub cleanup_chars: Option<u32>,
    pub cleanup_words: Option<u32>,
    pub clipboard_ms: u64,
    pub clipboard_error_code: Option<String>,
    pub paste_ms: u64,
    pub paste_error_code: Option<String>,
    pub pipeline_total_ms: Option<u64>,
    pub audio: Option<AudioSnapshot>,
    pub stt_provider: Option<SttProviderSnapshot>,
    pub rate_limit: Option<RateLimitSnapshot>,
    pub retries: RetrySnapshot,
    pub recovery_actions: Vec<RecoveryAction>,
    pub error_stage: Option<String>,
    pub sanitized_error_code: Option<String>,
    pub last_error: Option<LastError>,
    pub recording_started_at_unix_ms: Option<u64>,
    pub recording_ended_at_unix_ms: Option<u64>,
    #[allow(dead_code)]
    pub detailed_timeline: Vec<DetailedEvent>,
}

/// Top-level inputs to `DiagnosticsReport::build`. Designed so that tests
/// can construct a report from plain values without needing live managers.
#[derive(Debug, Clone, Default)]
pub struct ReportInputs {
    pub platform: PlatformInfo,
    pub hotkey: HotkeySnapshot,
    pub settings: SettingsSnapshot,
    pub session: SessionSnapshot,
    pub background_launch: bool,
    pub recording_active: bool,
    pub provider_available: bool,
}

impl DiagnosticsReport {
    /// Build a complete report. The function is deterministic in the
    /// snapshot it receives; collection failures (e.g. managers unavailable)
    /// should be reflected in the inputs, not in this builder.
    ///
    /// Free-form string fields are defensively redacted: any value that
    /// looks like a secret (API key, bearer token, raw transcript, raw audio,
    /// clipboard contents, etc.) is replaced with `"redacted"` before the
    /// report is returned.
    pub fn build(inputs: ReportInputs) -> DiagnosticsReport {
        let generated_at = current_iso8601_utc();

        let mut session = inputs.session.clone();
        redact_session(&mut session);

        let stages = build_session_stages(&session);
        let stage_summary = build_stage_summary(&session);
        let pipeline_total_ms = compute_pipeline_total_ms(&session);

        let trace_id = session.trace_id.clone();

        let last_session = LastSession {
            has_session: session_ran(&session),
            trace_id,
            completed: session.completed,
            stage_summary,
            stages,
            audio: session.audio.clone(),
            stt_provider: session.stt_provider.clone(),
            recovery_actions: session
                .recovery_actions
                .iter()
                .map(|action| RecoveryAction {
                    stage: redact_string(&action.stage),
                    action: redact_string(&action.action),
                    reason: redact_string(&action.reason),
                })
                .collect(),
            rate_limit: session.rate_limit.clone(),
            retries: session.retries.clone(),
            pipeline_total_ms: Some(pipeline_total_ms),
            recording_started_at_unix_ms: session.recording_started_at_unix_ms,
            recording_ended_at_unix_ms: session.recording_ended_at_unix_ms,
            detailed_timeline: build_detailed_timeline(&session),
            cleanup_chars: session.cleanup_chars,
            cleanup_words: session.cleanup_words,
        };

        let last_error = session.last_error.clone().or_else(|| {
            session.sanitized_error_code.as_ref().map(|code| LastError {
                stage: session
                    .error_stage
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                code: redact_string(code),
                message: String::new(),
            })
        });
        let last_error = last_error.map(|mut err| {
            err.code = redact_string(&err.code);
            err.message = redact_string(&err.message);
            err.stage = redact_string(&err.stage);
            err
        });

        let processing_active = inputs.recording_active
            || session.error_stage.is_some()
            || (!session.completed
                && (session.transcription_ms > 0
                    || session.cleanup_ms > 0
                    || session.clipboard_ms > 0
                    || session.paste_ms > 0));

        let mut settings = inputs.settings;
        settings.api_key_masked_preview = settings
            .api_key_masked_preview
            .map(|preview| redact_string(&preview));

        let hotkey = inputs.hotkey;

        let state_flags = StateFlags {
            api_key_configured: settings.api_key_configured,
            hotkey_registered: hotkey.is_registered,
            recording_active: inputs.recording_active,
            processing_active,
            background_launch: inputs.background_launch,
        };

        let provider_state = ProviderState {
            configured: settings.api_key_configured,
            available: inputs.provider_available,
        };

        let event_timeline = build_event_timeline(&last_session.stages);

        DiagnosticsReport {
            schema_version: REPORT_SCHEMA_VERSION,
            app: APP_NAME,
            app_version: APP_VERSION,
            environment: if cfg!(debug_assertions) {
                "development"
            } else {
                "production"
            },
            generated_at,
            platform: inputs.platform,
            hotkey,
            settings,
            provider_state,
            last_session,
            last_error,
            state_flags,
            event_timeline,
        }
    }
}

fn redact_session(session: &mut SessionSnapshot) {
    session.error_stage = session.error_stage.as_ref().map(|s| redact_string(s));
    session.sanitized_error_code = session
        .sanitized_error_code
        .as_ref()
        .map(|s| redact_string(s));
    session.transcription_error_code = session
        .transcription_error_code
        .as_ref()
        .map(|s| redact_string(s));
    session.cleanup_error_code = session
        .cleanup_error_code
        .as_ref()
        .map(|s| redact_string(s));
    session.clipboard_error_code = session
        .clipboard_error_code
        .as_ref()
        .map(|s| redact_string(s));
    session.paste_error_code = session.paste_error_code.as_ref().map(|s| redact_string(s));
    if let Some(mut last_error) = session.last_error.clone() {
        last_error.code = redact_string(&last_error.code);
        last_error.message = redact_string(&last_error.message);
        last_error.stage = redact_string(&last_error.stage);
        session.last_error = Some(last_error);
    }
}

fn session_ran(snapshot: &SessionSnapshot) -> bool {
    snapshot.audio.is_some()
        || snapshot.transcription_ms > 0
        || snapshot.cleanup_ms > 0
        || snapshot.error_stage.is_some()
}

fn compute_pipeline_total_ms(snapshot: &SessionSnapshot) -> u64 {
    if let Some(total) = snapshot.pipeline_total_ms {
        if total > 0 {
            return total;
        }
    }
    snapshot.hotkey_to_recording_start_ms
        + snapshot.recording_setup_ms
        + snapshot.audio_capture_ms
        + snapshot.audio_encode_ms
        + snapshot.transcription_ms
        + snapshot.cleanup_ms
        + snapshot.cleanup_validation_ms
        + snapshot.clipboard_ms
        + snapshot.paste_ms
}

fn build_session_stages(snapshot: &SessionSnapshot) -> SessionStages {
    SessionStages {
        hotkey_to_recording_start: stage_record(
            snapshot.hotkey_to_recording_start_ms,
            1,
            None,
            None,
        ),
        recording_setup: stage_record(snapshot.recording_setup_ms, 1, None, None),
        audio_capture: stage_record(snapshot.audio_capture_ms, 1, None, None),
        buffering_to_encode: stage_record(snapshot.buffering_to_encode_ms, 1, None, None),
        audio_encode: stage_record(snapshot.audio_encode_ms, 1, None, None),
        transcription: transcription_stage_record(snapshot),
        cleanup: cleanup_stage_record(snapshot),
        cleanup_validation: stage_record(snapshot.cleanup_validation_ms, 1, None, None),
        clipboard_write: clipboard_stage_record(snapshot),
        paste: paste_stage_record(snapshot),
    }
}

fn stage_record(
    duration_ms: u64,
    attempts: u32,
    model: Option<String>,
    error_code: Option<String>,
) -> StageRecord {
    if error_code.is_some() {
        StageRecord::failed(duration_ms, attempts, error_code)
    } else if duration_ms == 0 {
        StageRecord::not_run()
    } else {
        StageRecord::succeeded(duration_ms, attempts, model)
    }
}

fn transcription_stage_record(snapshot: &SessionSnapshot) -> StageRecord {
    let attempts = attempts_for(
        snapshot.transcription_attempts,
        snapshot.transcription_ms,
        snapshot.transcription_error_code.as_ref(),
    );
    if let Some(code) = snapshot.transcription_error_code.as_ref() {
        let code_lower = code.to_ascii_lowercase();
        if code_lower == "timeout" || code_lower.contains("timeout") {
            return StageRecord::timed_out(snapshot.transcription_ms, attempts, Some(code.clone()));
        }
        if code_lower == "cancelled" || code_lower.contains("cancelled") {
            return StageRecord::cancelled(snapshot.transcription_ms, Some(code.clone()));
        }
        return StageRecord::failed(snapshot.transcription_ms, attempts, Some(code.clone()));
    }
    if snapshot.transcription_ms == 0 && snapshot.stt_model.is_none() {
        return StageRecord::not_run();
    }
    StageRecord::succeeded(
        snapshot.transcription_ms,
        attempts,
        snapshot.stt_model.clone(),
    )
}

fn cleanup_stage_record(snapshot: &SessionSnapshot) -> StageRecord {
    let attempts = attempts_for(
        snapshot.cleanup_attempts,
        snapshot.cleanup_ms,
        snapshot.cleanup_error_code.as_ref(),
    );
    if let Some(code) = snapshot.cleanup_error_code.as_ref() {
        let code_lower = code.to_ascii_lowercase();
        if code_lower == "timeout" || code_lower.contains("timeout") {
            return StageRecord::timed_out(snapshot.cleanup_ms, attempts, Some(code.clone()));
        }
        if code_lower == "cancelled" || code_lower.contains("cancelled") {
            return StageRecord::cancelled(snapshot.cleanup_ms, Some(code.clone()));
        }
        return StageRecord::failed(snapshot.cleanup_ms, attempts, Some(code.clone()));
    }
    if snapshot.cleanup_ms == 0
        && snapshot.cleanup_validation_ms == 0
        && snapshot.cleanup_model.is_none()
    {
        return StageRecord::not_run();
    }
    StageRecord::succeeded(
        snapshot.cleanup_ms,
        attempts,
        snapshot.cleanup_model.clone(),
    )
}

fn clipboard_stage_record(snapshot: &SessionSnapshot) -> StageRecord {
    if let Some(code) = snapshot.clipboard_error_code.as_ref() {
        return StageRecord::failed(snapshot.clipboard_ms, 1, Some(code.clone()));
    }
    if snapshot.clipboard_ms == 0 {
        return StageRecord::not_run();
    }
    StageRecord::succeeded(snapshot.clipboard_ms, 1, None)
}

fn paste_stage_record(snapshot: &SessionSnapshot) -> StageRecord {
    if let Some(code) = snapshot.paste_error_code.as_ref() {
        return StageRecord::failed(snapshot.paste_ms, 1, Some(code.clone()));
    }
    if snapshot.paste_ms == 0 {
        return StageRecord::not_run();
    }
    StageRecord::succeeded(snapshot.paste_ms, 1, None)
}

fn attempts_for(explicit: u32, duration_ms: u64, error_code: Option<&String>) -> u32 {
    if explicit > 0 {
        return explicit;
    }
    if duration_ms > 0 || error_code.is_some() {
        1
    } else {
        0
    }
}

fn build_stage_summary(snapshot: &SessionSnapshot) -> StageSummary {
    StageSummary {
        hotkey_ok: snapshot.hotkey_to_recording_start_ms > 0 || snapshot.audio.is_some(),
        recording_ok: snapshot.audio.is_some() && snapshot.transcription_error_code.is_none(),
        transcription_ok: snapshot.transcription_ms > 0
            && snapshot.transcription_error_code.is_none(),
        cleanup_ok: snapshot.cleanup_ms > 0
            && snapshot.cleanup_error_code.is_none()
            && !snapshot.cleanup_fallback_used,
        cleanup_fallback_used: snapshot.cleanup_fallback_used,
        clipboard_ok: snapshot.clipboard_ms > 0 && snapshot.clipboard_error_code.is_none(),
        paste_ok: snapshot.paste_ms > 0 && snapshot.paste_error_code.is_none(),
        copied_only: snapshot.clipboard_ms > 0
            && snapshot.paste_error_code.is_some()
            && snapshot.clipboard_error_code.is_none(),
        error_stage: snapshot.error_stage.clone(),
        sanitized_error_code: snapshot.sanitized_error_code.clone(),
    }
}

fn build_event_timeline(stages: &SessionStages) -> Vec<TimelineEvent> {
    let entries: [(&str, &StageRecord); 10] = [
        (
            "hotkey_to_recording_start",
            &stages.hotkey_to_recording_start,
        ),
        ("recording_setup", &stages.recording_setup),
        ("audio_capture", &stages.audio_capture),
        ("buffering_to_encode", &stages.buffering_to_encode),
        ("audio_encode", &stages.audio_encode),
        ("transcription", &stages.transcription),
        ("cleanup", &stages.cleanup),
        ("cleanup_validation", &stages.cleanup_validation),
        ("clipboard_write", &stages.clipboard_write),
        ("paste", &stages.paste),
    ];

    entries
        .into_iter()
        .filter(|(_, record)| record.status != StageStatus::NotRun)
        .map(|(stage, record)| TimelineEvent {
            stage: stage.to_string(),
            status: record.status,
            duration_ms: record.duration_ms,
            attempts: record.attempts,
            error_code: record.error_code.clone(),
        })
        .collect()
}

fn build_detailed_timeline(snapshot: &SessionSnapshot) -> Vec<DetailedEvent> {
    let mut events: Vec<DetailedEvent> = Vec::new();

    if snapshot.hotkey_to_recording_start_ms > 0 {
        events.push(DetailedEvent {
            stage: "hotkey".into(),
            event_type: "pressed".into(),
            duration_ms: snapshot.hotkey_to_recording_start_ms,
            error_code: None,
            retry_count: None,
        });
    }

    if snapshot.recording_setup_ms > 0 {
        events.push(DetailedEvent {
            stage: "recording".into(),
            event_type: "started".into(),
            duration_ms: snapshot.recording_setup_ms,
            error_code: None,
            retry_count: None,
        });
    }

    if snapshot.transcription_ms > 0 || snapshot.transcription_error_code.is_some() {
        let event_type = if snapshot.transcription_error_code.is_some() {
            match snapshot.transcription_error_code.as_deref() {
                Some(c) if c.to_ascii_lowercase().contains("timeout") => "timeout",
                Some(c) if c.to_ascii_lowercase().contains("cancelled") => "cancelled",
                Some(_) => "failed",
                None => "completed",
            }
        } else {
            "completed"
        };

        events.push(DetailedEvent {
            stage: "transcription".into(),
            event_type: event_type.into(),
            duration_ms: snapshot.transcription_ms,
            error_code: snapshot.transcription_error_code.clone(),
            retry_count: Some(snapshot.transcription_attempts.max(1)),
        });
    }

    if snapshot.cleanup_fallback_used {
        events.push(DetailedEvent {
            stage: "cleanup".into(),
            event_type: "fallback".into(),
            duration_ms: snapshot.cleanup_ms,
            error_code: snapshot.cleanup_error_code.clone(),
            retry_count: Some(snapshot.cleanup_attempts.max(1)),
        });
    } else if snapshot.cleanup_ms > 0 || snapshot.cleanup_error_code.is_some() {
        let event_type = if snapshot.cleanup_error_code.is_some() {
            match snapshot.cleanup_error_code.as_deref() {
                Some(c) if c.to_ascii_lowercase().contains("timeout") => "timeout",
                Some(c) if c.to_ascii_lowercase().contains("cancelled") => "cancelled",
                Some(_) => "failed",
                None => "completed",
            }
        } else {
            "completed"
        };

        events.push(DetailedEvent {
            stage: "cleanup".into(),
            event_type: event_type.into(),
            duration_ms: snapshot.cleanup_ms,
            error_code: snapshot.cleanup_error_code.clone(),
            retry_count: Some(snapshot.cleanup_attempts.max(1)),
        });
    }

    if snapshot.clipboard_ms > 0 || snapshot.clipboard_error_code.is_some() {
        events.push(DetailedEvent {
            stage: "clipboard".into(),
            event_type: if snapshot.clipboard_error_code.is_some() {
                "failed"
            } else {
                "completed"
            }
            .into(),
            duration_ms: snapshot.clipboard_ms,
            error_code: snapshot.clipboard_error_code.clone(),
            retry_count: None,
        });
    }

    if snapshot.paste_ms > 0 || snapshot.paste_error_code.is_some() {
        events.push(DetailedEvent {
            stage: "paste".into(),
            event_type: if snapshot.paste_error_code.is_some() {
                "failed"
            } else {
                "completed"
            }
            .into(),
            duration_ms: snapshot.paste_ms,
            error_code: snapshot.paste_error_code.clone(),
            retry_count: None,
        });
    }

    if snapshot.cleanup_fallback_used {
        events.push(DetailedEvent {
            stage: "cleanup".into(),
            event_type: "completed".into(),
            duration_ms: snapshot.cleanup_ms,
            error_code: None,
            retry_count: Some(snapshot.cleanup_attempts.max(1)),
        });
    }

    events
}

/// Shared list of substrings that indicate a value may contain secrets.
/// Used by both `redact_string` and `sanitize_error_for_log` so that
/// any new marker added here is automatically checked everywhere.
pub const SECRET_MARKERS: &[&str] = &[
    "bearer ",
    "authorization",
    "api_key",
    "api-key",
    "apikey",
    "gsk_",
    "sk-",
    "sk_",
    "clipboard_text",
    "transcript",
    "raw_audio",
    "audio_bytes",
];

/// Returns `true` when `value` contains any of the known secret markers
/// (case-insensitive ASCII comparison). Shared by both Rust redaction paths.
pub fn contains_secret_marker(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    SECRET_MARKERS.iter().any(|m| lowered.contains(m))
}

/// Redact free-form text fields before they enter the report.
/// Returns the original string when it is safe; otherwise returns `"redacted"`.
#[allow(dead_code)]
pub fn redact_string_for_report(value: &str) -> String {
    redact_string(value)
}

/// Convert RateLimitMetadata into a BTreeMap for diagnostics RateLimitSnapshot.
pub fn rate_limit_to_map(rl: &Box<RateLimitMetadata>) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    if let Some(v) = &rl.remaining_requests {
        map.insert("remaining_requests".to_string(), v.clone());
    }
    if let Some(v) = &rl.remaining_tokens {
        map.insert("remaining_tokens".to_string(), v.clone());
    }
    if let Some(v) = &rl.reset_requests {
        map.insert("reset_requests".to_string(), v.clone());
    }
    if let Some(v) = &rl.reset_tokens {
        map.insert("reset_tokens".to_string(), v.clone());
    }
    if let Some(v) = rl.retry_after_seconds {
        map.insert("retry_after_seconds".to_string(), v.to_string());
    }
    map
}

fn redact_string(value: &str) -> String {
    let lowered = value.to_ascii_lowercase();

    // Preserve already-masked previews (contain ellipsis or multiple asterisks)
    // These are safe to show and help identify the provider type during debugging.
    if lowered.contains("...") || lowered.contains("…") || lowered.contains("****") {
        return value.to_string();
    }

    if contains_secret_marker(value) {
        return "redacted".to_string();
    }
    value.to_string()
}

fn current_iso8601_utc() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format_iso8601(d.as_secs(), d.subsec_millis())
}

pub fn format_iso8601(secs: u64, millis: u32) -> String {
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let month_days = if is_leap(y) {
        &LEAP_MONTH_DAYS[..]
    } else {
        &NORMAL_MONTH_DAYS[..]
    };
    let mut m = 1;
    for &md in month_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        m += 1;
    }
    let d = remaining + 1;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y, m, d, hours, minutes, seconds, millis
    )
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

const NORMAL_MONTH_DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
const LEAP_MONTH_DAYS: [i64; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

#[cfg(test)]
mod tests {
    use super::*;

    fn default_inputs() -> ReportInputs {
        ReportInputs {
            platform: PlatformInfo {
                os: "macos".to_string(),
                arch: "aarch64".to_string(),
                family: "unix".to_string(),
                tauri_version: None,
                os_version: None,
                cpu_model: None,
                cpu_logical_cores: None,
                memory_total_mb: None,
                memory_available_mb: None,
                process_memory_mb: None,
                uptime_secs: None,
            },
            hotkey: HotkeySnapshot {
                accelerator: "Alt+Space".to_string(),
                label: "Option + Space".to_string(),
                is_default: true,
                is_registered: true,
                error: None,
            },
            settings: SettingsSnapshot {
                api_key_configured: true,
                api_key_masked_preview: Some("gsk_…****".to_string()),
                start_at_login_enabled: Some(false),
                start_at_login_available: Some(true),
                keyring_migrated: true,
                feature_flags: BTreeMap::new(),
            },
            session: SessionSnapshot::default(),
            background_launch: false,
            recording_active: false,
            provider_available: false,
        }
    }

    #[test]
    fn empty_snapshot_produces_full_report() {
        let report = DiagnosticsReport::build(default_inputs());
        assert_eq!(report.schema_version, REPORT_SCHEMA_VERSION);
        assert_eq!(report.app, "Floe");
        assert_eq!(
            report.last_session.stages.transcription.status,
            StageStatus::NotRun
        );
        assert_eq!(
            report.last_session.stages.cleanup.status,
            StageStatus::NotRun
        );
        assert!(!report.last_session.stage_summary.transcription_ok);
        assert!(!report.last_session.stage_summary.cleanup_ok);
        assert!(report.last_session.stage_summary.error_stage.is_none());
        assert!(report
            .last_session
            .stage_summary
            .sanitized_error_code
            .is_none());
        assert!(report.event_timeline.is_empty());
    }

    #[test]
    fn successful_session_records_durations_and_models() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            trace_id: Some("abcd1234".into()),
            completed: true,
            hotkey_to_recording_start_ms: 25,
            audio_capture_ms: 1_500,
            buffering_to_encode_ms: 5,
            audio_encode_ms: 7,
            transcription_ms: 800,
            transcription_attempts: 1,
            stt_model: Some("whisper-large-v3-turbo".into()),
            cleanup_ms: 300,
            cleanup_validation_ms: 12,
            cleanup_model: Some("llama-3.3-70b-versatile".into()),
            clipboard_ms: 8,
            paste_ms: 80,
            pipeline_total_ms: Some(2_737),
            retries: RetrySnapshot { stt: 1, cleanup: 0 },
            ..Default::default()
        };

        let report = DiagnosticsReport::build(inputs);
        let stages = &report.last_session.stages;
        assert_eq!(stages.transcription.status, StageStatus::Succeeded);
        assert_eq!(
            stages.transcription.model.as_deref(),
            Some("whisper-large-v3-turbo")
        );
        assert_eq!(stages.transcription.duration_ms, 800);
        assert_eq!(stages.cleanup.status, StageStatus::Succeeded);
        assert_eq!(stages.cleanup.duration_ms, 300);
        assert_eq!(stages.clipboard_write.status, StageStatus::Succeeded);
        assert_eq!(stages.paste.status, StageStatus::Succeeded);
        assert_eq!(stages.audio_capture.duration_ms, 1_500);
        assert!(report.last_session.stage_summary.transcription_ok);
        assert!(report.last_session.stage_summary.cleanup_ok);
        assert!(report.last_session.stage_summary.paste_ok);
        assert_eq!(report.last_session.retries.stt, 1);
    }

    #[test]
    fn transcription_failure_marks_stage_failed_and_others_skipped() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            transcription_ms: 30_000,
            transcription_attempts: 3,
            transcription_error_code: Some("timeout".into()),
            error_stage: Some("stt".into()),
            sanitized_error_code: Some("timeout".into()),
            ..Default::default()
        };

        let report = DiagnosticsReport::build(inputs);
        assert_eq!(
            report.last_session.stages.transcription.status,
            StageStatus::Timeout
        );
        assert_eq!(
            report
                .last_session
                .stages
                .transcription
                .error_code
                .as_deref(),
            Some("timeout")
        );
        assert_eq!(
            report.last_session.stages.cleanup.status,
            StageStatus::NotRun
        );
        assert_eq!(
            report.last_session.stages.clipboard_write.status,
            StageStatus::NotRun
        );
        assert_eq!(report.last_session.stages.paste.status, StageStatus::NotRun);
        assert_eq!(
            report.last_session.stage_summary.error_stage.as_deref(),
            Some("stt")
        );
        assert!(!report.last_session.stage_summary.transcription_ok);
        assert!(report.last_error.is_some());
        assert_eq!(report.last_error.as_ref().unwrap().stage, "stt");
    }

    #[test]
    fn transcription_timeout_status() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            transcription_ms: 30_000,
            transcription_error_code: Some("timeout".into()),
            ..Default::default()
        };

        let report = DiagnosticsReport::build(inputs);
        assert_eq!(
            report.last_session.stages.transcription.status,
            StageStatus::Timeout
        );
    }

    #[test]
    fn transcription_cancelled_status() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            transcription_ms: 10_000,
            transcription_error_code: Some("cancelled".into()),
            ..Default::default()
        };

        let report = DiagnosticsReport::build(inputs);
        assert_eq!(
            report.last_session.stages.transcription.status,
            StageStatus::Cancelled
        );
    }

    #[test]
    fn cleanup_timeout_status() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            cleanup_ms: 15_000,
            cleanup_error_code: Some("timeout".into()),
            ..Default::default()
        };

        let report = DiagnosticsReport::build(inputs);
        assert_eq!(
            report.last_session.stages.cleanup.status,
            StageStatus::Timeout
        );
    }

    #[test]
    fn pipeline_total_is_aggregated_when_none() {
        let snapshot = SessionSnapshot {
            hotkey_to_recording_start_ms: 10,
            audio_capture_ms: 100,
            audio_encode_ms: 5,
            transcription_ms: 50,
            cleanup_ms: 20,
            cleanup_validation_ms: 2,
            clipboard_ms: 5,
            paste_ms: 8,
            pipeline_total_ms: None,
            ..Default::default()
        };
        assert_eq!(compute_pipeline_total_ms(&snapshot), 200);
    }

    #[test]
    fn pipeline_total_preserves_explicit_value() {
        let snapshot = SessionSnapshot {
            pipeline_total_ms: Some(999),
            ..Default::default()
        };
        assert_eq!(compute_pipeline_total_ms(&snapshot), 999);
    }

    #[test]
    fn pipeline_total_is_wrapped_in_some() {
        let report = DiagnosticsReport::build(default_inputs());
        assert_eq!(report.last_session.pipeline_total_ms, Some(0));
    }

    #[test]
    fn report_schema_version_is_stable() {
        assert_eq!(REPORT_SCHEMA_VERSION, 1);
    }

    #[test]
    fn timeline_skips_unstarted_stages() {
        let report = DiagnosticsReport::build(default_inputs());
        assert!(report.event_timeline.is_empty());
    }

    #[test]
    fn timeline_records_only_ran_stages() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            hotkey_to_recording_start_ms: 5,
            audio_capture_ms: 100,
            audio_encode_ms: 10,
            transcription_ms: 50,
            transcription_attempts: 1,
            stt_model: Some("whisper-large-v3-turbo".into()),
            ..Default::default()
        };
        let report = DiagnosticsReport::build(inputs);
        let names: Vec<&str> = report
            .event_timeline
            .iter()
            .map(|e| e.stage.as_str())
            .collect();
        assert!(names.contains(&"hotkey_to_recording_start"));
        assert!(names.contains(&"audio_capture"));
        assert!(names.contains(&"audio_encode"));
        assert!(names.contains(&"transcription"));
        assert!(!names.contains(&"cleanup"));
        assert!(!names.contains(&"paste"));
    }

    #[test]
    fn report_serializes_with_snake_case_keys_and_no_pii() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            trace_id: Some("deadbeef".into()),
            completed: true,
            transcription_ms: 200,
            transcription_attempts: 1,
            stt_model: Some("whisper".into()),
            cleanup_ms: 100,
            cleanup_model: Some("llama".into()),
            clipboard_ms: 5,
            paste_ms: 50,
            pipeline_total_ms: Some(1_000),
            retries: RetrySnapshot { stt: 0, cleanup: 0 },
            audio: Some(AudioSnapshot {
                format: "wav".into(),
                sample_rate: 16_000,
                channels: 1,
                bits_per_sample: 16,
                bytes: 32_000,
                duration_ms: 1_000,
                ended_reason: "manual".into(),
                max_duration_reached: false,
            }),
            ..Default::default()
        };

        let report = DiagnosticsReport::build(inputs);
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(value["schema_version"], REPORT_SCHEMA_VERSION);
        assert_eq!(value["app"], "Floe");
        assert_eq!(value["app_version"], APP_VERSION);
        assert!(value["platform"]["os"].is_string());
        assert!(value["platform"]["arch"].is_string());
        assert!(value["hotkey"]["accelerator"].is_string());
        assert_eq!(
            value["last_session"]["stages"]["transcription"]["status"],
            "succeeded"
        );
        assert_eq!(
            value["last_session"]["stages"]["transcription"]["model"],
            "whisper"
        );
        assert_eq!(value["last_session"]["retries"]["stt"], 0);
        assert!(value["event_timeline"].is_array());
    }

    #[test]
    fn report_never_carries_raw_transcript_or_keys() {
        let mut inputs = default_inputs();
        inputs.settings.api_key_masked_preview = Some("****...****".to_string());
        inputs.session = SessionSnapshot {
            trace_id: Some("abc123".into()),
            transcription_ms: 200,
            stt_model: Some("whisper-large-v3-turbo".into()),
            transcription_error_code: Some("bearer gsk_secretkey".into()),
            last_error: Some(LastError {
                stage: "stt".into(),
                code: "bearer gsk_secretkey".into(),
                message: "this is a private transcript with the api key gsk_abc".into(),
            }),
            sanitized_error_code: Some("timeout".into()),
            ..Default::default()
        };

        let json = serde_json::to_string(&DiagnosticsReport::build(inputs)).unwrap();
        let lowered = json.to_lowercase();
        for forbidden in [
            "bearer",
            "authorization",
            "raw_audio",
            "audio_bytes",
            "pcm_samples",
            "clipboard_text",
            "gsk_secretkey",
            "this is a private transcript",
        ] {
            assert!(
                !lowered.contains(forbidden),
                "report contains forbidden token '{forbidden}': {json}",
            );
        }
        assert!(json.contains("****...****"));
    }

    #[test]
    fn masked_preview_preserved_for_groq_and_openai() {
        let mut inputs = default_inputs();
        inputs.settings.api_key_masked_preview = Some("gsk_...abcd".to_string());
        let json = serde_json::to_string(&DiagnosticsReport::build(inputs.clone())).unwrap();
        assert!(json.contains("gsk_...abcd"));

        inputs.settings.api_key_masked_preview = Some("sk_...efgh".to_string());
        let json = serde_json::to_string(&DiagnosticsReport::build(inputs.clone())).unwrap();
        assert!(json.contains("sk_...efgh"));

        inputs.settings.api_key_masked_preview = Some("gsk_…****".to_string());
        let json = serde_json::to_string(&DiagnosticsReport::build(inputs.clone())).unwrap();
        assert!(json.contains("gsk_…****"));
    }

    #[test]
    fn redact_string_for_report_marks_secrets() {
        assert_eq!(
            redact_string_for_report("Authorization: Bearer gsk_abc123"),
            "redacted"
        );
        assert_eq!(redact_string_for_report("api_key=secret"), "redacted");
        assert_eq!(
            redact_string_for_report("clipboard_text: hello"),
            "redacted"
        );
        assert_eq!(redact_string_for_report("transcript raw text"), "redacted");
    }

    #[test]
    fn redact_string_for_report_keeps_safe_strings() {
        assert_eq!(redact_string_for_report("timeout_error"), "timeout_error");
        assert_eq!(redact_string_for_report("server_error"), "server_error");
        assert_eq!(redact_string_for_report("rate_limit"), "rate_limit");
    }

    #[test]
    fn cleanup_fallback_marks_stage_summary() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            cleanup_ms: 100,
            cleanup_fallback_used: true,
            cleanup_error_code: Some("server_error".into()),
            error_stage: Some("cleanup".into()),
            sanitized_error_code: Some("server_error".into()),
            ..Default::default()
        };

        let report = DiagnosticsReport::build(inputs);
        assert!(report.last_session.stage_summary.cleanup_fallback_used);
        assert!(!report.last_session.stage_summary.cleanup_ok);
        assert_eq!(
            report.last_session.stage_summary.error_stage.as_deref(),
            Some("cleanup")
        );
    }

    #[test]
    fn copied_only_summary_when_paste_fails_after_clipboard() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            clipboard_ms: 6,
            paste_ms: 12,
            paste_error_code: Some("pasteUnavailable".into()),
            ..Default::default()
        };
        let report = DiagnosticsReport::build(inputs);
        assert!(report.last_session.stage_summary.clipboard_ok);
        assert!(!report.last_session.stage_summary.paste_ok);
        assert!(report.last_session.stage_summary.copied_only);
    }

    #[test]
    fn stage_record_defaults_to_not_run() {
        let record = StageRecord::not_run();
        assert_eq!(record.status, StageStatus::NotRun);
        assert_eq!(record.duration_ms, 0);
        assert_eq!(record.attempts, 0);
    }

    #[test]
    fn stage_record_skipped_carries_reason() {
        let record = StageRecord::skipped("no recording present");
        assert_eq!(record.status, StageStatus::Skipped);
        assert_eq!(
            record.skipped_reason.as_deref(),
            Some("no recording present")
        );
    }

    #[test]
    fn stage_record_timed_out_and_cancelled() {
        let record = StageRecord::timed_out(5000, 3, Some("timeout".into()));
        assert_eq!(record.status, StageStatus::Timeout);
        assert_eq!(record.duration_ms, 5000);
        assert_eq!(record.attempts, 3);

        let record = StageRecord::cancelled(2000, Some("user cancelled".into()));
        assert_eq!(record.status, StageStatus::Cancelled);
        assert_eq!(record.duration_ms, 2000);
    }

    #[test]
    fn report_works_when_session_never_started() {
        let report = DiagnosticsReport::build(default_inputs());
        assert!(!report.last_session.has_session);
        assert!(report.last_session.trace_id.is_none());
        assert!(report.last_error.is_none());
        assert!(report.event_timeline.is_empty());
    }

    #[test]
    fn recording_active_flag_is_propagated() {
        let mut inputs = default_inputs();
        inputs.recording_active = true;
        let report = DiagnosticsReport::build(inputs);
        assert!(report.state_flags.recording_active);
    }

    #[test]
    fn iso8601_format_is_stable() {
        assert_eq!(format_iso8601(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            format_iso8601(1_700_000_000, 123),
            "2023-11-14T22:13:20.123Z"
        );
    }

    #[test]
    fn environment_is_development_in_tests() {
        let report = DiagnosticsReport::build(default_inputs());
        assert_eq!(report.environment, "development");
    }

    #[test]
    fn platform_sysinfo_fields_are_optional() {
        let report = DiagnosticsReport::build(default_inputs());
        assert!(report.platform.os_version.is_none());
        assert!(report.platform.cpu_model.is_none());
        assert!(report.platform.cpu_logical_cores.is_none());
        assert!(report.platform.memory_total_mb.is_none());
        assert!(report.platform.memory_available_mb.is_none());
        assert!(report.platform.process_memory_mb.is_none());
        assert!(report.platform.uptime_secs.is_none());
    }

    #[test]
    fn provider_state_reflects_configuration() {
        let mut inputs = default_inputs();
        inputs.provider_available = true;
        let report = DiagnosticsReport::build(inputs);
        assert!(report.provider_state.configured);
        assert!(report.provider_state.available);
    }

    #[test]
    fn feature_flags_is_empty_in_default() {
        let report = DiagnosticsReport::build(default_inputs());
        assert!(report.settings.feature_flags.is_empty());
    }

    #[test]
    fn detailed_timeline_includes_hotkey_and_recording() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            hotkey_to_recording_start_ms: 25,
            recording_setup_ms: 10,
            transcription_ms: 800,
            transcription_attempts: 1,
            stt_model: Some("whisper".into()),
            ..Default::default()
        };
        let report = DiagnosticsReport::build(inputs);
        let dt = &report.last_session.detailed_timeline;
        assert!(dt
            .iter()
            .any(|e| e.stage == "hotkey" && e.event_type == "pressed"));
        assert!(dt
            .iter()
            .any(|e| e.stage == "recording" && e.event_type == "started"));
        assert!(dt
            .iter()
            .any(|e| e.stage == "transcription" && e.event_type == "completed"));
    }

    #[test]
    fn detailed_timeline_handles_cleanup_fallback() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            hotkey_to_recording_start_ms: 10,
            audio_capture_ms: 500,
            audio_encode_ms: 5,
            transcription_ms: 400,
            transcription_attempts: 1,
            stt_model: Some("whisper".into()),
            cleanup_ms: 200,
            cleanup_fallback_used: true,
            cleanup_error_code: Some("server_error".into()),
            cleanup_attempts: 2,
            clipboard_ms: 5,
            paste_ms: 50,
            pipeline_total_ms: Some(1_172),
            ..Default::default()
        };
        let report = DiagnosticsReport::build(inputs);
        let dt = &report.last_session.detailed_timeline;
        assert!(dt
            .iter()
            .any(|e| e.stage == "cleanup" && e.event_type == "fallback"));
        assert!(dt
            .iter()
            .any(|e| e.stage == "cleanup" && e.event_type == "completed"));
    }

    #[test]
    fn stt_provider_snapshot_has_char_and_word_counts() {
        let snapshot = SessionSnapshot {
            stt_provider: Some(SttProviderSnapshot {
                provider_name: "groq".into(),
                model: "whisper".into(),
                audio_duration_ms: 1000,
                transcription_ms: 500,
                realtime_factor: 0.5,
                fallback_used: false,
                transcript_chars: Some(120),
                transcript_words: Some(20),
            }),
            ..Default::default()
        };
        let report = DiagnosticsReport::build(ReportInputs {
            session: snapshot,
            ..default_inputs()
        });
        let provider = report.last_session.stt_provider.unwrap();
        assert_eq!(provider.transcript_chars, Some(120));
        assert_eq!(provider.transcript_words, Some(20));
    }

    #[test]
    fn renamed_timestamp_fields_serialize_correctly() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            recording_started_at_unix_ms: Some(1_750_000_000_000),
            recording_ended_at_unix_ms: Some(1_750_000_002_500),
            ..Default::default()
        };
        let report = DiagnosticsReport::build(inputs);
        let json = serde_json::to_value(&report).unwrap();
        let ls = &json["last_session"];
        assert_eq!(
            ls["recording_started_at_unix_ms"].as_u64(),
            Some(1_750_000_000_000)
        );
        assert_eq!(
            ls["recording_ended_at_unix_ms"].as_u64(),
            Some(1_750_000_002_500)
        );
    }

    #[test]
    fn cleanup_chars_and_words_propagate_to_report() {
        let mut inputs = default_inputs();
        inputs.session = SessionSnapshot {
            cleanup_chars: Some(80),
            cleanup_words: Some(15),
            ..Default::default()
        };
        let report = DiagnosticsReport::build(inputs);
        assert_eq!(report.last_session.cleanup_chars, Some(80));
        assert_eq!(report.last_session.cleanup_words, Some(15));

        // Verify serialized form
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["last_session"]["cleanup_chars"].as_u64(), Some(80));
        assert_eq!(json["last_session"]["cleanup_words"].as_u64(), Some(15));
    }

    #[test]
    fn rate_limit_snapshot_propagates_stt_and_cleanup() {
        let mut inputs = default_inputs();
        let stt_map = {
            let mut m = BTreeMap::new();
            m.insert("remaining_requests".into(), "99".into());
            m.insert("remaining_tokens".into(), "5000".into());
            m
        };
        let cleanup_map = {
            let mut m = BTreeMap::new();
            m.insert("remaining_requests".into(), "50".into());
            m
        };

        inputs.session.rate_limit = Some(RateLimitSnapshot {
            stt: Some(stt_map),
            cleanup: Some(cleanup_map),
        });

        let report = DiagnosticsReport::build(inputs);
        let rl = report
            .last_session
            .rate_limit
            .as_ref()
            .expect("rate_limit should exist");

        assert_eq!(
            rl.stt.as_ref().unwrap().get("remaining_requests").unwrap(),
            "99"
        );
        assert_eq!(
            rl.stt.as_ref().unwrap().get("remaining_tokens").unwrap(),
            "5000"
        );
        assert_eq!(
            rl.cleanup
                .as_ref()
                .unwrap()
                .get("remaining_requests")
                .unwrap(),
            "50"
        );

        // Verify JSON serialization uses snake_case keys
        let json = serde_json::to_value(&report).unwrap();
        let rl_json = &json["last_session"]["rate_limit"];
        assert!(rl_json["stt"]["remaining_requests"].is_string());
        assert!(rl_json["cleanup"]["remaining_requests"].is_string());
    }

    #[test]
    fn recovery_actions_propagate_through_report() {
        let mut inputs = default_inputs();
        inputs.session.recovery_actions = vec![
            RecoveryAction {
                stage: "cleanup".into(),
                action: "fallback_to_raw_text".into(),
                reason: "cleanup_provider_failed: server_error".into(),
            },
            RecoveryAction {
                stage: "transcription".into(),
                action: "retry_succeeded".into(),
                reason: "stt_retried_2_times".into(),
            },
        ];

        let report = DiagnosticsReport::build(inputs);
        assert_eq!(report.last_session.recovery_actions.len(), 2);
        assert_eq!(
            report.last_session.recovery_actions[0].action,
            "fallback_to_raw_text"
        );
        assert_eq!(
            report.last_session.recovery_actions[1].action,
            "retry_succeeded"
        );
        assert_eq!(report.last_session.recovery_actions[0].stage, "cleanup");

        // Verify serialized form
        let json = serde_json::to_value(&report).unwrap();
        let actions = json["last_session"]["recovery_actions"].as_array().unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["action"], "fallback_to_raw_text");
        assert_eq!(actions[1]["action"], "retry_succeeded");
    }

    #[test]
    fn contains_secret_marker_catches_all_prefixes() {
        assert!(contains_secret_marker("Bearer gsk_abcdefgh"));
        assert!(contains_secret_marker("gsk_abcdef12"));
        assert!(contains_secret_marker("sk_abcdef12"));
        assert!(contains_secret_marker("sk-abcdef12"));
        assert!(contains_secret_marker("api_key=secret"));
        assert!(contains_secret_marker("api-key=secret"));
        assert!(contains_secret_marker("apikey=secret"));
        assert!(contains_secret_marker("clipboard_text data"));
        assert!(contains_secret_marker("transcript content"));
        assert!(!contains_secret_marker("timeout_error"));
        assert!(!contains_secret_marker("server_error"));
        assert!(!contains_secret_marker("rate_limit"));
    }
}
