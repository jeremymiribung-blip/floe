use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;

use crate::asr::error::SessionError;
use crate::asr::traits::{AsrProvider, AsrSession, TranscriptionError, TranscriptionErrorCode};
use crate::asr::types::{
    AsrDiagnostics, AudioChunk, BackendType, Deployment, HealthStatus, ModelSpec,
    ProviderCapabilities, SessionConfig, StreamingSupport, TranscriptResult,
};

#[derive(Debug)]
pub struct FakeAsrSession {
    response_text: String,
    model: String,
    provider_id: &'static str,
    fail: bool,
    failure_code: TranscriptionErrorCode,
    latency_ms: u64,
}

impl FakeAsrSession {
    pub fn ok(response_text: impl Into<String>, latency_ms: u64) -> Self {
        Self {
            response_text: response_text.into(),
            model: "whisper-large-v3-turbo".into(),
            provider_id: "fake_asr",
            fail: false,
            failure_code: TranscriptionErrorCode::ServerError,
            latency_ms,
        }
    }

    pub fn failing(code: TranscriptionErrorCode) -> Self {
        Self {
            response_text: String::new(),
            model: String::new(),
            provider_id: "fake_asr",
            fail: true,
            failure_code: code,
            latency_ms: 0,
        }
    }
}

#[async_trait]
impl AsrSession for FakeAsrSession {
    fn model(&self) -> &str {
        &self.model
    }

    fn provider_id(&self) -> &'static str {
        self.provider_id
    }

    async fn submit_audio(&self, _chunk: AudioChunk) -> Result<(), ()> {
        Ok(())
    }

    async fn partial_transcript(&self) -> Option<String> {
        None
    }

    async fn finalize(self: Box<Self>) -> Result<TranscriptResult, TranscriptionError> {
        if self.latency_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.latency_ms)).await;
        }
        if self.fail {
            return Err(TranscriptionError::new(
                self.failure_code,
                "fake ASR session failed",
                false,
            ));
        }
        Ok(TranscriptResult {
            text: self.response_text,
            model: self.model.clone(),
            diagnostics: AsrDiagnostics::new(
                self.provider_id,
                &self.model,
                BackendType::Cloud,
                1000,
                0,
                "test",
            ),
        })
    }

    async fn cancel(self: Box<Self>) {}
}

#[derive(Debug)]
pub struct FakeAsrProvider {
    id: &'static str,
    session: Mutex<Option<Box<dyn AsrSession>>>,
    fail_create: bool,
    health: HealthStatus,
}

impl FakeAsrProvider {
    pub fn ok(id: &'static str, session: Box<dyn AsrSession>) -> Self {
        Self {
            id,
            session: Mutex::new(Some(session)),
            fail_create: false,
            health: HealthStatus::Healthy,
        }
    }

    #[allow(dead_code)]
    pub fn unhealthy(id: &'static str, reason: &str) -> Self {
        Self {
            id,
            session: Mutex::new(None),
            fail_create: true,
            health: HealthStatus::Unhealthy(reason.into()),
        }
    }
}

#[async_trait]
impl AsrProvider for FakeAsrProvider {
    fn id(&self) -> &'static str {
        self.id
    }

    fn name(&self) -> &'static str {
        "Fake ASR Provider"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            backend_type: BackendType::Cloud,
            deployment: Deployment::Cloud,
            streaming: StreamingSupport::None,
            partials: false,
            timestamps: false,
            gpu_required: false,
            fallback_compatible: true,
            max_audio_seconds: 120,
            supported_sample_rates: vec![16_000],
            min_audio_bytes: 1,
            max_audio_bytes: 25_000_000,
        }
    }

    fn default_model(&self) -> &'static str {
        "whisper-large-v3-turbo"
    }

    fn available_models(&self) -> &[ModelSpec] {
        &[]
    }

    async fn create_session(
        &self,
        _config: SessionConfig,
    ) -> Result<Box<dyn AsrSession>, SessionError> {
        if self.fail_create {
            return Err(SessionError::new(
                crate::asr::error::SessionErrorCode::Internal,
                "fake provider unavailable",
            ));
        }
        self.session
            .lock()
            .map_err(|_| {
                SessionError::new(
                    crate::asr::error::SessionErrorCode::Internal,
                    "mutex poisoned",
                )
            })?
            .take()
            .ok_or_else(|| {
                SessionError::new(
                    crate::asr::error::SessionErrorCode::Internal,
                    "session already consumed",
                )
            })
    }

    async fn health_check(&self) -> Result<HealthStatus, ()> {
        Ok(self.health.clone())
    }
}
