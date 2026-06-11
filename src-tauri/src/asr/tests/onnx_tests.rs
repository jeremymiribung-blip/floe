//! Tests for the ONNX-based local ASR provider.
//!
//! These tests verify:
//! - Model loading failures
//! - Fallback behavior
//! - Backend routing
//! - Capability selection
//! - Diagnostics privacy

use crate::asr::adapters::onnx_runtime::SessionCache;
use crate::asr::adapters::onnx::WhisperOnnxProvider;
use crate::asr::error::{AsrErrorCode, SessionErrorCode};
use crate::asr::registry::ProviderRegistry;
use crate::asr::traits::{AsrProvider, AsrSession};
use crate::asr::types::{
    AsrDiagnostics, BackendType, Deployment, ModelSpec, ProviderCapabilities, SelectionCriteria,
    SessionConfig,
};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

// =========================================================================
// Model Loading Failures
// =========================================================================

#[test]
fn session_cache_returns_not_found_for_missing_model() {
    let cache = SessionCache::new(PathBuf::from("/nonexistent"));
    let result = cache.get_or_load("whisper-tiny");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code, SessionErrorCode::ModelUnavailable);
}

#[test]
fn session_cache_has_model_returns_false_for_missing() {
    let cache = SessionCache::new(PathBuf::from("/nonexistent"));
    assert!(!cache.has_model("whisper-tiny"));
}

#[tokio::test]
async fn provider_create_session_fails_without_models() {
    let cache = Arc::new(SessionCache::new(PathBuf::from("/nonexistent")));
    let provider = WhisperOnnxProvider::new(cache);
    let config = SessionConfig {
        model: Some("whisper-tiny".into()),
        sample_rate: 16000,
        max_duration_secs: 30,
    };
    let result = provider.create_session(config).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, SessionErrorCode::ModelUnavailable);
}

// =========================================================================
// Fallback Behavior
// =========================================================================

#[derive(Debug)]
struct FailingOnnxSession;

#[async_trait]
impl crate::asr::traits::AsrSession for FailingOnnxSession {
    fn model(&self) -> &str {
        "whisper-tiny"
    }
    fn provider_id(&self) -> &'static str {
        "local_whisper"
    }
    async fn submit_audio(&self, _: crate::asr::types::AudioChunk) -> Result<(), ()> {
        Ok(())
    }
    async fn partial_transcript(&self) -> Option<String> {
        None
    }
    async fn finalize(
        self: Box<Self>,
    ) -> Result<crate::asr::types::TranscriptResult, crate::asr::traits::TranscriptionError>
    {
        Err(crate::asr::traits::TranscriptionError::new(
            crate::asr::traits::TranscriptionErrorCode::Internal,
            "onnx inference failed",
            false,
        ))
    }
    async fn cancel(self: Box<Self>) {}
}

#[derive(Debug)]
struct FailingOnnxProvider;

#[async_trait]
impl AsrProvider for FailingOnnxProvider {
    fn id(&self) -> &'static str {
        "local_whisper"
    }
    fn name(&self) -> &'static str {
        "Failing ONNX"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            backend_type: BackendType::Onnx,
            deployment: Deployment::Local,
            fallback_compatible: true,
            ..Default::default()
        }
    }
    fn default_model(&self) -> &'static str {
        "whisper-tiny"
    }
    fn available_models(&self) -> &[ModelSpec] {
        &[]
    }
    async fn create_session(
        &self,
        _: SessionConfig,
    ) -> Result<Box<dyn AsrSession>, crate::asr::error::SessionError> {
        Ok(Box::new(FailingOnnxSession))
    }
    async fn health_check(&self) -> Result<crate::asr::types::HealthStatus, ()> {
        Ok(crate::asr::types::HealthStatus::Healthy)
    }
}

#[derive(Debug)]
struct OkSession;

#[async_trait]
impl crate::asr::traits::AsrSession for OkSession {
    fn model(&self) -> &str {
        "model"
    }
    fn provider_id(&self) -> &'static str {
        "ok_provider"
    }
    async fn submit_audio(&self, _: crate::asr::types::AudioChunk) -> Result<(), ()> {
        Ok(())
    }
    async fn partial_transcript(&self) -> Option<String> {
        None
    }
    async fn finalize(
        self: Box<Self>,
    ) -> Result<crate::asr::types::TranscriptResult, crate::asr::traits::TranscriptionError>
    {
        Ok(crate::asr::types::TranscriptResult {
            text: "fallback transcript".into(),
            model: "model".into(),
            diagnostics: AsrDiagnostics::new("ok_provider", "model", BackendType::Cloud, 1000, 500, "test"),
        })
    }
    async fn cancel(self: Box<Self>) {}
}

#[derive(Debug)]
struct OkProvider;

#[async_trait]
impl AsrProvider for OkProvider {
    fn id(&self) -> &'static str {
        "ok_provider"
    }
    fn name(&self) -> &'static str {
        "OK Provider"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            fallback_compatible: true,
            ..Default::default()
        }
    }
    fn default_model(&self) -> &'static str {
        "model"
    }
    fn available_models(&self) -> &[ModelSpec] {
        &[]
    }
    async fn create_session(
        &self,
        _: SessionConfig,
    ) -> Result<Box<dyn AsrSession>, crate::asr::error::SessionError> {
        Ok(Box::new(OkSession))
    }
    async fn health_check(&self) -> Result<crate::asr::types::HealthStatus, ()> {
        Ok(crate::asr::types::HealthStatus::Healthy)
    }
}

fn build_mono_wav(samples: &[i16]) -> Vec<u8> {
    let channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let sample_rate: u32 = 16000;
    let bytes_per_sample = (bits_per_sample / 8) as u32;
    let block_align = channels as u32 * bytes_per_sample;
    let byte_rate = sample_rate * block_align;
    let data_size = samples.len() as u32 * bytes_per_sample;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&(block_align as u16).to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for &s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}

#[tokio::test]
async fn fallback_to_ok_provider_when_onnx_fails() {
    let mut registry = ProviderRegistry::new();
    registry
        .register(Box::new(FailingOnnxProvider))
        .unwrap();
    registry.register(Box::new(OkProvider)).unwrap();

    let backend = crate::asr::backend::AsrBackend::new(
        Arc::new(registry),
        crate::asr::policy::ResourcePolicy::default()
            .with_local_models(true),
        crate::asr::state::RuntimeState::new("local_whisper".into()),
    );

    let result = backend
        .transcribe(build_mono_wav(&[0i16; 2]), 1000, Some("local_whisper"))
        .await
        .unwrap();

    assert_eq!(result.text, "fallback transcript");
    assert!(result.diagnostics.fallback_used);
}

#[tokio::test]
async fn groq_primary_does_not_fallback_to_groq() {
    let mut registry = ProviderRegistry::new();
    registry.register(Box::new(OkProvider)).unwrap();

    let backend = crate::asr::backend::AsrBackend::new(
        Arc::new(registry),
        crate::asr::policy::ResourcePolicy::default(),
        crate::asr::state::RuntimeState::new("ok_provider".into()),
    );

    let result = backend
        .transcribe(build_mono_wav(&[0i16; 2]), 1000, Some("ok_provider"))
        .await
        .unwrap();

    assert!(!result.diagnostics.fallback_used);
}

// =========================================================================
// Backend Routing
// =========================================================================

#[derive(Debug)]
struct LocalOnlySession;

#[async_trait]
impl crate::asr::traits::AsrSession for LocalOnlySession {
    fn model(&self) -> &str {
        "model"
    }
    fn provider_id(&self) -> &'static str {
        "local_only"
    }
    async fn submit_audio(&self, _: crate::asr::types::AudioChunk) -> Result<(), ()> {
        Ok(())
    }
    async fn partial_transcript(&self) -> Option<String> {
        None
    }
    async fn finalize(
        self: Box<Self>,
    ) -> Result<crate::asr::types::TranscriptResult, crate::asr::traits::TranscriptionError>
    {
        Ok(crate::asr::types::TranscriptResult {
            text: "local result".into(),
            model: "model".into(),
            diagnostics: AsrDiagnostics::new("local_only", "model", BackendType::Onnx, 1000, 200, "test"),
        })
    }
    async fn cancel(self: Box<Self>) {}
}

#[derive(Debug)]
struct LocalOnlyProvider;

#[async_trait]
impl AsrProvider for LocalOnlyProvider {
    fn id(&self) -> &'static str {
        "local_only"
    }
    fn name(&self) -> &'static str {
        "Local Only"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            backend_type: BackendType::Onnx,
            deployment: Deployment::Local,
            fallback_compatible: false,
            ..Default::default()
        }
    }
    fn default_model(&self) -> &'static str {
        "model"
    }
    fn available_models(&self) -> &[ModelSpec] {
        &[]
    }
    async fn create_session(
        &self,
        _: SessionConfig,
    ) -> Result<Box<dyn AsrSession>, crate::asr::error::SessionError> {
        Ok(Box::new(LocalOnlySession))
    }
    async fn health_check(&self) -> Result<crate::asr::types::HealthStatus, ()> {
        Ok(crate::asr::types::HealthStatus::Healthy)
    }
}

#[test]
fn selection_criteria_requires_local_selects_onnx_provider() {
    let mut registry = ProviderRegistry::new();
    registry.register(Box::new(LocalOnlyProvider)).unwrap();

    let selected = registry
        .select(SelectionCriteria {
            requires_local: true,
            ..Default::default()
        })
        .unwrap();

    assert_eq!(selected.id(), "local_only");
    assert_eq!(selected.capabilities().deployment, Deployment::Local);
    assert_eq!(selected.capabilities().backend_type, BackendType::Onnx);
}

#[test]
fn selection_criteria_requires_local_filters_cloud_providers() {
    use crate::asr::error::SelectionErrorCode;

    let mut registry = ProviderRegistry::new();
    registry
        .register(Box::new(OkProvider))
        .unwrap();

    let result = registry.select(SelectionCriteria {
        requires_local: true,
        ..Default::default()
    });

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code,
        SelectionErrorCode::NoSuitableProvider
    );
}

#[tokio::test]
async fn backend_rejects_local_when_policy_disallows() {
    let mut registry = ProviderRegistry::new();
    registry.register(Box::new(LocalOnlyProvider)).unwrap();

    let backend = crate::asr::backend::AsrBackend::new(
        Arc::new(registry),
        crate::asr::policy::ResourcePolicy::default()
            .with_local_models(false),
        crate::asr::state::RuntimeState::new("local_only".into()),
    );

    let result = backend
        .transcribe(build_mono_wav(&[0i16; 2]), 1000, Some("local_only"))
        .await;

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code,
        AsrErrorCode::ProviderRejected
    );
}

#[tokio::test]
async fn backend_allows_local_when_policy_enables() {
    let mut registry = ProviderRegistry::new();
    registry.register(Box::new(LocalOnlyProvider)).unwrap();

    let backend = crate::asr::backend::AsrBackend::new(
        Arc::new(registry),
        crate::asr::policy::ResourcePolicy::default()
            .with_local_models(true),
        crate::asr::state::RuntimeState::new("local_only".into()),
    );

    let result = backend
        .transcribe(build_mono_wav(&[0i16; 2]), 1000, Some("local_only"))
        .await;

    assert!(result.is_ok());
}

// =========================================================================
// Capability Selection
// =========================================================================

#[tokio::test]
async fn onnx_provider_capabilities_are_correct() {
    let cache = Arc::new(SessionCache::new(PathBuf::from("/nonexistent")));
    let provider = WhisperOnnxProvider::new(cache);
    let caps = provider.capabilities();

    assert_eq!(caps.backend_type, BackendType::Onnx);
    assert_eq!(caps.deployment, Deployment::Local);
    assert!(caps.fallback_compatible);
    assert!(!caps.gpu_required);
    assert!(!caps.partials);
    assert_eq!(caps.max_audio_seconds, 30);
    assert!(caps.supported_sample_rates.contains(&16000));
}

#[test]
fn local_only_provider_capabilities_match_deployment() {
    let provider = LocalOnlyProvider;
    let caps = provider.capabilities();

    assert_eq!(caps.deployment, Deployment::Local);
    assert_eq!(caps.backend_type, BackendType::Onnx);
    assert!(!caps.fallback_compatible);
}

#[test]
fn cloud_provider_capabilities_differ_from_local() {
    let provider = OkProvider;
    let caps = provider.capabilities();

    assert_eq!(caps.deployment, Deployment::Cloud);
    assert_eq!(caps.backend_type, BackendType::Cloud);
}

// =========================================================================
// Diagnostics Privacy
// =========================================================================

#[test]
fn onnx_diagnostics_has_no_transcript_text() {
    let diag = AsrDiagnostics::new(
        "local_whisper",
        "whisper-tiny",
        BackendType::Onnx,
        5000,
        1200,
        "test",
    );

    assert_eq!(diag.provider_name, "local_whisper");
    assert_eq!(diag.model_name, "whisper-tiny");
    assert_eq!(diag.backend_type, BackendType::Onnx);
    assert!(!diag.fallback_used);
}

#[test]
fn onnx_diagnostics_with_error_sanitizes() {
    use crate::asr::types::sanitize_error_code;

    let sanitized = sanitize_error_code("onnx_inference_error");
    assert_eq!(sanitized, "onnx_inference_error");

    let sanitized = sanitize_error_code("Bearer gsk_secret");
    assert_eq!(sanitized, "internal");
}

#[test]
fn onnx_diagnostics_metadata_only_no_audio() {
    let diag = AsrDiagnostics::new(
        "local_whisper",
        "whisper-tiny",
        BackendType::Onnx,
        5000,
        1200,
        "test",
    );

    assert_eq!(diag.provider_name, "local_whisper");
    assert_eq!(diag.model_name, "whisper-tiny");
    assert_eq!(diag.backend_type, BackendType::Onnx);
    assert_eq!(diag.audio_duration_ms, 5000);
    assert_eq!(diag.transcription_ms, 1200);
    assert_eq!(diag.cleanup_ms, 0);
}

#[test]
fn transcript_result_separates_text_from_diagnostics() {
    let result = crate::asr::types::TranscriptResult {
        text: "hello world".into(),
        model: "whisper-tiny".into(),
        diagnostics: AsrDiagnostics::new(
            "local_whisper",
            "whisper-tiny",
            BackendType::Onnx,
            5000,
            1200,
            "test",
        ),
    };

    assert_eq!(result.text, "hello world");
    assert_eq!(result.diagnostics.provider_name, "local_whisper");
    assert_eq!(result.diagnostics.model_name, "whisper-tiny");
}

// =========================================================================
// Backend Routing through AsrBackend
// =========================================================================

#[tokio::test]
async fn backend_routes_to_local_when_selected() {
    let mut registry = ProviderRegistry::new();
    registry.register(Box::new(LocalOnlyProvider)).unwrap();
    registry.register(Box::new(OkProvider)).unwrap();
    registry.set_default("local_only").unwrap();

    let backend = crate::asr::backend::AsrBackend::new(
        Arc::new(registry),
        crate::asr::policy::ResourcePolicy::default()
            .with_local_models(true),
        crate::asr::state::RuntimeState::new("local_only".into()),
    );

    let result = backend
        .transcribe(build_mono_wav(&[0i16; 2]), 1000, Some("local_only"))
        .await
        .unwrap();

    assert_eq!(result.text, "local result");
    assert_eq!(result.model, "model");
}

#[tokio::test]
async fn backend_falls_back_when_local_provider_fails() {
    let mut registry = ProviderRegistry::new();
    registry.register(Box::new(FailingOnnxProvider)).unwrap();
    registry.register(Box::new(OkProvider)).unwrap();
    registry.set_default("local_whisper").unwrap();

    let backend = crate::asr::backend::AsrBackend::new(
        Arc::new(registry),
        crate::asr::policy::ResourcePolicy::default()
            .with_local_models(true),
        crate::asr::state::RuntimeState::new("local_whisper".into()),
    );

    let result = backend
        .transcribe(build_mono_wav(&[0i16; 2]), 1000, None)
        .await
        .unwrap();

    assert!(result.diagnostics.fallback_used);
    assert_eq!(result.text, "fallback transcript");
}

// =========================================================================
// Fallback Provider Exclusion
// =========================================================================

#[test]
fn fallback_excludes_primary_provider() {
    let mut registry = ProviderRegistry::new();
    registry.register(Box::new(FailingOnnxProvider)).unwrap();
    registry.register(Box::new(OkProvider)).unwrap();

    let fallback = registry.fallback_provider_excluding(Some("local_whisper"));
    assert!(fallback.is_some());
    assert_eq!(fallback.unwrap().id(), "ok_provider");
}

#[test]
fn fallback_excludes_groq_when_groq_is_primary() {
    let mut registry = ProviderRegistry::new();
    registry.register(Box::new(OkProvider)).unwrap();

    let fallback = registry.fallback_provider_excluding(Some("ok_provider"));
    assert!(fallback.is_none());
}

#[test]
fn fallback_returns_none_when_no_compatible() {
    let mut registry = ProviderRegistry::new();
    registry.register(Box::new(LocalOnlyProvider)).unwrap();

    let fallback = registry.fallback_provider_excluding(None);
    assert!(fallback.is_none());
}

#[tokio::test]
async fn backend_fallback_excludes_primary() {
    let mut registry = ProviderRegistry::new();
    registry.register(Box::new(FailingOnnxProvider)).unwrap();
    registry.register(Box::new(OkProvider)).unwrap();

    let backend = crate::asr::backend::AsrBackend::new(
        Arc::new(registry),
        crate::asr::policy::ResourcePolicy::default()
            .with_local_models(true),
        crate::asr::state::RuntimeState::new("local_whisper".into()),
    );

    let result = backend
        .transcribe(build_mono_wav(&[0i16; 2]), 1000, Some("local_whisper"))
        .await
        .unwrap();

    assert!(result.diagnostics.fallback_used);
    assert_eq!(result.text, "fallback transcript");
    assert_eq!(result.diagnostics.fallback_provider.as_deref(), Some("OK Provider"));
}
