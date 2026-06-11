use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ort::session::Session;
use ort::value::Value;
use std::ops::Index;

use super::onnx_runtime::SessionCache;
use crate::asr::error::SessionError;
use crate::asr::traits::{AsrProvider, AsrSession, TranscriptionError, TranscriptionErrorCode};
use crate::asr::types::{
    AsrDiagnostics, AudioChunk, BackendType, Deployment, HealthStatus, ModelSpec,
    ProviderCapabilities, SessionConfig, StreamingSupport, TranscriptResult,
};

const SOT_TOKEN: i64 = 50258;
const EOT_TOKEN: i64 = 50257;
const ENGLISH_TOKEN: i64 = 50259;
const TRANSCRIBE_TOKEN: i64 = 50359;
const MAX_DECODE_STEPS: usize = 448;
const N_MELS: usize = 80;
const N_FFT: usize = 400;
const HOP_LENGTH: usize = 160;
const CHUNK_LENGTH: usize = 3000;
const SAMPLE_RATE: u32 = 16000;

const PROVIDER_ID: &str = "local_whisper";
const PROVIDER_NAME: &str = "Whisper ONNX (Local)";
const DEFAULT_MODEL: &str = "whisper-tiny";

#[derive(Debug)]
pub struct WhisperOnnxProvider {
    session_cache: Arc<SessionCache>,
}

impl WhisperOnnxProvider {
    pub fn new(session_cache: Arc<SessionCache>) -> Self {
        Self { session_cache }
    }
}

const ONNX_MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "whisper-tiny",
        name: "Whisper Tiny (39M)",
        requires_gpu: false,
        max_duration_secs: 30,
        supported_languages: None,
        parameters: Some("39M"),
    },
    ModelSpec {
        id: "whisper-base",
        name: "Whisper Base (74M)",
        requires_gpu: false,
        max_duration_secs: 30,
        supported_languages: None,
        parameters: Some("74M"),
    },
];

#[async_trait]
impl AsrProvider for WhisperOnnxProvider {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            backend_type: BackendType::Onnx,
            deployment: Deployment::Local,
            streaming: StreamingSupport::None,
            partials: false,
            timestamps: false,
            gpu_required: false,
            fallback_compatible: true,
            max_audio_seconds: 30,
            supported_sample_rates: vec![SAMPLE_RATE],
            min_audio_bytes: 1,
            max_audio_bytes: 25_000_000,
        }
    }

    fn default_model(&self) -> &'static str {
        DEFAULT_MODEL
    }

    fn available_models(&self) -> &[ModelSpec] {
        ONNX_MODELS
    }

    async fn create_session(
        &self,
        config: SessionConfig,
    ) -> Result<Box<dyn AsrSession>, SessionError> {
        let model_name = config.model.as_deref().unwrap_or(DEFAULT_MODEL);
        let cached = self.session_cache.get_or_load(model_name)?;

        let encoder =
            SessionCache::load_encoder_session(&cached)?;
        let decoder =
            SessionCache::load_decoder_session(&cached)?;
        let tokenizer =
            SessionCache::load_tokenizer(&cached)?;

        Ok(Box::new(WhisperOnnxSession::new(
            encoder,
            decoder,
            tokenizer,
            model_name.to_string(),
        )))
    }

    async fn health_check(&self) -> Result<HealthStatus, ()> {
        if self.session_cache.has_model(DEFAULT_MODEL) {
            Ok(HealthStatus::Healthy)
        } else {
            Ok(HealthStatus::Unhealthy(
                "no ONNX Whisper models found; place models in the models/ directory".into(),
            ))
        }
    }
}

#[derive(Debug)]
pub struct WhisperOnnxSession {
    encoder: Mutex<Session>,
    decoder: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    model_name: String,
    audio_data: Mutex<Vec<f32>>,
    sample_rate: AtomicU32,
}

impl WhisperOnnxSession {
    pub fn new(
        encoder: Session,
        decoder: Session,
        tokenizer: tokenizers::Tokenizer,
        model_name: String,
    ) -> Self {
        Self {
            encoder: Mutex::new(encoder),
            decoder: Mutex::new(decoder),
            tokenizer,
            model_name,
            audio_data: Mutex::new(Vec::new()),
            sample_rate: AtomicU32::new(SAMPLE_RATE),
        }
    }
}

#[async_trait]
impl AsrSession for WhisperOnnxSession {
    fn model(&self) -> &str {
        &self.model_name
    }

    fn provider_id(&self) -> &'static str {
        PROVIDER_ID
    }

    async fn submit_audio(&self, chunk: AudioChunk) -> Result<(), ()> {
        self.sample_rate.store(chunk.sample_rate, Ordering::SeqCst);
        let mut data = self.audio_data.lock().map_err(|_| ())?;
        data.extend_from_slice(&chunk.data);
        Ok(())
    }

    async fn partial_transcript(&self) -> Option<String> {
        None
    }

    async fn finalize(self: Box<Self>) -> Result<TranscriptResult, TranscriptionError> {
        let samples = {
            let mut data = self.audio_data.lock().map_err(|_| {
                TranscriptionError::new(
                    TranscriptionErrorCode::Internal,
                    "failed to lock audio buffer",
                    false,
                )
            })?;
            std::mem::take(&mut *data)
        };

        if samples.is_empty() {
            return Err(TranscriptionError::new(
                TranscriptionErrorCode::InvalidRequest,
                "no audio data to transcribe",
                false,
            ));
        }

        let started = std::time::Instant::now();

        let sample_rate = self.sample_rate.load(Ordering::SeqCst).max(SAMPLE_RATE);
        let audio = if sample_rate != SAMPLE_RATE {
            resample(&samples, sample_rate, SAMPLE_RATE)
        } else {
            samples
        };

        let mel = compute_log_mel_spectrogram(&audio).map_err(|e| {
            TranscriptionError::new(
                TranscriptionErrorCode::Internal,
                format!("audio preprocessing failed: {}", e),
                false,
            )
        })?;

        let token_ids = {
            let mut encoder = self.encoder.lock().map_err(|_| {
                TranscriptionError::new(
                    TranscriptionErrorCode::Internal,
                    "failed to lock encoder session",
                    false,
                )
            })?;
            let mut decoder = self.decoder.lock().map_err(|_| {
                TranscriptionError::new(
                    TranscriptionErrorCode::Internal,
                    "failed to lock decoder session",
                    false,
                )
            })?;

            let encoder_output = run_encoder(&mut encoder, &mel).map_err(|e| {
                TranscriptionError::new(
                    TranscriptionErrorCode::Internal,
                    format!("encoder inference failed: {}", e),
                    true,
                )
            })?;

            run_decoder(
                &mut decoder,
                &encoder_output,
            )
            .map_err(|e| {
                TranscriptionError::new(
                    TranscriptionErrorCode::Internal,
                    format!("decoder inference failed: {}", e),
                    true,
                )
            })?
        };

        let text = decode_token_ids(&self.tokenizer, &token_ids);
        let transcription_ms = started.elapsed().as_millis() as u64;

        Ok(TranscriptResult {
            text,
            model: self.model_name.clone(),
            diagnostics: AsrDiagnostics::new(
                PROVIDER_ID,
                &self.model_name,
                BackendType::Onnx,
                0,
                transcription_ms,
                "",
            ),
        })
    }

    async fn cancel(self: Box<Self>) {}
}

// ---------------------------------------------------------------------------
// Audio preprocessing: Whisper-style log-Mel spectrogram
// ---------------------------------------------------------------------------

fn resample(audio: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return audio.to_vec();
    }
    let ratio = to_rate as f64 / from_rate as f64;
    let output_len = (audio.len() as f64 * ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_len);
    for i in 0..output_len {
        let src_idx = i as f64 / ratio;
        let lo = src_idx.floor() as usize;
        let hi = (lo + 1).min(audio.len().saturating_sub(1));
        let frac = src_idx - lo as f64;
        let sample = audio[lo] as f64 * (1.0 - frac) + audio[hi] as f64 * frac;
        output.push(sample as f32);
    }
    output
}

fn compute_log_mel_spectrogram(audio: &[f32]) -> Result<Vec<f32>, String> {
    let target_len = CHUNK_LENGTH * HOP_LENGTH;
    let pcm = if audio.len() < target_len {
        let mut padded = audio.to_vec();
        padded.resize(target_len, 0.0);
        padded
    } else {
        audio[..target_len].to_vec()
    };

    let n_frames = 1 + (pcm.len() - N_FFT) / HOP_LENGTH;
    if n_frames < 1 {
        return Err("audio too short for spectrogram".into());
    }

    let mut stft_mag = vec![0.0_f32; n_frames * (N_FFT / 2 + 1)];

    let mut planner = rustfft::FftPlanner::new();
    let fft = planner.plan_fft_forward(N_FFT);
    let hann = hann_window(N_FFT);

    for frame in 0..n_frames {
        let start = frame * HOP_LENGTH;
        let mut complex_buf: Vec<rustfft::num_complex::Complex<f32>> = (0..N_FFT)
            .map(|i| {
                let idx = start + i;
                let windowed = if idx < pcm.len() {
                    pcm[idx] * hann[i]
                } else {
                    0.0
                };
                rustfft::num_complex::Complex::new(windowed, 0.0)
            })
            .collect();

        fft.process(&mut complex_buf);

        let out_start = frame * (N_FFT / 2 + 1);
        for i in 0..(N_FFT / 2 + 1) {
            stft_mag[out_start + i] = complex_buf[i].norm();
        }
    }

    let mel_matrix = mel_filterbank(
        SAMPLE_RATE as f64,
        N_FFT,
        N_MELS,
        0.0,
        SAMPLE_RATE as f64 / 2.0,
    );

    let n_freqs = N_FFT / 2 + 1;
    let mut mel_spec = vec![0.0_f32; n_frames * N_MELS];
    for t in 0..n_frames {
        let frame_start = t * n_freqs;
        let mel_start = t * N_MELS;
        for m in 0..N_MELS {
            let mut sum = 0.0_f32;
            for k in 0..n_freqs {
                sum += stft_mag[frame_start + k] * mel_matrix[m * n_freqs + k];
            }
            mel_spec[mel_start + m] = sum;
        }
    }

    let target_frames = CHUNK_LENGTH;
    let mut output = vec![0.0_f32; target_frames * N_MELS];

    if n_frames >= target_frames {
        for t in 0..target_frames {
            for m in 0..N_MELS {
                let val = mel_spec[t * N_MELS + m].max(1e-10);
                output[t * N_MELS + m] = val;
            }
        }
    } else {
        for t in 0..n_frames {
            for m in 0..N_MELS {
                let val = mel_spec[t * N_MELS + m].max(1e-10);
                output[t * N_MELS + m] = val;
            }
        }
        for t in n_frames..target_frames {
            for m in 0..N_MELS {
                output[t * N_MELS + m] = 1e-10;
            }
        }
    }

    Ok(output)
}

fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| {
            0.5 * (1.0
                - (2.0 * std::f32::consts::PI * i as f32 / (size as f32 - 1.0)).cos())
        })
        .collect()
}

fn mel_filterbank(sample_rate: f64, n_fft: usize, n_mels: usize, f_min: f64, f_max: f64) -> Vec<f32> {
    let n_freqs = n_fft / 2 + 1;
    let mut filterbank = vec![0.0_f32; n_mels * n_freqs];

    let mel_min = hz_to_mel(f_min);
    let mel_max = hz_to_mel(f_max);
    let mel_spacing = (mel_max - mel_min) / (n_mels as f64 + 1.0);

    let mel_centers_hz: Vec<f64> = (0..n_mels + 2)
        .map(|i| mel_to_hz(mel_min + i as f64 * mel_spacing))
        .collect();

    let fft_bins: Vec<f64> = mel_centers_hz
        .iter()
        .map(|&hz| hz / sample_rate * n_fft as f64)
        .collect();

    for m in 0..n_mels {
        let left = fft_bins[m];
        let center = fft_bins[m + 1];
        let right = fft_bins[m + 2];

        for k in 0..n_freqs {
            let kf = k as f64;
            let weight = if kf >= left && kf <= center {
                (kf - left) / (center - left)
            } else if kf > center && kf <= right {
                (right - kf) / (right - center)
            } else {
                0.0
            };
            filterbank[m * n_freqs + k] = weight as f32;
        }
    }

    filterbank
}

fn hz_to_mel(hz: f64) -> f64 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f64) -> f64 {
    700.0 * (10.0_f64.powf(mel / 2595.0) - 1.0)
}

// ---------------------------------------------------------------------------
// Encoder / Decoder inference (ONNX Runtime)
// ---------------------------------------------------------------------------

fn run_encoder(encoder: &mut Session, mel: &[f32]) -> Result<Vec<f32>, String> {
    let shape = [1usize, N_MELS, CHUNK_LENGTH];
    let input_tensor = Value::from_array((shape, mel.to_vec()))
        .map_err(|e| format!("encoder input tensor: {}", e))?;

    let input_name = encoder.inputs().first()
        .map(|o| o.name().to_string())
        .unwrap_or_else(|| "input_features".to_string());

    let outputs = encoder
        .run(ort::inputs! { input_name.as_str() => input_tensor })
        .map_err(|e| format!("encoder run: {}", e))?;

    let output = outputs.index(0);

    let (_shape, data) = output.try_extract_tensor::<f32>()
        .map_err(|e| format!("encoder output view: {}", e))?;

    Ok(data.to_vec())
}

fn run_decoder(
    decoder: &mut Session,
    encoder_output: &[f32],
) -> Result<Vec<i64>, String> {
    let enc_seq_len = CHUNK_LENGTH / 2;
    let d_model = encoder_output.len() / enc_seq_len;
    let enc_shape = [1usize, enc_seq_len, d_model];

    let encoder_tensor = Value::from_array((enc_shape, encoder_output.to_vec()))
        .map_err(|e| format!("decoder encoder tensor: {}", e))?;

    let vocab_size: usize = 51865;
    let mut tokens: Vec<i64> = vec![SOT_TOKEN, ENGLISH_TOKEN, TRANSCRIBE_TOKEN];

    for _ in 0..MAX_DECODE_STEPS {
        let seq_len = tokens.len();
        let input_ids_tensor =
            Value::from_array(([1usize, seq_len], tokens.clone()))
                .map_err(|e| format!("decoder input tensor: {}", e))?;

        let input_names: Vec<String> = decoder
            .inputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();

        let mut input_map: std::collections::HashMap<String, &str> =
            std::collections::HashMap::new();

        for name in &input_names {
            let lower = name.to_lowercase();
            if lower.contains("input_ids") || lower.contains("inputid") {
                input_map.insert(name.clone(), "input_ids");
            } else if lower.contains("encoder") || lower.contains("encoderhidden") {
                input_map.insert(name.clone(), "encoder_output");
            }
        }

        let outputs = if input_map.len() == 2 {
            let ids_name = input_map.iter()
                .find(|(_, v)| **v == "input_ids")
                .map(|(k, _)| k.as_str())
                .unwrap_or("input_ids");
            let enc_name = input_map.iter()
                .find(|(_, v)| **v == "encoder_output")
                .map(|(k, _)| k.as_str())
                .unwrap_or("encoder_hidden_states");

            decoder
                .run(ort::inputs! {
                    ids_name => input_ids_tensor,
                    enc_name => &encoder_tensor,
                })
                .map_err(|e| format!("decoder run: {}", e))?
        } else {
            decoder
                .run(ort::inputs![input_ids_tensor, &encoder_tensor])
                .map_err(|e| format!("decoder run: {}", e))?
        };

        let logits_output = outputs.index(0);

        let (_shape, logits) = logits_output.try_extract_tensor::<f32>()
            .map_err(|e| format!("decoder logits view: {}", e))?;

        let last_pos = seq_len - 1;
        let offset = last_pos * vocab_size;
        let next_token = (0..vocab_size)
            .map(|i| logits[offset + i])
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| idx as i64)
            .unwrap_or(EOT_TOKEN);

        if next_token == EOT_TOKEN {
            break;
        }

        tokens.push(next_token);
    }

    Ok(tokens[3..].to_vec())
}

// ---------------------------------------------------------------------------
// Token decoding
// ---------------------------------------------------------------------------

fn decode_token_ids(tokenizer: &tokenizers::Tokenizer, token_ids: &[i64]) -> String {
    if token_ids.is_empty() {
        return String::new();
    }

    let ids: Vec<u32> = token_ids.iter().filter_map(|&id| id.try_into().ok()).collect();
    match tokenizer.decode(&ids, true) {
        Ok(text) => text,
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::error::SessionErrorCode;
    use std::path::PathBuf;

    #[tokio::test]
    async fn provider_identity_and_capabilities() {
        let cache = Arc::new(SessionCache::new(PathBuf::from("/nonexistent")));
        let provider = WhisperOnnxProvider::new(cache);

        assert_eq!(provider.id(), PROVIDER_ID);
        assert_eq!(provider.name(), PROVIDER_NAME);
        assert_eq!(provider.default_model(), DEFAULT_MODEL);
    }

    #[tokio::test]
    async fn capabilities_are_local_onnx() {
        let cache = Arc::new(SessionCache::new(PathBuf::from("/nonexistent")));
        let provider = WhisperOnnxProvider::new(cache);
        let caps = provider.capabilities();

        assert_eq!(caps.backend_type, BackendType::Onnx);
        assert_eq!(caps.deployment, Deployment::Local);
        assert!(caps.fallback_compatible);
        assert!(!caps.gpu_required);
        assert_eq!(caps.max_audio_seconds, 30);
        assert!(caps.supported_sample_rates.contains(&16000));
    }

    #[tokio::test]
    async fn health_check_reports_unhealthy_when_no_models() {
        let cache = Arc::new(SessionCache::new(PathBuf::from("/nonexistent/path")));
        let provider = WhisperOnnxProvider::new(cache);
        let status = provider.health_check().await.unwrap();
        assert!(matches!(status, HealthStatus::Unhealthy(_)));
    }

    #[tokio::test]
    async fn create_session_fails_without_model_files() {
        let cache = Arc::new(SessionCache::new(PathBuf::from("/nonexistent")));
        let provider = WhisperOnnxProvider::new(cache);

        let config = SessionConfig {
            model: Some("whisper-tiny".into()),
            sample_rate: 16000,
            max_duration_secs: 30,
        };

        let result = provider.create_session(config).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, SessionErrorCode::ModelUnavailable);
    }

    #[tokio::test]
    async fn provider_models_include_whisper_tiny() {
        let cache = Arc::new(SessionCache::new(PathBuf::from("/nonexistent")));
        let provider = WhisperOnnxProvider::new(cache);
        let models = provider.available_models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "whisper-tiny"));
        assert_eq!(provider.default_model(), "whisper-tiny");
    }

    #[test]
    fn resample_identity_same_rate() {
        let audio = vec![0.0, 0.5, 1.0];
        let result = resample(&audio, 16000, 16000);
        assert_eq!(result, audio);
    }

    #[test]
    fn resample_downsample() {
        let audio: Vec<f32> = (0..160).map(|i| i as f32 / 160.0).collect();
        let result = resample(&audio, 16000, 8000);
        assert!(result.len() < audio.len());
        assert!(!result.is_empty());
    }

    #[test]
    fn resample_empty_input() {
        let audio: Vec<f32> = vec![];
        let result = resample(&audio, 48000, 16000);
        assert!(result.is_empty());
    }

    #[test]
    fn log_mel_spectrogram_pads_very_short_audio() {
        let short = vec![0.0; 100];
        let result = compute_log_mel_spectrogram(&short);
        assert!(result.is_ok());
    }

    #[test]
    fn log_mel_spectrogram_produces_correct_shape_for_30s() {
        let audio = vec![0.01; 480000];
        let result = compute_log_mel_spectrogram(&audio).unwrap();
        assert_eq!(result.len(), N_MELS * CHUNK_LENGTH);
    }

    #[test]
    fn log_mel_spectrogram_pads_short_audio() {
        let audio = vec![0.01; 16000];
        let result = compute_log_mel_spectrogram(&audio).unwrap();
        assert_eq!(result.len(), N_MELS * CHUNK_LENGTH);
    }

    #[test]
    fn log_mel_values_are_positive() {
        let audio = vec![0.01; 480000];
        let result = compute_log_mel_spectrogram(&audio).unwrap();
        for &v in &result {
            assert!(v > 0.0, "mel value should be positive, got {}", v);
        }
    }

    #[test]
    fn hann_window_is_correct_length() {
        let window = hann_window(N_FFT);
        assert_eq!(window.len(), N_FFT);
        assert!((window[0] - 0.0).abs() < 1e-6);
        assert!((window[N_FFT / 2] - 1.0).abs() < 1e-4);
        assert!((window[N_FFT - 1] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn mel_filterbank_produces_correct_shape() {
        let fb = mel_filterbank(16000.0, N_FFT, N_MELS, 0.0, 8000.0);
        assert_eq!(fb.len(), N_MELS * (N_FFT / 2 + 1));
    }

    #[test]
    fn mel_filterbank_non_negative_weights() {
        let fb = mel_filterbank(16000.0, N_FFT, N_MELS, 0.0, 8000.0);
        for &w in &fb {
            assert!(w >= 0.0, "weight {:.6} is negative", w);
        }
    }

    #[test]
    fn hz_to_mel_conversion_is_consistent() {
        let m = hz_to_mel(1000.0);
        let h = mel_to_hz(m);
        assert!((h - 1000.0).abs() < 1.0, "hz->mel->hz roundtrip: {}", h);
    }
}
