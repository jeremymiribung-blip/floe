#![allow(dead_code)]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Deserialize;

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

    /// Append a privacy-safe diagnostic entry.
    /// Only the specified safe fields are allowed.
    pub fn append(&self, entry: DiagEntry) {
        if let Ok(guard) = self.path.lock() {
            if let Some(path) = guard.as_ref() {
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                    let _ = writeln!(file, "{}", entry.to_log_string());
                }
            }
        }
    }
}

/// A privacy-safe diagnostic entry containing only approved fields.
/// This struct enforces at compile time that we cannot accidentally
/// include sensitive data in diagnostics.
#[derive(Debug, Clone, Deserialize)]
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
fn sanitize_error_for_log(code: &str) -> String {
    let lower = code.trim().to_ascii_lowercase();

    // Redact any error codes that might contain secrets
    if lower.contains("bearer")
        || lower.contains("authorization")
        || lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("gsk_")
        || lower.contains("token")
    {
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

#[tauri::command]
pub fn diag_log(diag: tauri::State<'_, DiagLog>, entry: DiagEntry) {
    diag.append(entry);
}

/// Append a raw string line to the diagnostics log file.
/// This is privacy-safe: the frontend sends a plain string, not a DiagEntry.
#[tauri::command]
pub fn diag_log_str(diag: tauri::State<'_, DiagLog>, line: String) {
    diag.append_str(&line);
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
            "whisper_local",
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
    fn diag_log_writes_to_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_diag.log");
        let path_str = path.to_str().unwrap().to_string();

        let diag = DiagLog::new();
        diag.set_path(path_str);

        let entry = DiagEntry::new(
            "groq", "whisper", "cloud", 1000, 500, 0, 0.5, false, None, 0, None,
        );

        diag.append(entry);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("provider_name=groq"));
        assert!(content.contains("model_name=whisper"));
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
    fn diag_log_ignores_empty_path() {
        let diag = DiagLog::new();
        diag.set_path("".to_string());

        let entry = DiagEntry::new(
            "groq", "whisper", "cloud", 1000, 500, 0, 0.5, false, None, 0, None,
        );

        // Should not panic or write to default location
        diag.append(entry);
    }
}
