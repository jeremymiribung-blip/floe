use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendType {
    Cloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Deployment {
    Cloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamingSupport {
    None,
    Full,
}

#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    pub backend_type: BackendType,
    pub deployment: Deployment,
    pub streaming: StreamingSupport,
    pub partials: bool,
    pub timestamps: bool,
    pub gpu_required: bool,
    pub fallback_compatible: bool,
    pub max_audio_seconds: u64,
    pub supported_sample_rates: Vec<u32>,
    pub min_audio_bytes: u64,
    pub max_audio_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub requires_gpu: bool,
    pub max_duration_secs: u64,
    pub supported_languages: Option<Vec<String>>,
    pub parameters: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub data: Vec<f32>,
    pub sample_rate: u32,
    pub is_final: bool,
}

#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub text: String,
    pub model: String,
    pub diagnostics: AsrDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrDiagnostics {
    pub trace_version: u8,
    pub created_at: String,
    pub platform: String,
    pub provider_name: String,
    pub model_name: String,
    pub backend_type: BackendType,
    pub audio_duration_ms: u64,
    pub transcription_ms: u64,
    pub cleanup_ms: u64,
    pub realtime_factor: f64,
    pub fallback_used: bool,
    pub fallback_provider: Option<String>,
    pub retry_count: u32,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub model: Option<String>,
    pub sample_rate: u32,
    pub max_duration_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
}

#[derive(Debug, Clone, Default)]
pub struct SelectionCriteria {
    pub preferred: Option<String>,
    pub audio_duration_ms: u64,
    pub requires_fallback_compatible: bool,
    pub requires_streaming: bool,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            backend_type: BackendType::Cloud,
            deployment: Deployment::Cloud,
            streaming: StreamingSupport::None,
            partials: false,
            timestamps: false,
            gpu_required: false,
            fallback_compatible: false,
            max_audio_seconds: 120,
            supported_sample_rates: vec![16_000],
            min_audio_bytes: 1,
            max_audio_bytes: 25_000_000,
        }
    }
}

impl AsrDiagnostics {
    pub fn new(
        provider_name: &str,
        model_name: &str,
        backend_type: BackendType,
        audio_duration_ms: u64,
        transcription_ms: u64,
        platform: &str,
    ) -> Self {
        Self {
            trace_version: 1,
            created_at: chrono_now_iso(),
            platform: platform.to_string(),
            provider_name: provider_name.to_string(),
            model_name: model_name.to_string(),
            backend_type,
            audio_duration_ms,
            transcription_ms,
            cleanup_ms: 0,
            realtime_factor: realtime_factor(transcription_ms, audio_duration_ms),
            fallback_used: false,
            fallback_provider: None,
            retry_count: 0,
            error_code: None,
        }
    }

    pub fn with_error(mut self, error_code: &str) -> Self {
        self.error_code = Some(sanitize_error_code(error_code));
        self
    }

    pub fn with_fallback(mut self, provider: &str) -> Self {
        self.fallback_used = true;
        self.fallback_provider = Some(provider.to_string());
        self
    }

    pub fn with_retry(mut self, count: u32) -> Self {
        self.retry_count = count;
        self
    }

    pub fn with_cleanup_ms(mut self, ms: u64) -> Self {
        self.cleanup_ms = ms;
        self
    }
}

impl HealthStatus {
    pub fn is_eligible_for_fallback(&self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded(_))
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
    format!(
        "1970-01-01T{:02}:{:02}:{:02}.{:03}Z",
        hours, minutes, seconds, millis
    )
}

pub fn realtime_factor(transcription_ms: u64, audio_duration_ms: u64) -> f64 {
    if audio_duration_ms == 0 {
        return 0.0;
    }
    let value = transcription_ms as f64 / audio_duration_ms as f64;
    (value * 1000.0).round() / 1000.0
}

pub fn sanitize_error_code(code: &str) -> String {
    let lower = code.trim().to_ascii_lowercase();
    if lower.contains("bearer")
        || lower.contains("authorization")
        || lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("gsk_")
    {
        return "internal".to_string();
    }
    let sanitized: String = code
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() || sanitized.len() > 64 {
        "internal".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_constructs_success() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 5000, 1200, "linux");
        assert_eq!(d.provider_name, "groq");
        assert_eq!(d.model_name, "whisper");
        assert!(!d.fallback_used);
        assert!(d.fallback_provider.is_none());
        assert!(d.error_code.is_none());
        assert_eq!(d.realtime_factor, 0.24);
    }

    #[test]
    fn diagnostics_with_error_sanitizes() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 1000, 500, "linux")
            .with_error("INVALID_AUTH");
        assert_eq!(d.error_code.as_deref(), Some("invalid_auth"));
    }

    #[test]
    fn diagnostics_with_error_redacts_secrets() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 1000, 500, "linux")
            .with_error("Bearer gsk_abcdef");
        assert_eq!(d.error_code.as_deref(), Some("internal"));
    }

    #[test]
    fn diagnostics_with_fallback() {
        let d = AsrDiagnostics::new(
            "whisper_local",
            "base",
            BackendType::Cloud,
            1000,
            200,
            "macos",
        )
        .with_fallback("groq");
        assert!(d.fallback_used);
        assert_eq!(d.fallback_provider.as_deref(), Some("groq"));
    }

    #[test]
    fn health_status_eligibility() {
        assert!(HealthStatus::Healthy.is_eligible_for_fallback());
        assert!(HealthStatus::Degraded("slow".into()).is_eligible_for_fallback());
        assert!(!HealthStatus::Unhealthy("down".into()).is_eligible_for_fallback());
    }

    #[test]
    fn realtime_factor_rounds_and_is_zero_safe() {
        assert!((realtime_factor(250, 1000) - 0.25).abs() < f64::EPSILON);
        assert!((realtime_factor(333, 1000) - 0.333).abs() < 0.001);
        assert!((realtime_factor(10, 0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn sanitize_error_removes_keys() {
        assert_eq!(sanitize_error_code("bearer token"), "internal");
        assert_eq!(sanitize_error_code("gsk_abc123"), "internal");
        assert_eq!(sanitize_error_code("timeout_error"), "timeout_error");
    }

    #[test]
    fn provider_capabilities_defaults_are_sensible() {
        let caps = ProviderCapabilities::default();
        assert_eq!(caps.backend_type, BackendType::Cloud);
        assert!(!caps.partials);
        assert!(!caps.gpu_required);
    }
}
