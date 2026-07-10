use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, Runtime};

use std::collections::BTreeMap;

use crate::{
    diag::{
        DiagnosticsReport, LastSessionStore, PlatformInfo, ReportInputs, SessionSnapshot,
        SettingsSnapshot,
    },
    settings::SettingsManager,
    system::autostart::{get_start_at_login_status_with, TauriAutostartIntegration},
    system::hotkey::HotkeyManager,
};

/// Privacy-safe diagnostics logger.
/// Only logs the allowed diagnostic fields:
/// - provider_name
/// - model_name
/// - backend_type
/// - audio_duration_ms
/// - transcription_ms
/// - cleanup_ms
/// - realtime_factor
/// - fallback_used
/// - error_code
///
/// Does NOT log: raw transcript, raw audio, API keys, clipboard contents
#[derive(Debug)]
pub struct DiagLog {
    path: Mutex<Option<PathBuf>>,
}

impl DiagLog {
    pub fn new() -> Self {
        Self {
            path: Mutex::new(None),
        }
    }
    #[cfg(test)]
    pub fn set_path(&self, path: String) {
        if let Ok(mut guard) = self.path.lock() {
            if !path.is_empty() {
                *guard = Some(PathBuf::from(path));
            }
        }
    }

    /// Append a raw string line to the log file.
    /// Privacy responsibility lies with the caller — the line is written as-is.
    pub fn append_str(&self, line: &str) {
        if let Ok(guard) = self.path.lock() {
            if let Some(path) = guard.as_ref() {
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                    let _ = writeln!(file, "{}", line);
                }
            }
        }
    }
}

/// A privacy-safe diagnostic entry containing only approved fields.
/// This struct enforces at compile time that we cannot accidentally
/// include sensitive data in diagnostics.
#[cfg(test)]
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DiagEntry {
    pub provider_name: String,
    pub model_name: String,
    pub backend_type: String,
    pub audio_duration_ms: u64,
    pub transcription_ms: u64,
    pub cleanup_ms: u64,
    pub realtime_factor: f64,
    pub fallback_used: bool,
    pub fallback_provider: Option<String>,
    pub retry_count: u32,
    pub error_code: Option<String>,
}

#[cfg(test)]
impl DiagEntry {
    /// Create a new privacy-safe diagnostic entry.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider_name: impl Into<String>,
        model_name: impl Into<String>,
        backend_type: impl Into<String>,
        audio_duration_ms: u64,
        transcription_ms: u64,
        cleanup_ms: u64,
        realtime_factor: f64,
        fallback_used: bool,
        fallback_provider: Option<String>,
        retry_count: u32,
        error_code: Option<String>,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            model_name: model_name.into(),
            backend_type: backend_type.into(),
            audio_duration_ms,
            transcription_ms,
            cleanup_ms,
            realtime_factor,
            fallback_used,
            fallback_provider,
            retry_count,
            error_code,
        }
    }

    /// Convert to a log string with only safe, approved fields.
    /// No transcript, audio, keys, or clipboard content can be included.
    pub fn to_log_string(&self) -> String {
        let timestamp = chrono_now_iso();
        let fallback_info = if self.fallback_used {
            format!(
                "fallback_used=true,fallback_provider={}",
                self.fallback_provider.as_deref().unwrap_or("unknown")
            )
        } else {
            "fallback_used=false".to_string()
        };
        let error_info = self
            .error_code
            .as_deref()
            .map(|c| format!(",error_code={}", sanitize_error_for_log(c)))
            .unwrap_or_default();

        format!(
            "[{}] provider_name={},model_name={},backend_type={},audio_duration_ms={},transcription_ms={},cleanup_ms={},realtime_factor={:.3},retry_count={},{}{}",
            timestamp,
            self.provider_name,
            self.model_name,
            self.backend_type,
            self.audio_duration_ms,
            self.transcription_ms,
            self.cleanup_ms,
            self.realtime_factor,
            self.retry_count,
            fallback_info,
            error_info
        )
    }
}

/// Sanitize error codes for logging to ensure no sensitive data leaks.
#[cfg(test)]
fn sanitize_error_for_log(code: &str) -> String {
    // Redact any error codes that might contain secrets.
    // Uses the shared marker list from diag::report so new markers
    // are automatically checked everywhere.
    if crate::diag::report::contains_secret_marker(code) {
        return "redacted".to_string();
    }

    // Only allow alphanumeric, underscore, hyphen, and dot
    let sanitized: String = code
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.is_empty() || sanitized.len() > 64 {
        "redacted".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
fn chrono_now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();
    let secs = secs % 86400;
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
}

/// Append a string line to the diagnostics log file.
///
/// Privacy-safe: every line is scrubbed through `redact_string_for_report`
/// before writing, so accidental inclusion of bearer tokens, API keys,
/// transcripts, clipboard text, or other secret-shaped content from the
/// frontend gets replaced with `"redacted"` rather than persistent disk write.
#[tauri::command]
pub fn diag_log_str(diag: tauri::State<'_, DiagLog>, line: String) {
    diag.append_str(&crate::diag::report::redact_string_for_report(&line));
}

#[tauri::command]
pub fn get_recent_traces(
    tracer: tauri::State<'_, crate::diag::PipelineTracer>,
    count: Option<u32>,
) -> Vec<crate::diag::PipelineTrace> {
    let count = count.unwrap_or(5).clamp(1, 20) as usize;
    tracer.recent(count)
}

#[tauri::command]
pub fn get_current_trace(ctx: tauri::State<'_, crate::diag::PipelineContext>) -> Option<String> {
    ctx.current_trace_id()
}

/// Build and return the privacy-safe diagnostics report describing the most
/// recent dictation session and current app state.
///
/// Always returns a valid, serializable report — even if no session has
/// run yet, managers are unavailable, or the last session failed.
fn collect_platform_info() -> PlatformInfo {
    let mut platform = PlatformInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        family: std::env::consts::FAMILY.to_string(),
        tauri_version: None,
        os_version: None,
        cpu_model: None,
        cpu_logical_cores: None,
        memory_total_mb: None,
        memory_available_mb: None,
        process_memory_mb: None,
        uptime_secs: None,
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        use sysinfo::{ProcessesToUpdate, System};

        let mut sys = System::new();

        // OS version
        platform.os_version = System::long_os_version();

        // CPU info
        sys.refresh_cpu_all();
        let cpus = sys.cpus();
        platform.cpu_logical_cores = Some(cpus.len() as u32);
        if let Some(first) = cpus.first() {
            let brand = first.brand().to_string();
            if !brand.is_empty() {
                platform.cpu_model = Some(brand);
            }
        }

        // Memory info
        sys.refresh_memory();
        platform.memory_total_mb = Some(sys.total_memory() / 1024);
        platform.memory_available_mb = Some(sys.available_memory() / 1024);

        // Process memory (best-effort)
        if let Ok(pid) = sysinfo::get_current_pid() {
            sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), false);
            if let Some(process) = sys.process(pid) {
                platform.process_memory_mb = Some(process.memory() / 1024);
            }
        }

        // System uptime (the method needs the System instance)
        platform.uptime_secs = Some(sysinfo::System::uptime());
    }

    platform
}

#[tauri::command]
pub fn get_diagnostics_report<R: Runtime>(
    app: AppHandle<R>,
    settings_manager: tauri::State<'_, SettingsManager>,
    hotkey_manager: tauri::State<'_, HotkeyManager>,
    last_session: tauri::State<'_, LastSessionStore>,
    tracer: tauri::State<'_, crate::diag::PipelineTracer>,
) -> DiagnosticsReport {
    let platform = collect_platform_info();

    let hotkey = hotkey_manager
        .get_hotkey_settings(&settings_manager)
        .map(|status| crate::diag::HotkeySnapshot {
            accelerator: status.accelerator,
            label: status.label,
            is_default: status.is_default,
            is_registered: status.is_registered,
            error: status.error,
        })
        .unwrap_or_default();

    let mut settings_snapshot = SettingsSnapshot {
        feature_flags: BTreeMap::new(),
        ..Default::default()
    };
    if let Ok(status) = settings_manager.get_api_key_status() {
        settings_snapshot.api_key_configured = status.configured;
        settings_snapshot.api_key_masked_preview = status.masked_preview;
    }
    if let Ok(settings) = settings_manager.get_app_settings() {
        settings_snapshot.keyring_migrated = settings.keyring_migrated;
    }

    let integration = TauriAutostartIntegration::new(&app);
    if let Ok(status) = get_start_at_login_status_with(&integration) {
        settings_snapshot.start_at_login_enabled = Some(status.enabled);
        settings_snapshot.start_at_login_available = Some(status.available);
    }

    let mut session: SessionSnapshot = last_session.get().unwrap_or_default();
    if session.trace_id.is_none() {
        if let Some(trace) = tracer.recent(1).into_iter().next() {
            session.trace_id = Some(trace.trace_id);
        }
    }

    let provider_available = session.stt_provider.is_some();

    let recording_active =
        if let Some(manager) = app.try_state::<crate::recording::RecordingManager>() {
            manager
                .get_recording_status()
                .map(|s| s.is_recording)
                .unwrap_or(false)
        } else {
            false
        };

    let background_launch = crate::system::startup::is_background_launch_from_env();

    DiagnosticsReport::build(ReportInputs {
        platform,
        hotkey,
        settings: settings_snapshot,
        session,
        background_launch,
        recording_active,
        provider_available,
    })
}

/// Update the hotkey-to-recording-start latency for the current session.
///
/// The frontend measures this latency (time between hotkey press and recording start)
/// and calls this command to persist it in the session snapshot.
#[tauri::command]
pub fn update_session_hotkey_latency(
    last_session: tauri::State<'_, LastSessionStore>,
    trace_id: String,
    hotkey_to_recording_start_ms: u64,
) {
    last_session.update(|snapshot| {
        if snapshot.trace_id.as_deref() == Some(&trace_id) {
            snapshot.hotkey_to_recording_start_ms = hotkey_to_recording_start_ms;
        }
    });
}

/// Structured event from the frontend dictation pipeline, pushed into the
/// backend's detailed_timeline so the diagnostics report can reconstruct
/// frontend-only lifecycle events (pipeline wall-clock start, stage
/// transitions, frontend-detected retries, etc.).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendEvent {
    /// The active trace_id from the frontend.
    pub trace_id: String,
    /// Pipeline stage name (e.g. "pipeline", "stt", "cleanup", "clipboard", "paste").
    pub stage: String,
    /// Event type (e.g. "started", "completed", "failed", "retry").
    pub event_type: String,
    /// Duration in ms for this event (0 for instantaneous events).
    pub duration_ms: u64,
    /// Optional error code.
    pub error_code: Option<String>,
    /// Optional retry count.
    pub retry_count: Option<u32>,
    /// Frontend-measured total pipeline wall-clock duration.
    /// Only set on the final pipeline event to update the backend total.
    pub pipeline_total_ms: Option<u64>,
}

/// Accept a structured lifecycle event from the frontend and merge it into
/// the backend session snapshot's detailed_timeline. This bridges the gap
/// between the frontend's PipelineDiagnostics and the backend's report.
#[tauri::command]
pub fn log_frontend_event(last_session: tauri::State<'_, LastSessionStore>, event: FrontendEvent) {
    let trace_id = &event.trace_id;
    let detailed = crate::diag::DetailedEvent {
        stage: event.stage,
        event_type: event.event_type,
        duration_ms: event.duration_ms,
        error_code: event.error_code,
        retry_count: event.retry_count,
    };
    last_session.push_frontend_event(trace_id, detailed);

    // If this event carries a frontend-measured total, write it onto the snapshot.
    if let Some(total_ms) = event.pipeline_total_ms {
        last_session.set_frontend_total_ms(trace_id, total_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn diag_entry_contains_only_safe_fields() {
        let entry = DiagEntry::new(
            "groq",
            "whisper-large-v3-turbo",
            "cloud",
            5000,
            1200,
            300,
            0.24,
            false,
            None,
            0,
            None,
        );
        let log = entry.to_log_string();

        // Verify all required fields are present
        assert!(log.contains("provider_name=groq"));
        assert!(log.contains("model_name=whisper-large-v3-turbo"));
        assert!(log.contains("backend_type=cloud"));
        assert!(log.contains("audio_duration_ms=5000"));
        assert!(log.contains("transcription_ms=1200"));
        assert!(log.contains("cleanup_ms=300"));
        assert!(log.contains("realtime_factor=0.240"));
        assert!(log.contains("fallback_used=false"));

        // Verify no sensitive fields are present
        assert!(!log.contains("text="));
        assert!(!log.contains("audio="));
        assert!(!log.contains("key="));
        assert!(!log.contains("clipboard"));
    }

    #[test]
    fn diag_entry_with_fallback() {
        let entry = DiagEntry::new(
            "groq",
            "base",
            "native",
            1000,
            200,
            0,
            0.2,
            true,
            Some("groq".to_string()),
            1,
            None,
        );
        let log = entry.to_log_string();

        assert!(log.contains("fallback_used=true"));
        assert!(log.contains("fallback_provider=groq"));
        assert!(log.contains("retry_count=1"));
    }

    #[test]
    fn diag_entry_with_error_code() {
        let entry = DiagEntry::new(
            "groq",
            "whisper",
            "cloud",
            1000,
            500,
            0,
            0.5,
            false,
            None,
            0,
            Some("timeout_error".to_string()),
        );
        let log = entry.to_log_string();

        assert!(log.contains("error_code=timeout_error"));
    }

    #[test]
    fn sanitize_error_for_log_redacts_secrets() {
        assert_eq!(sanitize_error_for_log("Bearer gsk_abc123"), "redacted");
        assert_eq!(sanitize_error_for_log("authorization: token"), "redacted");
        assert_eq!(sanitize_error_for_log("api_key=secret"), "redacted");
        assert_eq!(sanitize_error_for_log("gsk_xyz"), "redacted");
    }

    #[test]
    fn sanitize_error_for_log_allows_safe_codes() {
        assert_eq!(sanitize_error_for_log("timeout_error"), "timeout_error");
        assert_eq!(sanitize_error_for_log("server_error"), "server_error");
        assert_eq!(sanitize_error_for_log("rate_limit"), "rate_limit");
    }

    #[test]
    fn sanitize_error_for_log_handles_special_chars() {
        assert_eq!(sanitize_error_for_log("error: test"), "error__test");
        assert_eq!(sanitize_error_for_log("ERROR CODE"), "error_code");
        assert_eq!(sanitize_error_for_log("error@test"), "error_test");
    }

    #[test]
    fn diag_log_str_writes_to_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_diag_str.log");
        let path_str = path.to_str().unwrap().to_string();

        let diag = DiagLog::new();
        diag.set_path(path_str);

        diag.append_str("[FE] test message from frontend");

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("[FE] test message from frontend"));
    }

    #[test]
    fn diag_log_str_ignores_empty_path() {
        let diag = DiagLog::new();
        diag.set_path("".to_string());

        // Should not panic or write to default location
        diag.append_str("test message");
    }

    #[test]
    fn contains_secret_marker_catches_sk_prefix_and_api_key() {
        use crate::diag::report::contains_secret_marker;

        assert!(contains_secret_marker("sk_abcdef12"), "sk_ prefix");
        assert!(contains_secret_marker("sk-abcdef12"), "sk- prefix");
        assert!(contains_secret_marker("gsk_abcdef12"), "gsk_ prefix");
        assert!(contains_secret_marker("api_key=value"), "api_key");
        assert!(contains_secret_marker("api-key=value"), "api-key");
        assert!(contains_secret_marker("apikey=value"), "apikey");
        assert!(!contains_secret_marker("timeout_error"), "safe code");
        assert!(!contains_secret_marker("server_error"), "safe code");
    }

    #[test]
    fn log_frontend_event_appends_to_session_snapshot() {
        use crate::diag::{LastSessionStore, SessionSnapshot};

        let store = LastSessionStore::new();
        store.set(SessionSnapshot {
            trace_id: Some("deadbeef".into()),
            ..Default::default()
        });

        // Simulate what log_frontend_event does
        let event = FrontendEvent {
            trace_id: "deadbeef".into(),
            stage: "pipeline".into(),
            event_type: "started".into(),
            duration_ms: 25,
            error_code: None,
            retry_count: None,
            pipeline_total_ms: None,
        };
        let detailed = crate::diag::DetailedEvent {
            stage: event.stage,
            event_type: event.event_type,
            duration_ms: event.duration_ms,
            error_code: event.error_code,
            retry_count: event.retry_count,
        };
        store.push_frontend_event(&event.trace_id, detailed);

        let snapshot = store.get().unwrap();
        assert_eq!(snapshot.detailed_timeline.len(), 1);
        assert_eq!(snapshot.detailed_timeline[0].stage, "pipeline");
        assert_eq!(snapshot.detailed_timeline[0].event_type, "started");
        assert_eq!(snapshot.detailed_timeline[0].duration_ms, 25);
    }

    #[test]
    fn log_frontend_event_sets_pipeline_total_ms() {
        use crate::diag::LastSessionStore;

        let store = LastSessionStore::new();
        store.set(crate::diag::SessionSnapshot {
            trace_id: Some("abc123".into()),
            ..Default::default()
        });

        store.set_frontend_total_ms("abc123", 2737);

        let snapshot = store.get().unwrap();
        assert_eq!(snapshot.pipeline_total_ms, Some(2737));
    }

    #[test]
    fn frontend_event_requires_matching_trace_id() {
        use crate::diag::LastSessionStore;

        let store = LastSessionStore::new();
        store.set(crate::diag::SessionSnapshot {
            trace_id: Some("session1".into()),
            ..Default::default()
        });

        // Push event with a DIFFERENT trace_id — should be silently ignored.
        let detailed = crate::diag::DetailedEvent {
            stage: "stt".into(),
            event_type: "completed".into(),
            duration_ms: 500,
            error_code: None,
            retry_count: None,
        };
        store.push_frontend_event("wrong_trace", detailed);

        let snapshot = store.get().unwrap();
        assert!(snapshot.detailed_timeline.is_empty());
    }
}
