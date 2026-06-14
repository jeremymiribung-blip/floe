use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::audio::{fold_level, normalize_rms, LevelMeter, EMIT_INTERVAL_MS};

mod buffer;
mod error;
mod input;
mod sample;
mod types;
mod wav;

pub use self::{
    buffer::RecordingBuffer,
    error::{RecordingError, RecordingErrorCode},
    input::{CpalInputBackend, RecordingInput, RecordingStream, StartedRecording},
    types::{
        MAX_RECORDING_DURATION_SECONDS, RecordingEndReason, RecordingInfo, RecordingStatus,
        ShutdownRecordingResult, TARGET_WAV_SAMPLE_RATE, WAV_HEADER_LEN,
    },
    wav::{
        encode_pcm16_wav, encode_recording_wav, float_to_pcm16, read_u16_le, read_u32_le,
        resample_mono_linear,
    },
};

type SharedLevelEmitter = Arc<Mutex<LevelEmitterFn>>;
type LevelEmitterFn = Box<dyn Fn(f32) + Send + Sync>;

pub struct RecordingManager {
    backend: Box<dyn RecordingInput>,
    state: Arc<Mutex<ManagerState>>,
    max_duration: Duration,
    watchdog_grace: Duration,
    emitter: Mutex<Option<LevelEmitterHandle>>,
    watchdog: Mutex<Option<WatchdogHandle>>,
    emit_level: SharedLevelEmitter,
}

struct LevelEmitterHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

struct WatchdogHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct ManagerState {
    active: Option<ActiveRecording>,
    latest: Option<CompletedRecording>,
    last_error: Option<RecordingError>,
}

struct ActiveRecording {
    _stream: Box<dyn RecordingStream>,
    buffer: buffer::SharedBuffer,
    meter: Arc<LevelMeter>,
}

pub(super) struct CompletedRecording {
    info: RecordingInfo,
    wav_bytes: Vec<u8>,
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

impl WatchdogHandle {
    fn stop_and_join(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for WatchdogHandle {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

impl RecordingManager {
    pub fn with_cpal() -> Self {
        Self::new(Box::new(input::CpalInputBackend))
    }

    pub fn new(backend: Box<dyn RecordingInput>) -> Self {
        Self::new_with_emitter(backend, Box::new(no_op_emit))
    }

    pub fn new_with_emitter(
        backend: Box<dyn RecordingInput>,
        emit_level: LevelEmitterFn,
    ) -> Self {
        Self::new_with_options(
            backend,
            emit_level,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            Duration::from_secs(types::DEFAULT_WATCHDOG_GRACE_SECONDS),
        )
    }

    pub fn new_with_options(
        backend: Box<dyn RecordingInput>,
        emit_level: LevelEmitterFn,
        max_duration: Duration,
        watchdog_grace: Duration,
    ) -> Self {
        Self {
            backend,
            state: Arc::new(Mutex::new(ManagerState::default())),
            max_duration,
            watchdog_grace,
            emitter: Mutex::new(None),
            watchdog: Mutex::new(None),
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
                let error = RecordingError {
                    code: RecordingErrorCode::AlreadyRecording,
                    message: "A recording is already in progress.".to_string(),
                };
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

        if let Err(error) = self.start_level_emitter(meter) {
            if let Ok(mut state) = self.state.lock() {
                if let Some(active) = state.active.take() {
                    drop(active);
                }
                state.last_error = Some(error.clone());
            }
            self.stop_watchdog();
            return Err(error);
        }

        self.start_watchdog();

        Ok({
            let state = self.lock_state()?;
            self.status_from_state(&state)
        })
    }

    pub fn stop_recording(&self) -> Result<RecordingInfo, RecordingError> {
        self.stop_watchdog();

        let mut state = self.lock_state()?;

        let Some(active) = state.active.take() else {
            let error = RecordingError {
                code: RecordingErrorCode::NotRecording,
                message: "No recording is currently in progress.".to_string(),
            };
            state.last_error = Some(error.clone());
            return Err(error);
        };

        self.stop_level_emitter_and_reset(&active.meter);

        match finalize_active(active, RecordingEndReason::Manual) {
            Ok(completed) => {
                let info = completed.info.clone();
                state.latest = Some(completed);
                state.last_error = None;
                Ok(info)
            }
            Err(error) if error.code == RecordingErrorCode::EmptyRecording => {
                state.last_error = Some(error.clone());
                Err(error)
            }
            Err(_error) => {
                let surfaced = RecordingError {
                    code: RecordingErrorCode::StopFailed,
                    message: "Recording failed".to_string(),
                };
                state.last_error = Some(surfaced.clone());
                Err(surfaced)
            }
        }
    }

    pub fn stop_for_shutdown(&self) -> Result<ShutdownRecordingResult, RecordingError> {
        self.stop_watchdog();

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
            RecordingError {
                code: RecordingErrorCode::Internal,
                message: "Recording state could not be locked.".to_string(),
            }
        })
    }

    fn state_arc(&self) -> Arc<Mutex<ManagerState>> {
        Arc::clone(&self.state)
    }

    fn finalize_finished_active(
        &self,
        state: &mut ManagerState,
    ) -> Result<(), RecordingError> {
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
                        state.last_error = Some(RecordingError {
                            code: RecordingErrorCode::DeviceDisconnected,
                            message: "The input device disconnected while recording.".to_string(),
                        });
                    } else if completed.info.ended_reason == RecordingEndReason::WatchdogTimeout {
                        state.last_error = Some(RecordingError {
                            code: RecordingErrorCode::WatchdogTimeout,
                            message: "Recording failed".to_string(),
                        });
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

        self.stop_watchdog();

        Ok(())
    }

    fn status_from_state(&self, state: &ManagerState) -> RecordingStatus {
        let latest_recording = state.latest.as_ref().map(|latest| latest.info.clone());
        let last_error = state.last_error.clone();

        if let Some(active) = &state.active {
            return active
                .buffer
                .lock()
                .map(|buffer| {
                    buffer.status(latest_recording.clone(), last_error)
                })
                .unwrap_or_else(|_| RecordingStatus {
                    is_recording: false,
                    sample_rate: None,
                    input_channels: None,
                    output_channels: types::OUTPUT_CHANNELS,
                    duration_ms: 0,
                    sample_count: 0,
                    started_at_ms: None,
                    max_duration_seconds: MAX_RECORDING_DURATION_SECONDS,
                    latest_recording,
                    last_error: Some(RecordingError {
                        code: RecordingErrorCode::Internal,
                        message: "Recording buffer could not be locked.".to_string(),
                    }),
                });
        }

        RecordingStatus {
            is_recording: false,
            sample_rate: None,
            input_channels: None,
            output_channels: types::OUTPUT_CHANNELS,
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
            .map_err(|_| RecordingError {
                code: RecordingErrorCode::Internal,
                message: "Recording level emitter could not be started.".to_string(),
            })?;

        let mut slot = self.emitter.lock().map_err(|_| RecordingError {
            code: RecordingErrorCode::Internal,
            message: "Recording level emitter could not be started.".to_string(),
        })?;
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

    fn start_watchdog(&self) {
        let timeout = self.max_duration.saturating_add(self.watchdog_grace);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_signal = Arc::clone(&stop);
        let state = self.state_arc();

        let join = match std::thread::Builder::new()
            .name("floe-recording-watchdog".to_string())
            .spawn(move || {
                watchdog_loop(state, stop_signal, timeout);
            }) {
            Ok(join) => join,
            Err(_) => {
                log_watchdog_spawn_failed();
                return;
            }
        };

        if let Ok(mut slot) = self.watchdog.lock() {
            *slot = Some(WatchdogHandle {
                stop,
                join: Some(join),
            });
        }
    }

    fn stop_watchdog(&self) {
        if let Ok(mut slot) = self.watchdog.lock() {
            if let Some(mut handle) = slot.take() {
                handle.stop_and_join();
            }
        }
    }

    #[cfg(test)]
    fn watchdog_handle_is_clear(&self) -> bool {
        self.watchdog
            .lock()
            .map(|slot| slot.is_none())
            .unwrap_or(false)
    }
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

    let mut buffer = buffer.lock().map_err(|_| RecordingError {
        code: RecordingErrorCode::Internal,
        message: "Recording buffer could not be finalized.".to_string(),
    })?;

    buffer.snapshot_completed(default_reason)
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

fn watchdog_loop(
    state: Arc<Mutex<ManagerState>>,
    stop: Arc<AtomicBool>,
    timeout: Duration,
) {
    if timeout.is_zero() {
        return;
    }

    let poll_interval = Duration::from_millis(100);
    let mut elapsed = Duration::ZERO;

    while elapsed < timeout {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let remaining = timeout - elapsed;
        let sleep_duration = remaining.min(poll_interval);
        std::thread::sleep(sleep_duration);
        elapsed += sleep_duration;
    }

    if stop.load(Ordering::SeqCst) {
        return;
    }

    let Ok(mut state) = state.lock() else {
        return;
    };

    let Some(active) = state.active.as_mut() else {
        return;
    };

    if let Ok(mut buffer) = active.buffer.lock() {
        if !buffer.is_finished() {
            buffer.mark_watchdog_timeout();
        }
    };
}

fn log_watchdog_spawn_failed() {
    eprintln!("[floe:lifecycle] level=warn event=recording_watchdog_spawn_failed");
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

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::{
        buffer::RecordingBuffer,
        error::RecordingErrorCode,
        input::{RecordingInput, RecordingStream, StartedRecording},
        types::{
            MAX_RECORDING_DURATION_SECONDS, RecordingEndReason, ShutdownRecordingResult,
            WAV_HEADER_LEN,
        },
        wav::read_u32_le,
        RecordingManager,
    };
    use crate::audio::LevelMeter;

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

    fn test_manager(
        buffer: Arc<Mutex<RecordingBuffer>>,
        meter: Arc<LevelMeter>,
    ) -> RecordingManager {
        RecordingManager::new_with_options(
            Box::new(FakeBackend { buffer, meter }),
            Box::new(super::no_op_emit),
            Duration::from_millis(50),
            Duration::from_millis(10),
        )
    }

    #[test]
    fn manager_returns_latest_wav_bytes_without_disk_export() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            8_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = test_manager(Arc::clone(&buffer), Arc::new(LevelMeter::new()));

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
        assert_eq!(info.sample_rate, 8_000);
        assert_eq!(info.wav_sample_rate, 16_000);
        assert_eq!(info.wav_channels, 1);
        assert_eq!(info.wav_format, "wav");
        assert_eq!(wav.len(), WAV_HEADER_LEN + 8);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(read_u32_le(&wav, 24), Some(16_000));
    }

    #[test]
    fn manager_rejects_already_recording_and_not_recording() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = test_manager(Arc::clone(&buffer), Arc::new(LevelMeter::new()));

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
        let manager = test_manager(buffer, Arc::new(LevelMeter::new()));

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
        let manager = test_manager(Arc::clone(&empty_buffer), Arc::new(LevelMeter::new()));
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
        let manager = test_manager(Arc::clone(&buffer), Arc::new(LevelMeter::new()));

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
        let manager = test_manager(Arc::clone(&buffer), Arc::new(LevelMeter::new()));

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
        let manager = test_manager(buffer, Arc::new(LevelMeter::new()));

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
        let manager = test_manager(buffer, Arc::new(LevelMeter::new()));

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
        let manager = test_manager(Arc::clone(&buffer), Arc::new(LevelMeter::new()));

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
        let manager = test_manager(Arc::clone(&buffer), Arc::new(LevelMeter::new()));
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
    fn watchdog_finalizes_recording_when_sample_cap_is_unreached() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = RecordingManager::new_with_options(
            Box::new(FakeBackend {
                buffer: Arc::clone(&buffer),
                meter: Arc::new(LevelMeter::new()),
            }),
            Box::new(super::no_op_emit),
            Duration::from_millis(40),
            Duration::from_millis(20),
        );

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.5_f32, 0.25], &LevelMeter::new());
        std::thread::sleep(Duration::from_millis(80));

        let status = manager.get_recording_status().expect("status succeeds");
        assert!(!status.is_recording);
        let latest = status.latest_recording.expect("watchdog should finalize");
        assert_eq!(latest.ended_reason, RecordingEndReason::WatchdogTimeout);
        assert!(latest.max_duration_reached);
        assert_eq!(
            status.last_error.expect("error surfaced").code,
            RecordingErrorCode::WatchdogTimeout
        );
    }

    #[test]
    fn watchdog_does_not_fire_when_stop_completes_normally() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = RecordingManager::new_with_options(
            Box::new(FakeBackend {
                buffer: Arc::clone(&buffer),
                meter: Arc::new(LevelMeter::new()),
            }),
            Box::new(super::no_op_emit),
            Duration::from_millis(200),
            Duration::from_millis(50),
        );

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.5_f32, 0.5], &LevelMeter::new());
        let info = manager.stop_recording().expect("stop succeeds");

        std::thread::sleep(Duration::from_millis(300));

        assert_eq!(info.ended_reason, RecordingEndReason::Manual);
        assert!(manager.watchdog_handle_is_clear());
    }

    #[test]
    fn watchdog_is_cleared_after_normal_stop() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = test_manager(Arc::clone(&buffer), Arc::new(LevelMeter::new()));

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.5_f32], &LevelMeter::new());
        manager.stop_recording().expect("stop succeeds");

        assert!(manager.watchdog_handle_is_clear());
    }

    #[test]
    fn watchdog_is_not_duplicated_across_repeated_recordings() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = test_manager(Arc::clone(&buffer), Arc::new(LevelMeter::new()));

        for _ in 0..3 {
            manager.start_recording().expect("start succeeds");
            buffer
                .lock()
                .unwrap()
                .append_interleaved(&[0.5_f32], &LevelMeter::new());
            manager.stop_recording().expect("stop succeeds");
            assert!(manager.watchdog_handle_is_clear());
        }

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.5_f32], &LevelMeter::new());
        manager.stop_recording().expect("stop succeeds");
        assert!(manager.watchdog_handle_is_clear());
    }

    #[test]
    fn stop_recording_clears_active_state_when_finalize_fails() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = test_manager(Arc::clone(&buffer), Arc::new(LevelMeter::new()));

        manager.start_recording().expect("start succeeds");
        let state_arc = manager.state_arc();
        {
            let mut state = state_arc.lock().unwrap();
            if let Some(active) = state.active.as_mut() {
                let handle = active.buffer.clone();
                drop(state);
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _guard = handle.lock().unwrap();
                    panic!("poison buffer to simulate finalize failure");
                }));
            }
        }

        let stop_error = manager
            .stop_recording()
            .expect_err("stop_recording should fail when finalize fails");
        assert_eq!(stop_error.code, RecordingErrorCode::StopFailed);
        assert_eq!(stop_error.message, "Recording failed");
        assert!(manager.state_arc().lock().unwrap().active.is_none());

        buffer.clear_poison();
        buffer
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .reset_for_test();
        let next_status = manager
            .start_recording()
            .expect("start_recording should succeed after failed stop");
        assert!(next_status.is_recording);
    }

    #[test]
    fn next_recording_can_start_after_watchdog_timeout() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = RecordingManager::new_with_options(
            Box::new(FakeBackend {
                buffer: Arc::clone(&buffer),
                meter: Arc::new(LevelMeter::new()),
            }),
            Box::new(super::no_op_emit),
            Duration::from_millis(40),
            Duration::from_millis(20),
        );

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.5_f32, 0.25], &LevelMeter::new());
        std::thread::sleep(Duration::from_millis(80));

        let status = manager
            .get_recording_status()
            .expect("status after watchdog succeeds");
        assert!(!status.is_recording);

        buffer
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .reset_for_test();
        let next_status = manager
            .start_recording()
            .expect("start after watchdog succeeds");
        assert!(next_status.is_recording);
    }

    #[test]
    fn stop_recording_returns_quickly_even_with_long_watchdog_timeout() {
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = RecordingManager::new_with_options(
            Box::new(FakeBackend {
                buffer: Arc::clone(&buffer),
                meter: Arc::new(LevelMeter::new()),
            }),
            Box::new(super::no_op_emit),
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            Duration::from_secs(5),
        );

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.5_f32, 0.25], &LevelMeter::new());

        let started = std::time::Instant::now();
        let info = manager.stop_recording().expect("stop succeeds");
        let elapsed = started.elapsed();

        assert_eq!(info.ended_reason, RecordingEndReason::Manual);
        assert!(
            elapsed < Duration::from_secs(2),
            "stop_recording took {:?}, expected < 2s",
            elapsed,
        );
    }
}
