use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;

use crate::audio::{fold_level, normalize_rms, LevelMeter, EMIT_INTERVAL_MS};

pub const MAX_RECORDING_DURATION_SECONDS: u64 = 120;
const OUTPUT_CHANNELS: u16 = 1;

type SharedBuffer = Arc<Mutex<RecordingBuffer>>;
type LevelEmitterFn = Box<dyn Fn(f32) + Send + Sync>;
type SharedLevelEmitter = Arc<Mutex<LevelEmitterFn>>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingInfo {
    pub sample_rate: u32,
    pub input_channels: u16,
    pub output_channels: u16,
    pub duration_ms: u64,
    pub sample_count: u64,
    pub wav_byte_count: u64,
    pub wav_bits_per_sample: u16,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub max_duration_reached: bool,
    pub ended_reason: RecordingEndReason,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub is_recording: bool,
    pub sample_rate: Option<u32>,
    pub input_channels: Option<u16>,
    pub output_channels: u16,
    pub duration_ms: u64,
    pub sample_count: u64,
    pub started_at_ms: Option<u64>,
    pub max_duration_seconds: u64,
    pub latest_recording: Option<RecordingInfo>,
    pub last_error: Option<RecordingError>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RecordingEndReason {
    Manual,
    MaxDuration,
    DeviceDisconnected,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecordingError {
    pub code: RecordingErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RecordingErrorCode {
    NoInputDevice,
    PermissionDenied,
    AlreadyRecording,
    NotRecording,
    EmptyRecording,
    UnsupportedSampleFormat,
    DeviceDisconnected,
    StreamBuildFailed,
    StreamPlayFailed,
    WavEncodingFailed,
    Internal,
}

pub struct RecordingManager {
    backend: Box<dyn RecordingInput>,
    state: Mutex<ManagerState>,
    max_duration: Duration,
    emitter: Mutex<Option<LevelEmitterHandle>>,
    emit_level: SharedLevelEmitter,
}

struct LevelEmitterHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownRecordingResult {
    Idle,
    Finalized,
    DiscardedEmpty,
}

#[derive(Default)]
struct ManagerState {
    active: Option<ActiveRecording>,
    latest: Option<CompletedRecording>,
    last_error: Option<RecordingError>,
}

struct ActiveRecording {
    _stream: Box<dyn RecordingStream>,
    buffer: SharedBuffer,
    meter: Arc<LevelMeter>,
}

impl LevelEmitterHandle {
    fn stop_and_join(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for LevelEmitterHandle {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

struct CompletedRecording {
    info: RecordingInfo,
    _samples: Vec<f32>,
    wav_bytes: Vec<u8>,
}

pub trait RecordingInput: Send + Sync + 'static {
    fn start_recording(&self, max_duration: Duration) -> Result<StartedRecording, RecordingError>;
}

pub struct StartedRecording {
    stream: Box<dyn RecordingStream>,
    buffer: SharedBuffer,
    meter: Arc<LevelMeter>,
}

pub trait RecordingStream: Send + 'static {}

pub struct CpalInputBackend;

struct CpalRecordingStream {
    _stream: cpal::Stream,
}

impl RecordingStream for CpalRecordingStream {}

impl RecordingManager {
    pub fn with_cpal() -> Self {
        Self::new(Box::new(CpalInputBackend))
    }

    pub fn new(backend: Box<dyn RecordingInput>) -> Self {
        Self::new_with_emitter(backend, Box::new(no_op_emit))
    }

    pub fn new_with_emitter(backend: Box<dyn RecordingInput>, emit_level: LevelEmitterFn) -> Self {
        Self {
            backend,
            state: Mutex::new(ManagerState::default()),
            max_duration: Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            emitter: Mutex::new(None),
            emit_level: Arc::new(Mutex::new(emit_level)),
        }
    }

    pub fn set_level_emitter(&self, emit_level: LevelEmitterFn) {
        if let Ok(mut slot) = self.emit_level.lock() {
            *slot = emit_level;
        }
    }

    pub fn start_recording(&self) -> Result<RecordingStatus, RecordingError> {
        {
            let mut state = self.lock_state()?;
            self.finalize_finished_active(&mut state)?;

            if state.active.is_some() {
                let error = recording_error(
                    RecordingErrorCode::AlreadyRecording,
                    "A recording is already in progress.",
                );
                state.last_error = Some(error.clone());
                return Err(error);
            }
        }

        let started = self.backend.start_recording(self.max_duration)?;
        let meter = Arc::clone(&started.meter);

        {
            let mut state = self.lock_state()?;
            state.last_error = None;
            state.active = Some(ActiveRecording {
                _stream: started.stream,
                buffer: started.buffer,
                meter: Arc::clone(&meter),
            });
        }

        self.start_level_emitter(meter)?;

        Ok({
            let state = self.lock_state()?;
            self.status_from_state(&state)
        })
    }

    pub fn stop_recording(&self) -> Result<RecordingInfo, RecordingError> {
        let mut state = self.lock_state()?;

        let Some(active) = state.active.take() else {
            let error = recording_error(
                RecordingErrorCode::NotRecording,
                "No recording is currently in progress.",
            );
            state.last_error = Some(error.clone());
            return Err(error);
        };

        self.stop_level_emitter_and_reset(&active.meter);

        let completed = finalize_active(active, RecordingEndReason::Manual)?;
        let info = completed.info.clone();
        state.latest = Some(completed);
        state.last_error = None;

        Ok(info)
    }

    pub fn stop_for_shutdown(&self) -> Result<ShutdownRecordingResult, RecordingError> {
        let mut state = self.lock_state()?;

        let Some(active) = state.active.take() else {
            return Ok(ShutdownRecordingResult::Idle);
        };

        self.stop_level_emitter_and_reset(&active.meter);

        match finalize_active(active, RecordingEndReason::Shutdown) {
            Ok(completed) => {
                state.latest = Some(completed);
                state.last_error = None;
                Ok(ShutdownRecordingResult::Finalized)
            }
            Err(error) if error.code == RecordingErrorCode::EmptyRecording => {
                state.last_error = None;
                Ok(ShutdownRecordingResult::DiscardedEmpty)
            }
            Err(error) => {
                state.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub fn get_recording_status(&self) -> Result<RecordingStatus, RecordingError> {
        let mut state = self.lock_state()?;
        self.finalize_finished_active(&mut state)?;

        Ok(self.status_from_state(&state))
    }

    pub fn get_latest_recording_info(&self) -> Result<Option<RecordingInfo>, RecordingError> {
        let mut state = self.lock_state()?;
        self.finalize_finished_active(&mut state)?;

        Ok(state.latest.as_ref().map(|latest| latest.info.clone()))
    }

    pub fn get_latest_recording_wav_bytes(&self) -> Result<Option<Vec<u8>>, RecordingError> {
        let mut state = self.lock_state()?;
        self.finalize_finished_active(&mut state)?;

        Ok(state.latest.as_ref().map(|latest| latest.wav_bytes.clone()))
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, ManagerState>, RecordingError> {
        self.state.lock().map_err(|_| {
            recording_error(
                RecordingErrorCode::Internal,
                "Recording state could not be locked.",
            )
        })
    }

    fn finalize_finished_active(&self, state: &mut ManagerState) -> Result<(), RecordingError> {
        let should_finalize = state
            .active
            .as_ref()
            .map(|active| {
                active
                    .buffer
                    .lock()
                    .map(|buffer| buffer.is_finished())
                    .unwrap_or(true)
            })
            .unwrap_or(false);

        if !should_finalize {
            return Ok(());
        }

        if let Some(active) = state.active.take() {
            self.stop_level_emitter_and_reset(&active.meter);
            match finalize_active(active, RecordingEndReason::Manual) {
                Ok(completed) => {
                    if completed.info.ended_reason == RecordingEndReason::DeviceDisconnected {
                        state.last_error = Some(recording_error(
                            RecordingErrorCode::DeviceDisconnected,
                            "The input device disconnected while recording.",
                        ));
                    } else {
                        state.last_error = None;
                    }
                    state.latest = Some(completed);
                }
                Err(error) if error.code == RecordingErrorCode::EmptyRecording => {
                    state.last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }

        Ok(())
    }

    fn status_from_state(&self, state: &ManagerState) -> RecordingStatus {
        let latest_recording = state.latest.as_ref().map(|latest| latest.info.clone());
        let last_error = state.last_error.clone();

        if let Some(active) = &state.active {
            return active
                .buffer
                .lock()
                .map(|buffer| buffer.status(latest_recording.clone(), last_error))
                .unwrap_or_else(|_| RecordingStatus {
                    is_recording: false,
                    sample_rate: None,
                    input_channels: None,
                    output_channels: OUTPUT_CHANNELS,
                    duration_ms: 0,
                    sample_count: 0,
                    started_at_ms: None,
                    max_duration_seconds: MAX_RECORDING_DURATION_SECONDS,
                    latest_recording,
                    last_error: Some(recording_error(
                        RecordingErrorCode::Internal,
                        "Recording buffer could not be locked.",
                    )),
                });
        }

        RecordingStatus {
            is_recording: false,
            sample_rate: None,
            input_channels: None,
            output_channels: OUTPUT_CHANNELS,
            duration_ms: 0,
            sample_count: 0,
            started_at_ms: None,
            max_duration_seconds: MAX_RECORDING_DURATION_SECONDS,
            latest_recording,
            last_error,
        }
    }

    fn start_level_emitter(&self, meter: Arc<LevelMeter>) -> Result<(), RecordingError> {
        let emit_level = Arc::clone(&self.emit_level);

        let stop = Arc::new(AtomicBool::new(false));
        let stop_signal = Arc::clone(&stop);
        let meter_for_thread = Arc::clone(&meter);

        let join = std::thread::Builder::new()
            .name("floe-recording-level".to_string())
            .spawn(move || {
                level_emitter_loop(meter_for_thread, stop_signal, emit_level);
            })
            .map_err(|_| internal_error())?;

        let mut slot = self.emitter.lock().map_err(|_| internal_error())?;
        *slot = Some(LevelEmitterHandle {
            stop,
            join: Some(join),
        });

        Ok(())
    }

    fn stop_level_emitter_and_reset(&self, meter: &Arc<LevelMeter>) {
        if let Ok(mut slot) = self.emitter.lock() {
            if let Some(mut handle) = slot.take() {
                handle.stop_and_join();
            }
        }

        meter.store(0.0);

        if let Ok(emit_slot) = self.emit_level.lock() {
            (emit_slot)(0.0);
        }
    }
}

impl RecordingInput for CpalInputBackend {
    fn start_recording(&self, max_duration: Duration) -> Result<StartedRecording, RecordingError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| {
            recording_error(
                RecordingErrorCode::NoInputDevice,
                "No default input device is available.",
            )
        })?;

        let supported_config = device.default_input_config().map_err(map_config_error)?;
        let sample_rate = supported_config.sample_rate();
        let input_channels = supported_config.channels();
        let meter = Arc::new(LevelMeter::new());
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            sample_rate,
            input_channels,
            max_duration,
            now_ms(),
        )));

        let stream_config = supported_config.config();
        let err_buffer = Arc::clone(&buffer);
        let err_fn = move |_error| {
            if let Ok(mut buffer) = err_buffer.lock() {
                buffer.mark_device_disconnected();
            }
        };

        let sample_format = supported_config.sample_format();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_input_stream::<f32>(
                &device,
                &stream_config,
                &buffer,
                Arc::clone(&meter),
                err_fn,
            ),
            cpal::SampleFormat::F64 => build_input_stream::<f64>(
                &device,
                &stream_config,
                &buffer,
                Arc::clone(&meter),
                err_fn,
            ),
            cpal::SampleFormat::I16 => build_input_stream::<i16>(
                &device,
                &stream_config,
                &buffer,
                Arc::clone(&meter),
                err_fn,
            ),
            cpal::SampleFormat::U16 => build_input_stream::<u16>(
                &device,
                &stream_config,
                &buffer,
                Arc::clone(&meter),
                err_fn,
            ),
            cpal::SampleFormat::I8 => build_input_stream::<i8>(
                &device,
                &stream_config,
                &buffer,
                Arc::clone(&meter),
                err_fn,
            ),
            cpal::SampleFormat::U8 => build_input_stream::<u8>(
                &device,
                &stream_config,
                &buffer,
                Arc::clone(&meter),
                err_fn,
            ),
            cpal::SampleFormat::I32 => build_input_stream::<i32>(
                &device,
                &stream_config,
                &buffer,
                Arc::clone(&meter),
                err_fn,
            ),
            cpal::SampleFormat::U32 => build_input_stream::<u32>(
                &device,
                &stream_config,
                &buffer,
                Arc::clone(&meter),
                err_fn,
            ),
            _ => Err(recording_error(
                RecordingErrorCode::UnsupportedSampleFormat,
                "The default input device uses an unsupported sample format.",
            )),
        }?;

        stream.play().map_err(|error| {
            map_stream_error(
                error.to_string(),
                RecordingErrorCode::StreamPlayFailed,
                "The input stream could not be started.",
            )
        })?;

        Ok(StartedRecording {
            stream: Box::new(CpalRecordingStream { _stream: stream }),
            buffer,
            meter,
        })
    }
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: &SharedBuffer,
    meter: Arc<LevelMeter>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, RecordingError>
where
    T: AudioSample + cpal::SizedSample + Send + 'static,
{
    let data_buffer = Arc::clone(buffer);
    let data_meter = meter;
    device
        .build_input_stream(
            config,
            move |data: &[T], _info: &cpal::InputCallbackInfo| {
                if let Ok(mut buffer) = data_buffer.lock() {
                    buffer.append_interleaved(data, &data_meter);
                }
            },
            err_fn,
            None,
        )
        .map_err(|error| {
            map_stream_error(
                error.to_string(),
                RecordingErrorCode::StreamBuildFailed,
                "The input stream could not be created.",
            )
        })
}

trait AudioSample: Copy {
    fn to_mono_value(self) -> f32;
}

impl AudioSample for f32 {
    fn to_mono_value(self) -> f32 {
        self.clamp(-1.0, 1.0)
    }
}

impl AudioSample for f64 {
    fn to_mono_value(self) -> f32 {
        (self as f32).clamp(-1.0, 1.0)
    }
}

impl AudioSample for i16 {
    fn to_mono_value(self) -> f32 {
        signed_to_f32(self as f32, i16::MAX as f32)
    }
}

impl AudioSample for i8 {
    fn to_mono_value(self) -> f32 {
        signed_to_f32(self as f32, i8::MAX as f32)
    }
}

impl AudioSample for i32 {
    fn to_mono_value(self) -> f32 {
        signed_to_f32(self as f32, i32::MAX as f32)
    }
}

impl AudioSample for u16 {
    fn to_mono_value(self) -> f32 {
        unsigned_to_f32(self as f64, u16::MAX as f64)
    }
}

impl AudioSample for u8 {
    fn to_mono_value(self) -> f32 {
        unsigned_to_f32(self as f64, u8::MAX as f64)
    }
}

impl AudioSample for u32 {
    fn to_mono_value(self) -> f32 {
        unsigned_to_f32(self as f64, u32::MAX as f64)
    }
}

fn signed_to_f32(value: f32, max: f32) -> f32 {
    (value / max).clamp(-1.0, 1.0)
}

fn unsigned_to_f32(value: f64, max: f64) -> f32 {
    (((value / max) * 2.0) - 1.0).clamp(-1.0, 1.0) as f32
}

struct RecordingBuffer {
    samples: Vec<f32>,
    sample_rate: u32,
    input_channels: u16,
    max_samples: usize,
    started_at_ms: u64,
    ended_at_ms: Option<u64>,
    end_reason: Option<RecordingEndReason>,
}

impl RecordingBuffer {
    fn new(
        sample_rate: u32,
        input_channels: u16,
        max_duration: Duration,
        started_at_ms: u64,
    ) -> Self {
        let max_samples = max_duration
            .as_secs()
            .saturating_mul(sample_rate as u64)
            .try_into()
            .unwrap_or(usize::MAX);

        Self {
            samples: Vec::with_capacity(max_samples.min(sample_rate as usize)),
            sample_rate,
            input_channels,
            max_samples,
            started_at_ms,
            ended_at_ms: None,
            end_reason: None,
        }
    }

    fn append_interleaved<T: AudioSample>(&mut self, input: &[T], level_meter: &LevelMeter) {
        if self.is_finished() || input.is_empty() || self.input_channels == 0 {
            return;
        }

        let channels = self.input_channels as usize;
        let available_samples = self.max_samples.saturating_sub(self.samples.len());
        let available_frames = input.len() / channels;
        let frames_to_take = available_frames.min(available_samples);

        let mut sum_squares: f64 = 0.0;
        let mut frame_count: usize = 0;
        for frame in input.chunks_exact(channels).take(frames_to_take) {
            let sum: f32 = frame.iter().map(|sample| sample.to_mono_value()).sum();
            let mono = sum / self.input_channels as f32;
            let mono_f64 = mono as f64;
            sum_squares += mono_f64 * mono_f64;
            frame_count += 1;
            self.samples.push(mono);
        }

        if frame_count > 0 {
            let rms = ((sum_squares / frame_count as f64).sqrt() as f32).clamp(0.0, 1.0);
            level_meter.store(rms);
        }

        if self.samples.len() >= self.max_samples {
            self.finish(RecordingEndReason::MaxDuration);
        }
    }

    fn mark_device_disconnected(&mut self) {
        self.finish(RecordingEndReason::DeviceDisconnected);
    }

    fn is_finished(&self) -> bool {
        self.end_reason.is_some()
    }

    fn finish(&mut self, reason: RecordingEndReason) {
        if self.end_reason.is_none() {
            self.ended_at_ms = Some(self.started_at_ms.saturating_add(self.duration_ms()));
            self.end_reason = Some(reason);
        }
    }

    fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }

        ((self.samples.len() as u64).saturating_mul(1000)) / self.sample_rate as u64
    }

    fn status(
        &self,
        latest_recording: Option<RecordingInfo>,
        last_error: Option<RecordingError>,
    ) -> RecordingStatus {
        RecordingStatus {
            is_recording: !self.is_finished(),
            sample_rate: Some(self.sample_rate),
            input_channels: Some(self.input_channels),
            output_channels: OUTPUT_CHANNELS,
            duration_ms: self.duration_ms(),
            sample_count: self.samples.len() as u64,
            started_at_ms: Some(self.started_at_ms),
            max_duration_seconds: MAX_RECORDING_DURATION_SECONDS,
            latest_recording,
            last_error,
        }
    }

    #[cfg(test)]
    fn into_completed(
        mut self,
        default_reason: RecordingEndReason,
    ) -> Result<CompletedRecording, RecordingError> {
        if self.samples.is_empty() {
            return Err(recording_error(
                RecordingErrorCode::EmptyRecording,
                "The recording did not capture any audio samples.",
            ));
        }

        if self.end_reason.is_none() {
            self.finish(default_reason.clone());
        }

        let ended_reason = self.end_reason.clone().unwrap_or(default_reason);
        let ended_at_ms = self.ended_at_ms.unwrap_or_else(now_ms);
        let info = RecordingInfo {
            sample_rate: self.sample_rate,
            input_channels: self.input_channels,
            output_channels: OUTPUT_CHANNELS,
            duration_ms: self.duration_ms(),
            sample_count: self.samples.len() as u64,
            wav_byte_count: 0,
            wav_bits_per_sample: WAV_BITS_PER_SAMPLE,
            started_at_ms: self.started_at_ms,
            ended_at_ms,
            max_duration_reached: ended_reason == RecordingEndReason::MaxDuration,
            ended_reason,
        };
        let wav_bytes = encode_pcm16_wav(&self.samples, self.sample_rate, OUTPUT_CHANNELS)?;
        let info = RecordingInfo {
            wav_byte_count: wav_bytes.len() as u64,
            ..info
        };

        Ok(CompletedRecording {
            info,
            _samples: self.samples,
            wav_bytes,
        })
    }

    fn snapshot_completed(
        &mut self,
        default_reason: RecordingEndReason,
    ) -> Result<CompletedRecording, RecordingError> {
        if self.samples.is_empty() {
            return Err(recording_error(
                RecordingErrorCode::EmptyRecording,
                "The recording did not capture any audio samples.",
            ));
        }

        if self.end_reason.is_none() {
            self.finish(default_reason.clone());
        }

        let ended_reason = self.end_reason.clone().unwrap_or(default_reason);
        let ended_at_ms = self.ended_at_ms.unwrap_or_else(now_ms);
        let info = RecordingInfo {
            sample_rate: self.sample_rate,
            input_channels: self.input_channels,
            output_channels: OUTPUT_CHANNELS,
            duration_ms: self.duration_ms(),
            sample_count: self.samples.len() as u64,
            wav_byte_count: 0,
            wav_bits_per_sample: WAV_BITS_PER_SAMPLE,
            started_at_ms: self.started_at_ms,
            ended_at_ms,
            max_duration_reached: ended_reason == RecordingEndReason::MaxDuration,
            ended_reason,
        };
        let wav_bytes = encode_pcm16_wav(&self.samples, self.sample_rate, OUTPUT_CHANNELS)?;
        let info = RecordingInfo {
            wav_byte_count: wav_bytes.len() as u64,
            ..info
        };

        Ok(CompletedRecording {
            info,
            _samples: self.samples.clone(),
            wav_bytes,
        })
    }
}

const WAV_HEADER_LEN: usize = 44;
const WAV_AUDIO_FORMAT_PCM: u16 = 1;
const WAV_BITS_PER_SAMPLE: u16 = 16;

fn encode_pcm16_wav(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<Vec<u8>, RecordingError> {
    if sample_rate == 0 || channels == 0 {
        return Err(recording_error(
            RecordingErrorCode::WavEncodingFailed,
            "Recording WAV bytes could not be encoded.",
        ));
    }

    let data_len = samples
        .len()
        .checked_mul(2)
        .ok_or_else(wav_encoding_error)?;
    let riff_chunk_size = 36usize
        .checked_add(data_len)
        .ok_or_else(wav_encoding_error)?;
    if riff_chunk_size > u32::MAX as usize || data_len > u32::MAX as usize {
        return Err(wav_encoding_error());
    }

    let block_align = channels
        .checked_mul(WAV_BITS_PER_SAMPLE / 8)
        .ok_or_else(wav_encoding_error)?;
    let byte_rate = sample_rate
        .checked_mul(block_align as u32)
        .ok_or_else(wav_encoding_error)?;

    let mut wav = Vec::with_capacity(WAV_HEADER_LEN + data_len);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(riff_chunk_size as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&WAV_AUDIO_FORMAT_PCM.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&WAV_BITS_PER_SAMPLE.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_len as u32).to_le_bytes());

    for sample in samples {
        wav.extend_from_slice(&float_to_pcm16(*sample).to_le_bytes());
    }

    validate_pcm16_wav_header(&wav, sample_rate, channels, samples.len())?;
    Ok(wav)
}

fn validate_pcm16_wav_header(
    wav: &[u8],
    sample_rate: u32,
    channels: u16,
    sample_count: usize,
) -> Result<(), RecordingError> {
    let expected_data_len = sample_count.checked_mul(2).ok_or_else(wav_encoding_error)?;
    let expected_len = WAV_HEADER_LEN
        .checked_add(expected_data_len)
        .ok_or_else(wav_encoding_error)?;
    let expected_riff_size = 36usize
        .checked_add(expected_data_len)
        .ok_or_else(wav_encoding_error)?;

    let is_valid = wav.len() == expected_len
        && wav.get(0..4) == Some(b"RIFF")
        && read_u32_le(wav, 4) == Some(expected_riff_size as u32)
        && wav.get(8..12) == Some(b"WAVE")
        && wav.get(12..16) == Some(b"fmt ")
        && read_u32_le(wav, 16) == Some(16)
        && read_u16_le(wav, 20) == Some(WAV_AUDIO_FORMAT_PCM)
        && read_u16_le(wav, 22) == Some(channels)
        && read_u32_le(wav, 24) == Some(sample_rate)
        && read_u32_le(wav, 28)
            == sample_rate.checked_mul((channels * (WAV_BITS_PER_SAMPLE / 8)) as u32)
        && read_u16_le(wav, 32) == Some(channels * (WAV_BITS_PER_SAMPLE / 8))
        && read_u16_le(wav, 34) == Some(WAV_BITS_PER_SAMPLE)
        && wav.get(36..40) == Some(b"data")
        && read_u32_le(wav, 40) == Some(expected_data_len as u32);

    if is_valid {
        Ok(())
    } else {
        Err(wav_encoding_error())
    }
}

fn float_to_pcm16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    if clamped < 0.0 {
        (clamped * 32_768.0).round() as i16
    } else {
        (clamped * 32_767.0).round() as i16
    }
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes
        .get(offset..offset + 2)
        .map(|value| u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|value| u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn wav_encoding_error() -> RecordingError {
    recording_error(
        RecordingErrorCode::WavEncodingFailed,
        "Recording WAV bytes could not be encoded.",
    )
}

fn finalize_active(
    active: ActiveRecording,
    default_reason: RecordingEndReason,
) -> Result<CompletedRecording, RecordingError> {
    let ActiveRecording {
        _stream,
        buffer,
        meter: _,
    } = active;
    drop(_stream);

    let mut buffer = buffer.lock().map_err(|_| {
        recording_error(
            RecordingErrorCode::Internal,
            "Recording buffer could not be finalized.",
        )
    })?;

    buffer.snapshot_completed(default_reason)
}

fn map_config_error(error: cpal::DefaultStreamConfigError) -> RecordingError {
    let message = error.to_string();
    if looks_like_permission_denied(&message) {
        return recording_error(
            RecordingErrorCode::PermissionDenied,
            "Permission to access the input device was denied.",
        );
    }

    recording_error(
        RecordingErrorCode::NoInputDevice,
        "The default input device configuration is not available.",
    )
}

fn map_stream_error(
    detail: String,
    fallback_code: RecordingErrorCode,
    fallback_message: &'static str,
) -> RecordingError {
    if looks_like_permission_denied(&detail) {
        return recording_error(
            RecordingErrorCode::PermissionDenied,
            "Permission to access the input device was denied.",
        );
    }

    recording_error(fallback_code, fallback_message)
}

fn looks_like_permission_denied(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("permission") || lower.contains("denied") || lower.contains("unauthorized")
}

fn recording_error(code: RecordingErrorCode, message: &'static str) -> RecordingError {
    RecordingError {
        code,
        message: message.to_string(),
    }
}

fn internal_error() -> RecordingError {
    recording_error(
        RecordingErrorCode::Internal,
        "Recording level emitter could not be started.",
    )
}

fn level_emitter_loop(
    meter: Arc<LevelMeter>,
    stop: Arc<AtomicBool>,
    emit_level: SharedLevelEmitter,
) {
    let mut smoothed: f32 = 0.0;

    while !stop.load(Ordering::SeqCst) {
        let raw = meter.load();
        let normalized = normalize_rms(raw);
        smoothed = fold_level(smoothed, normalized);

        if let Ok(emit) = emit_level.lock() {
            (emit)(smoothed);
        }

        std::thread::sleep(Duration::from_millis(EMIT_INTERVAL_MS));
    }
}

fn no_op_emit(_level: f32) {}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use super::{
        encode_pcm16_wav, float_to_pcm16, normalize_rms, read_u16_le, read_u32_le, LevelMeter,
        RecordingBuffer, RecordingEndReason, RecordingErrorCode, RecordingInput, RecordingManager,
        RecordingStream, ShutdownRecordingResult, StartedRecording, MAX_RECORDING_DURATION_SECONDS,
        WAV_HEADER_LEN,
    };

    struct FakeStream;

    impl RecordingStream for FakeStream {}

    struct FakeBackend {
        buffer: Arc<Mutex<RecordingBuffer>>,
        meter: Arc<LevelMeter>,
    }

    impl RecordingInput for FakeBackend {
        fn start_recording(
            &self,
            _max_duration: Duration,
        ) -> Result<StartedRecording, super::RecordingError> {
            Ok(StartedRecording {
                stream: Box::new(FakeStream),
                buffer: Arc::clone(&self.buffer),
                meter: Arc::clone(&self.meter),
            })
        }
    }

    #[test]
    fn mono_input_keeps_samples() {
        let mut buffer = RecordingBuffer::new(48_000, 1, Duration::from_secs(1), 1000);
        let meter = LevelMeter::new();

        buffer.append_interleaved(&[0.25_f32, -0.5, 1.0], &meter);

        assert_eq!(buffer.samples, vec![0.25, -0.5, 1.0]);
    }

    #[test]
    fn stereo_input_is_averaged_to_mono() {
        let mut buffer = RecordingBuffer::new(48_000, 2, Duration::from_secs(1), 1000);
        let meter = LevelMeter::new();

        buffer.append_interleaved(&[1.0_f32, -1.0, 0.5, 0.25], &meter);

        assert_eq!(buffer.samples, vec![0.0, 0.375]);
    }

    #[test]
    fn integer_samples_are_normalized() {
        let mut signed = RecordingBuffer::new(48_000, 1, Duration::from_secs(1), 1000);
        let meter = LevelMeter::new();
        signed.append_interleaved(&[i16::MIN, 0, i16::MAX], &meter);

        assert_eq!(signed.samples[0], -1.0);
        assert_eq!(signed.samples[1], 0.0);
        assert_eq!(signed.samples[2], 1.0);

        let mut unsigned = RecordingBuffer::new(48_000, 1, Duration::from_secs(1), 1000);
        let meter = LevelMeter::new();
        unsigned.append_interleaved(&[u8::MIN, u8::MAX], &meter);

        assert_eq!(unsigned.samples[0], -1.0);
        assert_eq!(unsigned.samples[1], 1.0);
    }

    #[test]
    fn sample_cap_is_enforced() {
        let mut buffer = RecordingBuffer::new(10, 1, Duration::from_secs(2), 1000);
        let meter = LevelMeter::new();

        buffer.append_interleaved(&[0.2_f32; 25], &meter);

        assert_eq!(buffer.samples.len(), 20);
        assert!(buffer.is_finished());
        assert_eq!(buffer.end_reason, Some(RecordingEndReason::MaxDuration));
    }

    #[test]
    fn completed_info_tracks_duration_and_sample_count() {
        let mut buffer = RecordingBuffer::new(1_000, 2, Duration::from_secs(1), 10_000);
        let meter = LevelMeter::new();
        buffer.append_interleaved(&[0.5_f32, 0.5, 0.25, 0.25, 1.0, -1.0], &meter);

        let completed = buffer
            .into_completed(RecordingEndReason::Manual)
            .expect("buffer should complete");

        assert_eq!(completed.info.sample_rate, 1_000);
        assert_eq!(completed.info.input_channels, 2);
        assert_eq!(completed.info.output_channels, 1);
        assert_eq!(completed.info.sample_count, 3);
        assert_eq!(completed.info.duration_ms, 3);
        assert_eq!(completed.info.wav_byte_count, 50);
        assert_eq!(completed.info.wav_bits_per_sample, 16);
        assert_eq!(completed.info.started_at_ms, 10_000);
        assert_eq!(completed.info.ended_at_ms, 10_003);
    }

    #[test]
    fn wav_header_matches_pcm16_mono_recording() {
        let wav =
            encode_pcm16_wav(&[-1.0, 0.0, 1.0], 16_000, 1).expect("wav encoding should succeed");

        assert_eq!(wav.len(), WAV_HEADER_LEN + 6);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(read_u32_le(&wav, 4), Some(42));
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(read_u32_le(&wav, 16), Some(16));
        assert_eq!(read_u16_le(&wav, 20), Some(1));
        assert_eq!(read_u16_le(&wav, 22), Some(1));
        assert_eq!(read_u32_le(&wav, 24), Some(16_000));
        assert_eq!(read_u32_le(&wav, 28), Some(32_000));
        assert_eq!(read_u16_le(&wav, 32), Some(2));
        assert_eq!(read_u16_le(&wav, 34), Some(16));
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(read_u32_le(&wav, 40), Some(6));
    }

    #[test]
    fn wav_encoding_writes_clamped_pcm16_samples() {
        let wav = encode_pcm16_wav(&[-2.0, -0.5, 0.0, 0.5, 2.0], 48_000, 1)
            .expect("wav encoding should succeed");
        let pcm: Vec<i16> = wav[WAV_HEADER_LEN..]
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
            .collect();

        assert_eq!(pcm, vec![-32768, -16384, 0, 16384, 32767]);
        assert_eq!(float_to_pcm16(f32::NAN), 0);
    }

    #[test]
    fn wav_encoding_rejects_invalid_format_parameters() {
        let sample_rate_error =
            encode_pcm16_wav(&[0.0], 0, 1).expect_err("zero sample rate should fail");
        let channels_error =
            encode_pcm16_wav(&[0.0], 48_000, 0).expect_err("zero channels should fail");

        assert_eq!(
            sample_rate_error.code,
            RecordingErrorCode::WavEncodingFailed
        );
        assert_eq!(channels_error.code, RecordingErrorCode::WavEncodingFailed);
    }

    #[test]
    fn manager_returns_latest_wav_bytes_without_disk_export() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            8_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = RecordingManager::new(Box::new(FakeBackend {
            buffer: Arc::clone(&buffer),
            meter: Arc::new(LevelMeter::new()),
        }));

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.0_f32, 1.0], &LevelMeter::new());
        let info = manager.stop_recording().expect("stop succeeds");
        let wav = manager
            .get_latest_recording_wav_bytes()
            .expect("latest wav lookup succeeds")
            .expect("latest wav exists");

        assert_eq!(info.wav_byte_count, wav.len() as u64);
        assert_eq!(wav.len(), WAV_HEADER_LEN + 4);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn manager_rejects_already_recording_and_not_recording() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = RecordingManager::new(Box::new(FakeBackend {
            buffer: Arc::clone(&buffer),
            meter: Arc::new(LevelMeter::new()),
        }));

        manager.start_recording().expect("first start succeeds");
        let start_error = manager
            .start_recording()
            .expect_err("second start should fail");
        assert_eq!(start_error.code, RecordingErrorCode::AlreadyRecording);

        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.5_f32], &LevelMeter::new());
        manager.stop_recording().expect("stop succeeds");
        let stop_error = manager
            .stop_recording()
            .expect_err("second stop should fail");
        assert_eq!(stop_error.code, RecordingErrorCode::NotRecording);
    }

    #[test]
    fn manager_rejects_empty_recording() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = RecordingManager::new(Box::new(FakeBackend {
            buffer,
            meter: Arc::new(LevelMeter::new()),
        }));

        manager.start_recording().expect("start succeeds");
        let error = manager
            .stop_recording()
            .expect_err("empty stop should fail");

        assert_eq!(error.code, RecordingErrorCode::EmptyRecording);
    }

    #[test]
    fn empty_stop_preserves_previous_latest_recording() {
        let mut latest_buffer = RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        );
        latest_buffer.append_interleaved(&[0.5_f32], &LevelMeter::new());
        let previous_latest = latest_buffer
            .into_completed(RecordingEndReason::Manual)
            .expect("latest fixture should complete");
        let empty_buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            2000,
        )));
        let manager = RecordingManager::new(Box::new(FakeBackend {
            buffer: Arc::clone(&empty_buffer),
            meter: Arc::new(LevelMeter::new()),
        }));
        manager.state.lock().unwrap().latest = Some(previous_latest);

        manager.start_recording().expect("start succeeds");
        let error = manager
            .stop_recording()
            .expect_err("empty stop should fail");
        let latest = manager
            .get_latest_recording_info()
            .unwrap()
            .expect("previous latest should remain");

        assert_eq!(error.code, RecordingErrorCode::EmptyRecording);
        assert_eq!(latest.started_at_ms, 1000);
        assert_eq!(latest.sample_count, 1);
    }

    #[test]
    fn stop_returns_info_after_max_duration_is_reached() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            2,
            1,
            Duration::from_secs(1),
            1000,
        )));
        let manager = RecordingManager::new(Box::new(FakeBackend {
            buffer: Arc::clone(&buffer),
            meter: Arc::new(LevelMeter::new()),
        }));

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.25_f32, 0.25, 0.25], &LevelMeter::new());

        let info = manager.stop_recording().expect("stop succeeds");

        assert_eq!(info.sample_count, 2);
        assert!(info.max_duration_reached);
        assert_eq!(info.ended_reason, RecordingEndReason::MaxDuration);
    }

    #[test]
    fn device_disconnect_finalizes_status_without_microphone() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = RecordingManager::new(Box::new(FakeBackend {
            buffer: Arc::clone(&buffer),
            meter: Arc::new(LevelMeter::new()),
        }));

        manager.start_recording().expect("start succeeds");
        {
            let mut buffer = buffer.lock().unwrap();
            buffer.append_interleaved(&[0.25_f32, 0.25], &LevelMeter::new());
            buffer.mark_device_disconnected();
        }

        let status = manager.get_recording_status().expect("status succeeds");

        assert!(!status.is_recording);
        assert_eq!(
            status.latest_recording.unwrap().ended_reason,
            RecordingEndReason::DeviceDisconnected
        );
        assert_eq!(
            status.last_error.unwrap().code,
            RecordingErrorCode::DeviceDisconnected
        );
    }

    #[test]
    fn shutdown_when_idle_is_idempotent() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = RecordingManager::new(Box::new(FakeBackend {
            buffer,
            meter: Arc::new(LevelMeter::new()),
        }));

        assert_eq!(
            manager.stop_for_shutdown().unwrap(),
            ShutdownRecordingResult::Idle
        );
        assert_eq!(
            manager.stop_for_shutdown().unwrap(),
            ShutdownRecordingResult::Idle
        );
    }

    #[test]
    fn shutdown_discards_empty_active_recording_without_error() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = RecordingManager::new(Box::new(FakeBackend {
            buffer,
            meter: Arc::new(LevelMeter::new()),
        }));

        manager.start_recording().expect("start succeeds");

        assert_eq!(
            manager.stop_for_shutdown().unwrap(),
            ShutdownRecordingResult::DiscardedEmpty
        );
        let status = manager.get_recording_status().unwrap();
        assert!(!status.is_recording);
        assert!(status.latest_recording.is_none());
        assert!(status.last_error.is_none());
    }

    #[test]
    fn shutdown_finalizes_captured_samples_with_shutdown_reason() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = RecordingManager::new(Box::new(FakeBackend {
            buffer: Arc::clone(&buffer),
            meter: Arc::new(LevelMeter::new()),
        }));

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.5_f32, 0.25], &LevelMeter::new());

        assert_eq!(
            manager.stop_for_shutdown().unwrap(),
            ShutdownRecordingResult::Finalized
        );
        let latest = manager
            .get_latest_recording_info()
            .unwrap()
            .expect("shutdown recording exists");

        assert_eq!(latest.ended_reason, RecordingEndReason::Shutdown);
        assert_eq!(latest.sample_count, 2);
    }

    #[test]
    fn shutdown_empty_recording_preserves_previous_latest_recording() {
        let latest = RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        );
        let mut active = RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            2000,
        );
        active.append_interleaved(&[0.5_f32], &LevelMeter::new());
        let completed = active
            .into_completed(RecordingEndReason::Manual)
            .expect("latest fixture should complete");
        let buffer = Arc::new(Mutex::new(latest));
        let manager = RecordingManager::new(Box::new(FakeBackend {
            buffer: Arc::clone(&buffer),
            meter: Arc::new(LevelMeter::new()),
        }));
        manager.state.lock().unwrap().latest = Some(completed);

        manager.start_recording().expect("start succeeds");

        assert_eq!(
            manager.stop_for_shutdown().unwrap(),
            ShutdownRecordingResult::DiscardedEmpty
        );
        let latest = manager
            .get_latest_recording_info()
            .unwrap()
            .expect("previous latest remains");

        assert_eq!(latest.ended_reason, RecordingEndReason::Manual);
        assert_eq!(latest.sample_count, 1);
    }

    #[test]
    fn append_interleaved_updates_level_meter_for_silence() {
        let mut buffer = RecordingBuffer::new(48_000, 1, Duration::from_secs(1), 1000);
        let meter = LevelMeter::new();

        buffer.append_interleaved(&[0.0_f32; 256], &meter);

        assert_eq!(meter.load(), 0.0);
    }

    #[test]
    fn append_interleaved_updates_level_meter_for_signal() {
        let mut buffer = RecordingBuffer::new(48_000, 1, Duration::from_secs(1), 1000);
        let meter = LevelMeter::new();
        let samples: Vec<f32> = (0..512)
            .map(|index| if index % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        let expected_rms = 0.5_f32;

        buffer.append_interleaved(&samples, &meter);

        let actual = meter.load();
        assert!(
            (actual - expected_rms).abs() < 0.01,
            "expected ~{expected_rms} got {actual}"
        );
    }

    #[test]
    fn append_interleaved_keeps_existing_samples_alongside_level() {
        let mut buffer = RecordingBuffer::new(48_000, 1, Duration::from_secs(1), 1000);
        let meter = LevelMeter::new();

        buffer.append_interleaved(&[0.5_f32, -0.5, 0.5, -0.5], &meter);

        assert_eq!(buffer.samples, vec![0.5, -0.5, 0.5, -0.5]);
        assert!(meter.load() > 0.0);
    }

    #[test]
    fn normalize_rms_maps_quiet_signal_above_zero() {
        let normalized = normalize_rms(0.05);
        assert!(normalized > 0.0);
        assert!(normalized < 1.0);
    }
}
