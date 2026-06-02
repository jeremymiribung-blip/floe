use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;

pub const MAX_RECORDING_DURATION_SECONDS: u64 = 120;
const OUTPUT_CHANNELS: u16 = 1;

type SharedBuffer = Arc<Mutex<RecordingBuffer>>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingInfo {
    pub sample_rate: u32,
    pub input_channels: u16,
    pub output_channels: u16,
    pub duration_ms: u64,
    pub sample_count: u64,
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
    Internal,
}

pub struct RecordingManager {
    backend: Box<dyn RecordingInput>,
    state: Mutex<ManagerState>,
    max_duration: Duration,
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
}

struct CompletedRecording {
    info: RecordingInfo,
    _samples: Vec<f32>,
}

pub trait RecordingInput: Send + Sync + 'static {
    fn start_recording(&self, max_duration: Duration) -> Result<StartedRecording, RecordingError>;
}

pub struct StartedRecording {
    stream: Box<dyn RecordingStream>,
    buffer: SharedBuffer,
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
        Self {
            backend,
            state: Mutex::new(ManagerState::default()),
            max_duration: Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
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

        let mut state = self.lock_state()?;
        state.last_error = None;
        state.active = Some(ActiveRecording {
            _stream: started.stream,
            buffer: started.buffer,
        });

        Ok(self.status_from_state(&state))
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

        let completed = finalize_active(active, RecordingEndReason::Manual)?;
        let info = completed.info.clone();
        state.latest = Some(completed);
        state.last_error = None;

        Ok(info)
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
            cpal::SampleFormat::F32 => {
                build_input_stream::<f32>(&device, &stream_config, &buffer, err_fn)
            }
            cpal::SampleFormat::F64 => {
                build_input_stream::<f64>(&device, &stream_config, &buffer, err_fn)
            }
            cpal::SampleFormat::I16 => {
                build_input_stream::<i16>(&device, &stream_config, &buffer, err_fn)
            }
            cpal::SampleFormat::U16 => {
                build_input_stream::<u16>(&device, &stream_config, &buffer, err_fn)
            }
            cpal::SampleFormat::I8 => {
                build_input_stream::<i8>(&device, &stream_config, &buffer, err_fn)
            }
            cpal::SampleFormat::U8 => {
                build_input_stream::<u8>(&device, &stream_config, &buffer, err_fn)
            }
            cpal::SampleFormat::I32 => {
                build_input_stream::<i32>(&device, &stream_config, &buffer, err_fn)
            }
            cpal::SampleFormat::U32 => {
                build_input_stream::<u32>(&device, &stream_config, &buffer, err_fn)
            }
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
        })
    }
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: &SharedBuffer,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, RecordingError>
where
    T: AudioSample + cpal::SizedSample + Send + 'static,
{
    let data_buffer = Arc::clone(buffer);
    device
        .build_input_stream(
            config,
            move |data: &[T], _info: &cpal::InputCallbackInfo| {
                if let Ok(mut buffer) = data_buffer.lock() {
                    buffer.append_interleaved(data);
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

    fn append_interleaved<T: AudioSample>(&mut self, input: &[T]) {
        if self.is_finished() || input.is_empty() || self.input_channels == 0 {
            return;
        }

        let channels = self.input_channels as usize;
        let available_samples = self.max_samples.saturating_sub(self.samples.len());
        let available_frames = input.len() / channels;
        let frames_to_take = available_frames.min(available_samples);

        for frame in input.chunks_exact(channels).take(frames_to_take) {
            let sum: f32 = frame.iter().map(|sample| sample.to_mono_value()).sum();
            self.samples.push(sum / self.input_channels as f32);
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
            started_at_ms: self.started_at_ms,
            ended_at_ms,
            max_duration_reached: ended_reason == RecordingEndReason::MaxDuration,
            ended_reason,
        };

        Ok(CompletedRecording {
            info,
            _samples: self.samples,
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
            started_at_ms: self.started_at_ms,
            ended_at_ms,
            max_duration_reached: ended_reason == RecordingEndReason::MaxDuration,
            ended_reason,
        };

        Ok(CompletedRecording {
            info,
            _samples: self.samples.clone(),
        })
    }
}

fn finalize_active(
    active: ActiveRecording,
    default_reason: RecordingEndReason,
) -> Result<CompletedRecording, RecordingError> {
    let ActiveRecording { _stream, buffer } = active;
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
        RecordingBuffer, RecordingEndReason, RecordingErrorCode, RecordingInput, RecordingManager,
        RecordingStream, StartedRecording, MAX_RECORDING_DURATION_SECONDS,
    };

    struct FakeStream;

    impl RecordingStream for FakeStream {}

    struct FakeBackend {
        buffer: Arc<Mutex<RecordingBuffer>>,
    }

    impl RecordingInput for FakeBackend {
        fn start_recording(
            &self,
            _max_duration: Duration,
        ) -> Result<StartedRecording, super::RecordingError> {
            Ok(StartedRecording {
                stream: Box::new(FakeStream),
                buffer: Arc::clone(&self.buffer),
            })
        }
    }

    #[test]
    fn mono_input_keeps_samples() {
        let mut buffer = RecordingBuffer::new(48_000, 1, Duration::from_secs(1), 1000);

        buffer.append_interleaved(&[0.25_f32, -0.5, 1.0]);

        assert_eq!(buffer.samples, vec![0.25, -0.5, 1.0]);
    }

    #[test]
    fn stereo_input_is_averaged_to_mono() {
        let mut buffer = RecordingBuffer::new(48_000, 2, Duration::from_secs(1), 1000);

        buffer.append_interleaved(&[1.0_f32, -1.0, 0.5, 0.25]);

        assert_eq!(buffer.samples, vec![0.0, 0.375]);
    }

    #[test]
    fn integer_samples_are_normalized() {
        let mut signed = RecordingBuffer::new(48_000, 1, Duration::from_secs(1), 1000);
        signed.append_interleaved(&[i16::MIN, 0, i16::MAX]);

        assert_eq!(signed.samples[0], -1.0);
        assert_eq!(signed.samples[1], 0.0);
        assert_eq!(signed.samples[2], 1.0);

        let mut unsigned = RecordingBuffer::new(48_000, 1, Duration::from_secs(1), 1000);
        unsigned.append_interleaved(&[u8::MIN, u8::MAX]);

        assert_eq!(unsigned.samples[0], -1.0);
        assert_eq!(unsigned.samples[1], 1.0);
    }

    #[test]
    fn sample_cap_is_enforced() {
        let mut buffer = RecordingBuffer::new(10, 1, Duration::from_secs(2), 1000);

        buffer.append_interleaved(&[0.2_f32; 25]);

        assert_eq!(buffer.samples.len(), 20);
        assert!(buffer.is_finished());
        assert_eq!(buffer.end_reason, Some(RecordingEndReason::MaxDuration));
    }

    #[test]
    fn completed_info_tracks_duration_and_sample_count() {
        let mut buffer = RecordingBuffer::new(1_000, 2, Duration::from_secs(1), 10_000);
        buffer.append_interleaved(&[0.5_f32, 0.5, 0.25, 0.25, 1.0, -1.0]);

        let completed = buffer
            .into_completed(RecordingEndReason::Manual)
            .expect("buffer should complete");

        assert_eq!(completed.info.sample_rate, 1_000);
        assert_eq!(completed.info.input_channels, 2);
        assert_eq!(completed.info.output_channels, 1);
        assert_eq!(completed.info.sample_count, 3);
        assert_eq!(completed.info.duration_ms, 3);
        assert_eq!(completed.info.started_at_ms, 10_000);
        assert_eq!(completed.info.ended_at_ms, 10_003);
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
        }));

        manager.start_recording().expect("first start succeeds");
        let start_error = manager
            .start_recording()
            .expect_err("second start should fail");
        assert_eq!(start_error.code, RecordingErrorCode::AlreadyRecording);

        buffer.lock().unwrap().append_interleaved(&[0.5_f32]);
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
        let manager = RecordingManager::new(Box::new(FakeBackend { buffer }));

        manager.start_recording().expect("start succeeds");
        let error = manager
            .stop_recording()
            .expect_err("empty stop should fail");

        assert_eq!(error.code, RecordingErrorCode::EmptyRecording);
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
        }));

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.25_f32, 0.25, 0.25]);

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
        }));

        manager.start_recording().expect("start succeeds");
        {
            let mut buffer = buffer.lock().unwrap();
            buffer.append_interleaved(&[0.25_f32, 0.25]);
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
}
