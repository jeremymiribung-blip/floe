use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tract_onnx::prelude::*;

use super::model_cache::{ModelCache, ModelPaths};
use crate::asr::error::SessionError;
use crate::asr::traits::{AsrProvider, AsrSession, TranscriptionError, TranscriptionErrorCode};
use crate::asr::types::{
    AsrDiagnostics, AudioChunk, BackendType, Deployment, HealthStatus, ModelSpec,
    ProviderCapabilities, SessionConfig, StreamingSupport, TranscriptResult,
};

const SOT_TOKEN: i64 = 50258;
const EOT_TOKEN: i64 = 50257;
const MAX_DECODE_STEPS: usize = 448;
const N_MELS: usize = 80;
const N_FFT: usize = 512;
const HOP_LENGTH: usize = 160;
const CHUNK_LENGTH: usize = 3000;
const SAMPLE_RATE: u32 = 16000;

const PROVIDER_ID: &str = "local_whisper";
const PROVIDER_NAME: &str = "Whisper ONNX (Local)";
const DEFAULT_MODEL: &str = "whisper-tiny";

#[derive(Debug)]
pub struct WhisperOnnxProvider {
    cache: Arc<ModelCache>,
}

impl WhisperOnnxProvider {
    pub fn new(cache: Arc<ModelCache>) -> Self {
        Self { cache }
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

type Plan = SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>;

fn load_plan(path: &std::path::Path) -> TractResult<Plan> {
    onnx()
        .model_for_path(path)?
        .into_optimized()?
        .into_runnable()
}

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
        let paths = self.cache.get_or_load(model_name).map_err(|e| {
            SessionError::new(
                crate::asr::error::SessionErrorCode::Internal,
                format!("ONNX model load failed: {}", e),
            )
        })?;

        Ok(Box::new(WhisperOnnxSession::new(paths)))
    }

    async fn health_check(&self) -> Result<HealthStatus, ()> {
        if self.cache.has_model(DEFAULT_MODEL) {
            Ok(HealthStatus::Healthy)
        } else if self.cache.model_dir().join(DEFAULT_MODEL).exists() {
            Ok(HealthStatus::Degraded(
                "model files present but not loaded".into(),
            ))
        } else {
            Ok(HealthStatus::Unhealthy(
                "no ONNX Whisper models found; place models in the models/ directory".into(),
            ))
        }
    }
}

#[derive(Debug)]
pub struct WhisperOnnxSession {
    paths: ModelPaths,
    audio_data: Mutex<Vec<f32>>,
    sample_rate: AtomicU32,
}

impl WhisperOnnxSession {
    pub fn new(paths: ModelPaths) -> Self {
        Self {
            paths,
            audio_data: Mutex::new(Vec::new()),
            sample_rate: AtomicU32::new(SAMPLE_RATE),
        }
    }
}

#[async_trait]
impl AsrSession for WhisperOnnxSession {
    fn model(&self) -> &str {
        &self.paths.model_name
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
                return TranscriptionError::new(
                    TranscriptionErrorCode::Internal,
                    "failed to lock audio buffer",
                    false,
                );
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

        let mel = compute_log_mel_spectrogram(&audio, SAMPLE_RATE).map_err(|e| {
            TranscriptionError::new(
                TranscriptionErrorCode::Internal,
                format!("audio preprocessing failed: {}", e),
                false,
            )
        })?;

        let encoder = load_plan(&self.paths.encoder_path).map_err(|e| {
            TranscriptionError::new(
                TranscriptionErrorCode::Internal,
                format!("encoder load failed: {}", e),
                true,
            )
        })?;

        let decoder = load_plan(&self.paths.decoder_path).map_err(|e| {
            TranscriptionError::new(
                TranscriptionErrorCode::Internal,
                format!("decoder load failed: {}", e),
                true,
            )
        })?;

        let encoder_output = run_encoder(&encoder, &mel).map_err(|e| {
            TranscriptionError::new(
                TranscriptionErrorCode::Internal,
                format!("encoder inference failed: {}", e),
                true,
            )
        })?;

        let tokens = run_decoder_greedy(&decoder, &encoder_output).map_err(|e| {
            TranscriptionError::new(
                TranscriptionErrorCode::Internal,
                format!("decoder inference failed: {}", e),
                true,
            )
        })?;

        let text = decode_token_ids(&tokens);
        let transcription_ms = started.elapsed().as_millis() as u64;

        Ok(TranscriptResult {
            text,
            model: self.paths.model_name.clone(),
            diagnostics: AsrDiagnostics::new(
                PROVIDER_ID,
                &self.paths.model_name,
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
// Audio preprocessing: log-Mel spectrogram
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

fn compute_log_mel_spectrogram(audio: &[f32], _sample_rate: u32) -> Result<Vec<f32>, String> {
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

    let mel_matrix =
        mel_filterbank(SAMPLE_RATE as f64, N_FFT, N_MELS, 80.0, SAMPLE_RATE as f64 / 2.0);

    let mut mel_spec = vec![0.0_f32; n_frames * N_MELS];
    for t in 0..n_frames {
        let frame_start = t * (N_FFT / 2 + 1);
        let mel_start = t * N_MELS;
        for m in 0..N_MELS {
            let mut sum = 0.0_f32;
            for k in 0..(N_FFT / 2 + 1) {
                sum += stft_mag[frame_start + k] * mel_matrix[m * (N_FFT / 2 + 1) + k];
            }
            mel_spec[mel_start + m] = sum.max(1e-10).ln();
        }
    }

    let target_frames = CHUNK_LENGTH;
    let mut output = vec![0.0_f32; target_frames * N_MELS];

    if n_frames >= target_frames {
        for t in 0..target_frames {
            for m in 0..N_MELS {
                output[t * N_MELS + m] = mel_spec[t * N_MELS + m].clamp(-10.0, 10.0);
            }
        }
    } else {
        for t in 0..n_frames {
            for m in 0..N_MELS {
                output[t * N_MELS + m] = mel_spec[t * N_MELS + m].clamp(-10.0, 10.0);
            }
        }
        for t in n_frames..target_frames {
            for m in 0..N_MELS {
                output[t * N_MELS + m] = -10.0;
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
// Encoder / Decoder inference (tract-onnx)
// ---------------------------------------------------------------------------

fn run_encoder(encoder: &Plan, mel: &[f32]) -> Result<Vec<f32>, String> {
    let shape = (1, N_MELS, CHUNK_LENGTH);
    let array = tract_ndarray::Array3::<f32>::from_shape_vec(shape, mel.to_vec())
        .map_err(|e| format!("encoder input shape: {}", e))?;

    let tensor = Tensor::from(array);
    let result = encoder
        .run(tvec!(tensor.into()))
        .map_err(|e| format!("encoder run: {}", e))?;

    if result.is_empty() {
        return Err("encoder produced no output".into());
    }

    let output = result[0]
        .to_array_view::<f32>()
        .map_err(|e| format!("encoder output view: {}", e))?;

    Ok(output.iter().copied().collect())
}

fn run_decoder_greedy(decoder: &Plan, encoder_output: &[f32]) -> Result<Vec<i64>, String> {
    let d_model = encoder_output.len() / (CHUNK_LENGTH / 2);
    let enc_shape = (1, CHUNK_LENGTH / 2, d_model);
    let encoder_array = tract_ndarray::Array3::<f32>::from_shape_vec(enc_shape, encoder_output.to_vec())
        .map_err(|e| format!("decoder encoder_input shape: {}", e))?;

    let encoder_tensor: Tensor = encoder_array.into();

    let mut tokens: Vec<i64> = vec![SOT_TOKEN, 50259, 50359];
    let max_steps = MAX_DECODE_STEPS;
    let vocab_size: usize = 51865;

    for _ in 0..max_steps {
        let seq_shape = (1, tokens.len());
        let input_array = tract_ndarray::Array2::<i64>::from_shape_vec(seq_shape, tokens.clone())
            .map_err(|e| format!("decoder input shape: {}", e))?;

        let input_tensor: Tensor = input_array.into();

        let result = decoder
            .run(tvec!(input_tensor.into(), encoder_tensor.clone().into()))
            .map_err(|e| format!("decoder step: {}", e))?;

        if result.is_empty() {
            break;
        }

        let logits = result[0]
            .to_array_view::<f32>()
            .map_err(|e| format!("decoder logits view: {}", e))?;

        let last_pos = tokens.len() - 1;
        let next_token = (0..vocab_size)
            .map(|i| logits[[0, last_pos, i]])
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
// BPE token decoding
// ---------------------------------------------------------------------------

fn decode_token_ids(token_ids: &[i64]) -> String {
    let mut bytes: Vec<u8> = Vec::new();
    for &id in token_ids {
        if id >= 0 && id < 256 {
            bytes.push(id as u8);
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::error::SessionErrorCode;
    use std::path::PathBuf;

    #[tokio::test]
    async fn provider_identity_and_capabilities() {
        let cache = Arc::new(ModelCache::new(PathBuf::from("/nonexistent")));
        let provider = WhisperOnnxProvider::new(cache);

        assert_eq!(provider.id(), PROVIDER_ID);
        assert_eq!(provider.name(), PROVIDER_NAME);
        assert_eq!(provider.default_model(), DEFAULT_MODEL);
    }

    #[tokio::test]
    async fn capabilities_are_local_onnx() {
        let cache = Arc::new(ModelCache::new(PathBuf::from("/nonexistent")));
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
        let cache = Arc::new(ModelCache::new(PathBuf::from("/nonexistent/path")));
        let provider = WhisperOnnxProvider::new(cache);
        let status = provider.health_check().await.unwrap();
        assert!(matches!(status, HealthStatus::Unhealthy(_)));
    }

    #[tokio::test]
    async fn create_session_fails_without_model_files() {
        let cache = Arc::new(ModelCache::new(PathBuf::from("/nonexistent")));
        let provider = WhisperOnnxProvider::new(cache);

        let config = SessionConfig {
            model: Some("whisper-tiny".into()),
            sample_rate: 16000,
            max_duration_secs: 30,
        };

        let result = provider.create_session(config).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, SessionErrorCode::Internal);
        assert!(err.message.contains("ONNX model load failed"));
    }

    #[tokio::test]
    async fn provider_models_include_whisper_tiny() {
        let cache = Arc::new(ModelCache::new(PathBuf::from("/nonexistent")));
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
        let result = compute_log_mel_spectrogram(&short, 16000);
        assert!(result.is_ok());
    }

    #[test]
    fn log_mel_spectrogram_produces_correct_shape_for_30s() {
        let audio = vec![0.01; 480000];
        let result = compute_log_mel_spectrogram(&audio, 16000).unwrap();
        assert_eq!(result.len(), N_MELS * CHUNK_LENGTH);
    }

    #[test]
    fn log_mel_spectrogram_pads_short_audio() {
        let audio = vec![0.01; 16000];
        let result = compute_log_mel_spectrogram(&audio, 16000).unwrap();
        assert_eq!(result.len(), N_MELS * CHUNK_LENGTH);
    }

    #[test]
    fn decode_token_ids_handles_ascii() {
        let ids: Vec<i64> = vec![72, 101, 108, 108, 111];
        let text = decode_token_ids(&ids);
        assert_eq!(text, "Hello");
    }

    #[test]
    fn decode_token_ids_handles_empty() {
        let ids: Vec<i64> = vec![];
        let text = decode_token_ids(&ids);
        assert_eq!(text, "");
    }

    #[test]
    fn decode_token_ids_skips_non_byte_tokens() {
        let ids: Vec<i64> = vec![72, 256, 101];
        let text = decode_token_ids(&ids);
        assert_eq!(text, "He");
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
        let fb = mel_filterbank(16000.0, N_FFT, N_MELS, 80.0, 8000.0);
        assert_eq!(fb.len(), N_MELS * (N_FFT / 2 + 1));
    }

    #[test]
    fn mel_filterbank_non_negative_weights() {
        let fb = mel_filterbank(16000.0, N_FFT, N_MELS, 80.0, 8000.0);
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
