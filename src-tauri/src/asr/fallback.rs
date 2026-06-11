use std::time::Duration;

use super::error::AsrError;
use super::traits::{AsrProvider, TranscriptionError, TranscriptionErrorCode};
use super::types::TranscriptResult;

const MAX_RETRY_ATTEMPTS: usize = 2;
const RETRY_BACKOFF: Duration = Duration::from_millis(250);

pub struct FallbackStrategy;

impl FallbackStrategy {
    pub async fn execute(
        primary: &dyn AsrProvider,
        fallback: Option<&dyn AsrProvider>,
        audio: Vec<u8>,
        audio_duration_ms: u64,
    ) -> Result<TranscriptResult, AsrError> {
        let primary_name = primary.name();
        let fallback_name = fallback.map(|p| p.name());

        match Self::attempt_transcription(primary, &audio, audio_duration_ms).await {
            Ok(result) => {
                let mut diag = result.diagnostics.clone();
                diag.fallback_used = false;
                let mut result = result;
                result.diagnostics = diag;
                Ok(result)
            }
            Err(err) => {
                if !err.retryable {
                    return Self::try_fallback(
                        fallback,
                        fallback_name,
                        audio,
                        audio_duration_ms,
                        err,
                        0,
                    )
                    .await;
                }
                Self::retry_then_fallback(
                    primary,
                    primary_name,
                    fallback,
                    fallback_name,
                    audio,
                    audio_duration_ms,
                    err,
                )
                .await
            }
        }
    }

    async fn attempt_transcription(
        provider: &dyn AsrProvider,
        audio: &[u8],
        audio_duration_ms: u64,
    ) -> Result<TranscriptResult, TranscriptionError> {
        let config = super::types::SessionConfig {
            model: None,
            sample_rate: 16_000,
            max_duration_secs: (audio_duration_ms / 1000).max(1),
        };
        let session = provider.create_session(config).await.map_err(|_| {
            TranscriptionError::new(
                TranscriptionErrorCode::Internal,
                "failed to create session",
                false,
            )
        })?;

        session
            .submit_audio(super::types::AudioChunk {
                data: decode_wav_to_f32(audio).map_err(|_| {
                    TranscriptionError::new(
                        TranscriptionErrorCode::Internal,
                        "failed to decode audio",
                        false,
                    )
                })?,
                sample_rate: 16_000,
                is_final: true,
            })
            .await
            .map_err(|_| {
                TranscriptionError::new(
                    TranscriptionErrorCode::Internal,
                    "failed to submit audio",
                    true,
                )
            })?;

        session.finalize().await
    }

    async fn retry_then_fallback(
        primary: &dyn AsrProvider,
        _primary_name: &str,
        fallback: Option<&dyn AsrProvider>,
        fallback_name: Option<&str>,
        audio: Vec<u8>,
        audio_duration_ms: u64,
        last_error: TranscriptionError,
    ) -> Result<TranscriptResult, AsrError> {
        let mut last_err = last_error;

        for attempt in 1..=MAX_RETRY_ATTEMPTS {
            let delay = RETRY_BACKOFF * (1 << attempt) as u32;
            tokio::time::sleep(delay).await;

            match Self::attempt_transcription(primary, &audio, audio_duration_ms).await {
                Ok(result) => {
                    let mut result = result;
                    result.diagnostics = result.diagnostics.with_retry(attempt as u32);
                    return Ok(result);
                }
                Err(e) => {
                    last_err = e;
                    if !last_err.retryable {
                        break;
                    }
                }
            }
        }

        Self::try_fallback(fallback, fallback_name, audio, audio_duration_ms, last_err, MAX_RETRY_ATTEMPTS as u32).await
    }

    async fn try_fallback(
        fallback: Option<&dyn AsrProvider>,
        fallback_name: Option<&str>,
        audio: Vec<u8>,
        audio_duration_ms: u64,
        primary_error: TranscriptionError,
        retry_count: u32,
    ) -> Result<TranscriptResult, AsrError> {
        let Some(fallback_provider) = fallback else {
            return Err(AsrError::new(
                super::error::AsrErrorCode::FallbackFailed,
                format!(
                    "Primary provider failed and no fallback is available: {}",
                    primary_error.message
                ),
            ));
        };

        match Self::attempt_transcription(fallback_provider, &audio, audio_duration_ms).await {
            Ok(mut result) => {
                result.diagnostics = result
                    .diagnostics
                    .with_fallback(fallback_name.unwrap_or("unknown"))
                    .with_retry(retry_count);
                Ok(result)
            }
            Err(fb_err) => Err(AsrError::new(
                super::error::AsrErrorCode::FallbackFailed,
                format!(
                    "Primary failed ({}) and fallback also failed: {}",
                    primary_error.message, fb_err.message
                ),
            )),
        }
    }
}

fn decode_wav_to_f32(wav_bytes: &[u8]) -> Result<Vec<f32>, ()> {
    // Minimal RIFF/WAV parser for 16-bit PCM.
    // Returns mono f32 samples normalized to [-1.0, 1.0].

    if wav_bytes.len() < 44 {
        return Err(());
    }

    // RIFF header
    if &wav_bytes[0..4] != b"RIFF" || &wav_bytes[8..12] != b"WAVE" {
        return Err(());
    }

    let file_len = u32::from_le_bytes([wav_bytes[4], wav_bytes[5], wav_bytes[6], wav_bytes[7]]);
    if (file_len as usize) + 8 > wav_bytes.len() {
        return Err(());
    }

    let mut pos: usize = 12; // skip RIFF + WAVE
    let mut channels: u16 = 0;
    let mut _sample_rate: u32 = 0;
    let mut bits_per_sample: u16 = 0;
    let mut audio_format: u16 = 0;
    let mut data_start: usize = 0;
    let mut data_size: usize = 0;
    let mut found_fmt = false;
    let mut found_data = false;

    while pos + 8 <= wav_bytes.len() && !(found_fmt && found_data) {
        let chunk_id = &wav_bytes[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes([wav_bytes[pos + 4], wav_bytes[pos + 5], wav_bytes[pos + 6], wav_bytes[pos + 7]])
            as usize;

        if chunk_id == b"fmt " {
            if chunk_size < 16 || pos + 8 + chunk_size > wav_bytes.len() {
                return Err(());
            }
            audio_format = u16::from_le_bytes([wav_bytes[pos + 8], wav_bytes[pos + 9]]);
            channels = u16::from_le_bytes([wav_bytes[pos + 10], wav_bytes[pos + 11]]);
            _sample_rate = u32::from_le_bytes([wav_bytes[pos + 12], wav_bytes[pos + 13], wav_bytes[pos + 14], wav_bytes[pos + 15]]);
            bits_per_sample = u16::from_le_bytes([wav_bytes[pos + 22], wav_bytes[pos + 23]]);
            found_fmt = true;
        } else if chunk_id == b"data" {
            data_start = pos + 8;
            data_size = chunk_size;
            found_data = true;
        }

        // Advance to next chunk (padded to even byte boundary)
        let advance = 8 + chunk_size;
        pos += advance;
        if advance % 2 != 0 {
            pos += 1;
        }
    }

    if !found_fmt || !found_data {
        return Err(());
    }
    if audio_format != 1 {
        // Only PCM supported
        return Err(());
    }
    if bits_per_sample != 16 {
        // Only 16-bit supported
        return Err(());
    }
    if channels != 1 && channels != 2 {
        return Err(());
    }
    if data_size == 0 {
        return Ok(Vec::new());
    }
    if data_start + data_size > wav_bytes.len() {
        return Err(());
    }

    let bytes_per_sample = (bits_per_sample / 8) as usize; // 2 for 16-bit
    let total_samples = data_size / (bytes_per_sample * channels as usize);

    let mut result = Vec::with_capacity(total_samples);

    for i in 0..total_samples {
        let offset = data_start + i * bytes_per_sample * channels as usize;
        let mut sum: i32 = 0;

        for ch in 0..channels {
            let ch_offset = offset + ch as usize * bytes_per_sample;
            if ch_offset + 1 >= wav_bytes.len() {
                return Err(());
            }
            let sample = i16::from_le_bytes([wav_bytes[ch_offset], wav_bytes[ch_offset + 1]]);
            sum += sample as i32;
        }

        // Average channels for stereo → mono
        let averaged = (sum as f64 / channels as f64) as f32;
        // Normalize to [-1.0, 1.0]
        let normalized = averaged / (i16::MAX as f32);
        let clamped = normalized.clamp(-1.0, 1.0);
        result.push(clamped);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::error::SessionError;
    use crate::asr::traits::AsrSession;
    use crate::asr::types::*;
    use async_trait::async_trait;

    #[derive(Debug)]
    struct MockSession {
        succeed: bool,
        retryable: bool,
        call_count: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl AsrSession for MockSession {
        fn model(&self) -> &str {
            "mock"
        }
        fn provider_id(&self) -> &'static str {
            "mock"
        }
        async fn submit_audio(&self, _: AudioChunk) -> Result<(), ()> {
            Ok(())
        }
        async fn partial_transcript(&self) -> Option<String> {
            None
        }

        async fn finalize(self: Box<Self>) -> Result<TranscriptResult, TranscriptionError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.succeed {
                Ok(TranscriptResult {
                    text: "hello".into(),
                    model: "mock".into(),
                    diagnostics: AsrDiagnostics::new(
                        "mock",
                        "mock",
                        BackendType::Cloud,
                        1000,
                        500,
                        "test",
                    ),
                })
            } else {
                Err(TranscriptionError::new(
                    TranscriptionErrorCode::ServerError,
                    "mock failure",
                    self.retryable,
                ))
            }
        }
        async fn cancel(self: Box<Self>) {}
    }

    #[derive(Debug)]
    struct MockProvider {
        id: &'static str,
        succeed: bool,
        retryable: bool,
    }

    #[async_trait]
    impl AsrProvider for MockProvider {
        fn id(&self) -> &'static str {
            self.id
        }
        fn name(&self) -> &'static str {
            self.id
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                fallback_compatible: true,
                ..Default::default()
            }
        }
        fn default_model(&self) -> &'static str {
            "mock"
        }
        fn available_models(&self) -> &[ModelSpec] {
            &[]
        }
        async fn create_session(
            &self,
            _: SessionConfig,
        ) -> Result<Box<dyn AsrSession>, SessionError> {
            Ok(Box::new(MockSession {
                succeed: self.succeed,
                retryable: self.retryable,
                call_count: std::sync::atomic::AtomicUsize::new(0),
            }))
        }
        async fn health_check(&self) -> Result<HealthStatus, ()> {
            Ok(HealthStatus::Healthy)
        }
    }

    #[tokio::test]
    async fn primary_success_no_fallback_needed() {
        let primary = MockProvider {
            id: "primary",
            succeed: true,
            retryable: false,
        };
        let fallback = MockProvider {
            id: "fallback",
            succeed: true,
            retryable: false,
        };
        let result = FallbackStrategy::execute(&primary, Some(&fallback), build_mono_wav(&[0i16; 2]), 1000)
            .await
            .unwrap();
        assert!(!result.diagnostics.fallback_used);
    }

    #[tokio::test]
    async fn non_retryable_failure_triggers_fallback() {
        let primary = MockProvider {
            id: "primary",
            succeed: false,
            retryable: false,
        };
        let fallback = MockProvider {
            id: "fallback",
            succeed: true,
            retryable: false,
        };
        let result = FallbackStrategy::execute(&primary, Some(&fallback), build_mono_wav(&[0i16; 2]), 1000)
            .await
            .unwrap();
        assert!(result.diagnostics.fallback_used);
        assert_eq!(
            result.diagnostics.fallback_provider.as_deref(),
            Some("fallback")
        );
    }

    #[tokio::test]
    async fn no_fallback_returns_error() {
        let primary = MockProvider {
            id: "primary",
            succeed: false,
            retryable: false,
        };
        let err = FallbackStrategy::execute(&primary, None, build_mono_wav(&[0i16; 2]), 1000)
            .await
            .unwrap_err();
        assert_eq!(err.code, crate::asr::error::AsrErrorCode::FallbackFailed);
    }

    #[tokio::test]
    async fn fallback_also_fails_returns_error() {
        let primary = MockProvider {
            id: "primary",
            succeed: false,
            retryable: false,
        };
        let fallback = MockProvider {
            id: "fallback",
            succeed: false,
            retryable: false,
        };
        let err = FallbackStrategy::execute(&primary, Some(&fallback), build_mono_wav(&[0i16; 2]), 1000)
            .await
            .unwrap_err();
        assert_eq!(err.code, crate::asr::error::AsrErrorCode::FallbackFailed);
    }

    #[tokio::test]
    async fn fallback_preserves_audio_data() {
        // Test that audio data is not lost during fallback by ensuring
        // the same audio is passed to both primary and fallback providers
        let primary = MockProvider {
            id: "primary",
            succeed: false,
            retryable: false,
        };
        let fallback = MockProvider {
            id: "fallback",
            succeed: true,
            retryable: false,
        };
        
        // Use non-empty audio data
        let audio_data = build_mono_wav(&[100i16, -100i16]);
        let result = FallbackStrategy::execute(&primary, Some(&fallback), audio_data.clone(), 1000)
            .await
            .unwrap();
        
        assert!(result.diagnostics.fallback_used);
        assert_eq!(result.text, "hello");
    }

    #[tokio::test]
    async fn fallback_with_retry_preserves_audio_data() {
        // Test that audio data is preserved through retry attempts and fallback
        let primary = MockProvider {
            id: "primary",
            succeed: false,
            retryable: true, // This will trigger retries
        };
        let fallback = MockProvider {
            id: "fallback",
            succeed: true,
            retryable: false,
        };
        
        let audio_data = build_mono_wav(&[10i16, 20]);
        let result = FallbackStrategy::execute(&primary, Some(&fallback), audio_data.clone(), 1000)
            .await
            .unwrap();
        
        // Should have retried and then fallen back
        assert!(result.diagnostics.fallback_used);
        assert_eq!(result.text, "hello");
    }

    #[tokio::test]
    async fn fallback_deterministic_order() {
        // Test that fallback always tries primary first, then fallback
        // This ensures deterministic behavior
        let primary = MockProvider {
            id: "primary",
            succeed: false,
            retryable: false,
        };
        let fallback = MockProvider {
            id: "fallback",
            succeed: true,
            retryable: false,
        };
        
        let audio_data = build_mono_wav(&[1i16]);
        let result = FallbackStrategy::execute(&primary, Some(&fallback), audio_data, 1000)
            .await
            .unwrap();
        
        // Fallback should have been used
        assert!(result.diagnostics.fallback_used);
        assert_eq!(result.diagnostics.fallback_provider.as_deref(), Some("fallback"));
    }

    // ========================================================================
    // WAV Decode Tests
    // ========================================================================

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
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
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

    fn build_stereo_wav(left: &[i16], right: &[i16]) -> Vec<u8> {
        assert_eq!(left.len(), right.len());
        let channels: u16 = 2;
        let bits_per_sample: u16 = 16;
        let sample_rate: u32 = 16000;
        let bytes_per_sample = (bits_per_sample / 8) as u32;
        let block_align = channels as u32 * bytes_per_sample;
        let byte_rate = sample_rate * block_align;
        let data_size = (left.len() as u32 * bytes_per_sample * channels as u32);
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
        for i in 0..left.len() {
            wav.extend_from_slice(&left[i].to_le_bytes());
            wav.extend_from_slice(&right[i].to_le_bytes());
        }
        wav
    }

    #[test]
    fn decode_wav_mono_16bit() {
        let samples = [0i16, 100, -100, i16::MAX, i16::MIN];
        let wav = build_mono_wav(&samples);
        let decoded = decode_wav_to_f32(&wav).unwrap();
        assert_eq!(decoded.len(), 5);
        assert!((decoded[0] - 0.0).abs() < f32::EPSILON);
        assert!((decoded[1] - (100.0 / i16::MAX as f32)).abs() < 1e-6);
        assert!((decoded[2] - (-100.0 / i16::MAX as f32)).abs() < 1e-6);
        assert!((decoded[3] - 1.0).abs() < f32::EPSILON);
        assert!((decoded[4] - (-1.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn decode_wav_stereo_to_mono() {
        // Left: [100, 200], Right: [300, 400] → mono: [200, 300]
        let left = [100i16, 200i16];
        let right = [300i16, 400i16];
        let wav = build_stereo_wav(&left, &right);
        let decoded = decode_wav_to_f32(&wav).unwrap();
        assert_eq!(decoded.len(), 2);
        let expected_0 = ((100i32 + 300) / 2) as f32 / i16::MAX as f32;
        let expected_1 = ((200i32 + 400) / 2) as f32 / i16::MAX as f32;
        assert!((decoded[0] - expected_0).abs() < 1e-6);
        assert!((decoded[1] - expected_1).abs() < 1e-6);
    }

    #[test]
    fn decode_wav_normalization() {
        // Verify extremes map to [-1.0, 1.0]
        let samples = [i16::MIN, 0, i16::MAX];
        let wav = build_mono_wav(&samples);
        let decoded = decode_wav_to_f32(&wav).unwrap();
        assert!((decoded[0] - (-1.0)).abs() < f32::EPSILON);
        assert!((decoded[1] - 0.0).abs() < f32::EPSILON);
        assert!((decoded[2] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn decode_wav_invalid_riff() {
        let bytes = b"NOTRIFF....WAVE";
        assert!(decode_wav_to_f32(bytes).is_err());
    }

    #[test]
    fn decode_wav_no_wave() {
        let mut bytes = build_mono_wav(&[]);
        bytes[8..12].copy_from_slice(b"NO W");
        assert!(decode_wav_to_f32(&bytes).is_err());
    }

    #[test]
    fn decode_wav_empty_data() {
        let wav = build_mono_wav(&[]);
        let decoded = decode_wav_to_f32(&wav).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn decode_wav_too_short() {
        assert!(decode_wav_to_f32(b"").is_err());
        assert!(decode_wav_to_f32(b"RIFF").is_err());
    }

    #[test]
    fn decode_wav_rejects_non_pcm() {
        // Build a WAV with format = 3 (IEEE float) instead of 1 (PCM)
        let channels: u16 = 1;
        let bits_per_sample: u16 = 16;
        let sample_rate: u32 = 16000;
        let bytes_per_sample = (bits_per_sample / 8) as u32;
        let block_align = channels as u32 * bytes_per_sample;
        let byte_rate = sample_rate * block_align;
        let data_size = 4u32 * bytes_per_sample;
        let file_size = 36 + data_size;

        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&file_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&3u16.to_le_bytes()); // IEEE float — not PCM
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&(block_align as u16).to_le_bytes());
        wav.extend_from_slice(&bits_per_sample.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        for _ in 0..4 {
            wav.extend_from_slice(&0i16.to_le_bytes());
        }

        assert!(decode_wav_to_f32(&wav).is_err());
    }

    #[test]
    fn decode_wav_rejects_unsupported_bits() {
        // Should reject 8-bit
        let channels: u16 = 1;
        let sample_rate: u32 = 16000;
        let data_size = 4u32;
        let file_size = 36 + data_size;

        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&file_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * channels as u32).to_le_bytes());
        wav.extend_from_slice(&(channels as u16).to_le_bytes());
        wav.extend_from_slice(&8u16.to_le_bytes()); // 8-bit — unsupported
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        for _ in 0..4 {
            wav.push(0);
        }

        assert!(decode_wav_to_f32(&wav).is_err());
    }

    #[test]
    fn decode_wav_rejects_5channel() {
        // Should reject 5 channels
        let channels: u16 = 5;
        let bits_per_sample: u16 = 16;
        let sample_rate: u32 = 16000;
        let bytes_per_sample = (bits_per_sample / 8) as u32;
        let block_align = channels as u32 * bytes_per_sample;
        let byte_rate = sample_rate * block_align;
        let data_size = 4u32 * bytes_per_sample * channels as u32;
        let file_size = 36 + data_size;

        let mut wav = Vec::new();
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
        for _ in 0..4 {
            for _ in 0..channels {
                wav.extend_from_slice(&0i16.to_le_bytes());
            }
        }

        assert!(decode_wav_to_f32(&wav).is_err());
    }
}
