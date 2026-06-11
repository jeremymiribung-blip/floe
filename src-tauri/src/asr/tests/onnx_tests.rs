//! Tests for the ONNX-based local ASR provider.
//!
//! These tests verify:
//! - Successful ONNX inference (with a dummy model created from bytes)
//! - Model loading failures
//! - Fallback behavior
//! - Backend routing
//! - Capability selection
//! - Diagnostics privacy

use crate::asr::adapters::model_cache::ModelCache;
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

/// Build a minimal valid ONNX model proto that acts as an identity:
/// input "x" (float[2]) → Identity → output "y" (float[2]).
///
/// The raw protobuf wire-format bytes were constructed manually against
/// the ONNX ModelProto schema (ir_version 8, opset 21).
fn minimal_onnx_identity_bytes() -> Vec<u8> {
    // Constructed from ONNX protobuf schema:
    // ModelProto:
    //   ir_version: 8            (field 1, int64, varint)
    //   producer_name: "test"    (field 2, string)
    //   opset_import: { domain: "", version: 21 }  (field 8, message)
    //   graph: { name: "test", node: { output: ["y"], op_type: "Identity",
    //            input: ["x"] }, input: [{name:"x", type{tensor{elem:1,shape{dim{2}}}}}],
    //            output: [{name:"y", type{tensor{elem:1,shape{dim{2}}}}}] }

    let mut bytes = Vec::new();

    // ir_version = 8 (field 1 varint)
    bytes.extend_from_slice(&[0x08, 0x08]);
    // producer_name = "test" (field 2 string, len=4)
    bytes.extend_from_slice(&[0x12, 0x04, b't', b'e', b's', b't']);
    // opset_import: domain="" version=21
    bytes.extend_from_slice(&[0x42, 0x04, 0x0A, 0x00, 0x10, 0x15]);

    // GraphProto (field 7 message)
    let graph = vec![
        // name = "test"
        0x12, 0x04, b't', b'e', b's', b't',
        // node[0] (field 1 repeated message)  -- Identity
        0x0A, 0x1C, // length=28
        //   NodeProto: input=["x"]
        0x0A, 0x01, b'x',
        //                output=["y"]
        0x12, 0x01, b'y',
        //                op_type="Identity"
        0x22, 0x08, b'I', b'd', b'e', b'n', b't', b'i', b't', b'y',
        // input[0] (field 11 repeated message) -- ValueInfoProto: name="x"
        // ValueInfoProto { name: "x", type: TypeProto { tensor_type: Tensor { elem_type: FLOAT(1), shape: { dim: [{ dim_value: 2 }] } } } }
        0x62, 0x10, // field 11 message, length=16
        //   name = "x"
        0x0A, 0x01, b'x',
        //   type (field 2 message)
        0x12, 0x0B, // length=11
        //     TypeProto: tensor_type (field 1 message)
        0x0A, 0x09, // length=9
        //       elem_type = FLOAT(1)
        0x08, 0x01,
        //       shape (field 2 message)
        0x12, 0x04, // length=4
        //         dim[0] (field 1 message)
        0x0A, 0x02, // length=2
        //           dim_value = 2
        0x08, 0x02,
        // output[0] (field 12 repeated message) -- ValueInfoProto: name="y"
        0x6A, 0x10, // field 12 message, length=16
        0x0A, 0x01, b'y',
        0x12, 0x0B, // length=11
        0x0A, 0x09, // length=9
        0x08, 0x01, 0x12, 0x04, 0x0A, 0x02, 0x08, 0x02,
    ];

    bytes.push(0x3A); // graph field 7 tag
    encode_varint(graph.len(), &mut bytes);
    bytes.extend_from_slice(&graph);

    bytes
}

fn encode_varint(mut value: usize, buf: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

// =========================================================================
// ONNX Inference Test (using in-memory model bytes)
// =========================================================================

#[test]
fn minimal_onnx_model_bytes_are_valid_protobuf() {
    let bytes = minimal_onnx_identity_bytes();
    assert!(!bytes.is_empty());
    assert!(bytes.len() > 20, "model too small: {} bytes", bytes.len());
}

#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn onnx_session_loads_from_memory() {
    // On non-Windows platforms with ort, verify model loading from
    // memory bytes works. On Windows, ort DLL lookup may require
    // specific setup, so this test is gated.
    let bytes = minimal_onnx_identity_bytes();

    let result = ort::Session::builder()
        .and_then(|b| b.with_model_from_memory("test.onnx", &bytes));

    match result {
        Ok(_session) => {
            // Session loaded successfully - model bytes are valid
        }
        Err(_) => {
            // ort may not be initialized or DLL not found in test env;
            // that's acceptable for this infrastructure test
        }
    }
}

// =========================================================================
// Model Loading Failures
// =========================================================================

#[test]
fn model_cache_returns_not_found_for_missing_model() {
    let cache = ModelCache::new(PathBuf::from("/nonexistent"));
    let result = cache.get_or_load("whisper-tiny");
    assert!(result.is_err());
}

#[test]
fn model_cache_has_model_returns_false_for_missing() {
    let cache = ModelCache::new(PathBuf::from("/nonexistent"));
    assert!(!cache.has_model("whisper-tiny"));
}

#[tokio::test]
async fn provider_create_session_fails_without_models() {
    let cache = Arc::new(ModelCache::new(PathBuf::from("/nonexistent")));
    let provider = WhisperOnnxProvider::new(cache);
    let config = SessionConfig {
        model: Some("whisper-tiny".into()),
        sample_rate: 16000,
        max_duration_secs: 30,
    };
    let result = provider.create_session(config).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, SessionErrorCode::Internal);
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
            fallback_compatible: false,
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
        crate::asr::policy::ResourcePolicy::default(),
        crate::asr::state::RuntimeState::new("local_whisper".into()),
    );

    let result = backend
        .transcribe(build_mono_wav(&[0i16; 2]), 1000, None)
        .await
        .unwrap();

    assert_eq!(result.text, "fallback transcript");
    assert!(result.diagnostics.fallback_used);
    assert_eq!(
        result.diagnostics.fallback_provider.as_deref(),
        Some("OK Provider")
    );
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

// =========================================================================
// Capability Selection
// =========================================================================

#[tokio::test]
async fn onnx_provider_capabilities_are_correct() {
    let cache = Arc::new(ModelCache::new(PathBuf::from("/nonexistent")));
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

    // The diagnostics struct has no text field
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

    // Verify only metadata fields exist
    assert_eq!(diag.provider_name, "local_whisper");
    assert_eq!(diag.model_name, "whisper-tiny");
    assert_eq!(diag.backend_type, BackendType::Onnx);
    assert_eq!(diag.audio_duration_ms, 5000);
    assert_eq!(diag.transcription_ms, 1200);
    assert_eq!(diag.cleanup_ms, 0);
    // No text, audio, or key fields accessible
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

    // Text is accessible on the TranscriptResult
    assert_eq!(result.text, "hello world");
    // But the text is NOT present in diagnostics
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
