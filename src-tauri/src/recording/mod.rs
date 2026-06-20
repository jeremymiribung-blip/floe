use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::task::JoinHandle;

use crate::audio::{fold_level, normalize_rms, LevelMeter, EMIT_INTERVAL_MS};

mod buffer;
mod error;
mod input;
mod sample;
mod types;
mod wav;

#[allow(unused_imports)]
pub use self::{
    buffer::RecordingBuffer,
    error::{RecordingError, RecordingErrorCode},
    input::{RecordingInput, RecordingStream, StartedRecording},
    types::{
        RecordingEndReason, RecordingInfo, RecordingState, RecordingStatePayload, RecordingStatus,
        ShutdownRecordingResult, MAX_RECORDING_DURATION_SECONDS, WAV_HEADER_LEN,
    },
    wav::encode_pcm16_wav,
};

type SharedLevelEmitter = Arc<Mutex<LevelEmitterFn>>;
type LevelEmitterFn = Box<dyn Fn(f32) + Send + Sync>;

type SharedStateEmitter = Arc<Mutex<StateEmitterFn>>;
type StateEmitterFn = Box<dyn Fn(types::RecordingState) + Send + Sync>;

/// Manages audio recording lifecycle including start, stop, and watchdog timeout.
///
/// # Watchdog/Stop Race Condition
///
/// There is a known race between [`stop_recording`](RecordingManager::stop_recording) and the
/// watchdog thread. When `stop_recording` calls [`stop_watchdog`] (which sets the `stop` flag
/// and aborts the watchdog join handle), the watchdog thread may already be past its
/// [`stop.load()`](std::sync::atomic::AtomicBool::load) check and blocked on
/// [`state.lock()`](std::sync::Mutex::lock). In that window the watchdog can mark the
/// recording buffer as finished (via `mark_watchdog_timeout`) after
/// [`stop_recording`](RecordingManager::stop_recording) has already taken ownership of the
/// `active` recording with `Option::take`.
///
/// ## Current Mitigation
///
/// Both [`start_recording`](RecordingManager::start_recording) and
/// [`stop_recording`](RecordingManager::stop_recording) call
/// [`try_finalize_if_finished`](RecordingManager::try_finalize_if_finished) as their first
/// action. This ensures that if the watchdog has already marked the buffer as finished, the
/// recording is finalized before the rest of the stop logic proceeds. Additionally, the
/// watchdog always checks `state.active.as_mut()` after acquiring the lock and returns
/// early when the active recording has already been taken.
///
/// ## Consequence
///
/// In the worst case, both the watchdog path and `stop_recording` could attempt to finalize
/// the same recording (a double-finalize). This is mitigated by `Option::take` which
/// moves the `Option<ActiveRecording>` out, and by the fact that
/// [`finalize_active`] consumes the recording by value. If the active has already been taken,
/// any subsequent attempt to finalize it is a no-op because the buffer is still accessible
/// through the shared `Arc` and its `is_finished()` flag is idempotent.
///
/// ## Known Limitation
///
/// This is a known limitation of the current synchronous architecture. The long-term fix
/// (see prompt 5 in the design spec) is to convert the recording lifecycle to an async
/// state machine where the watchdog and stop logic are serialised through a single async
/// task, eliminating the race entirely.
pub struct RecordingManager {
    backend: Box<dyn RecordingInput>,
    state: Arc<Mutex<ManagerState>>,
    max_duration: Duration,
    watchdog_grace: Duration,
    emit_level: SharedLevelEmitter,
    emit_state: SharedStateEmitter,
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
    recording_state: types::RecordingState,
    active: Option<ActiveRecording>,
    latest: Option<CompletedRecording>,
    last_error: Option<RecordingError>,
    emitter_handle: Option<LevelEmitterHandle>,
    watchdog_handle: Option<WatchdogHandle>,
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
            handle.abort();
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
            handle.abort();
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

    pub fn new_with_emitter(backend: Box<dyn RecordingInput>, emit_level: LevelEmitterFn) -> Self {
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
            emit_level: Arc::new(Mutex::new(emit_level)),
            emit_state: Arc::new(Mutex::new(Box::new(no_op_state_emit))),
        }
    }

    pub fn set_level_emitter(&self, emit_level: LevelEmitterFn) {
        if let Ok(mut slot) = self.emit_level.lock() {
            *slot = emit_level;
        }
    }

    pub fn set_state_emitter(&self, emit_state_fn: StateEmitterFn) {
        if let Ok(mut slot) = self.emit_state.lock() {
            *slot = emit_state_fn;
        }
    }

    /// Emit without taking the state lock. Call when state lock is already held.
    fn emit_only(&self, new_state: types::RecordingState) {
        if let Ok(emit) = self.emit_state.lock() {
            (emit)(new_state);
        }
    }

    pub fn start_recording(&self) -> Result<RecordingStatus, RecordingError> {
        // Guard: finalize any finished recording before checking can_start
        self.try_finalize_if_finished()?;

        let mut state = self.lock_state()?;
        if let Err(code) = state.recording_state.can_start() {
            let error = RecordingError {
                domain: "recording",
                code,
                message: "A recording is already in progress.".to_string(),
            };
            state.last_error = Some(error.clone());
            return Err(error);
        }
        state.recording_state = types::RecordingState::Starting;
        self.emit_only(types::RecordingState::Starting);
        drop(state);

        let started = self.backend.start_recording(self.max_duration)?;
        let meter = Arc::clone(&started.meter);

        let mut state = self.lock_state()?;
        state.last_error = None;
        state.active = Some(ActiveRecording {
            _stream: started.stream,
            buffer: started.buffer,
            meter: Arc::clone(&meter),
        });

        if let Err(error) = start_level_emitter(meter, Arc::clone(&self.emit_level), &mut state) {
            state.active = None;
            state.last_error = Some(error.clone());
            drop(state);
            self.emit_only(types::RecordingState::Idle);
            return Err(error);
        }

        start_watchdog_thread(
            &mut state,
            Arc::clone(&self.state),
            self.max_duration.saturating_add(self.watchdog_grace),
        );
        state.recording_state = types::RecordingState::Recording;
        let status = self.status_from_state(&state);
        drop(state);
        self.emit_only(types::RecordingState::Recording);

        Ok(status)
    }

    pub fn stop_recording(&self) -> Result<RecordingInfo, RecordingError> {
        // Guard: finalize any finished recording before checking can_stop
        let did_finalize = self.try_finalize_if_finished()?;

        // If try_finalize_if_finished already finalized (e.g. max duration),
        // return the result directly
        if did_finalize {
            let state = self.lock_state()?;
            if let Some(latest) = &state.latest {
                return Ok(latest.info.clone());
            }
        }

        let (active, meter) = {
            let mut state = self.lock_state()?;

            stop_watchdog(&mut state);

            if let Err(code) = state.recording_state.can_stop() {
                let error = RecordingError {
                    domain: "recording",
                    code,
                    message: "No recording is currently in progress.".to_string(),
                };
                state.last_error = Some(error.clone());
                return Err(error);
            }

            let active = match state.active.take() {
                Some(active) => {
                    state.recording_state = types::RecordingState::Stopping;
                    active
                }
                None => {
                    let error = RecordingError {
                        domain: "recording",
                        code: RecordingErrorCode::NotRecording,
                        message: "No recording is currently in progress.".to_string(),
                    };
                    state.last_error = Some(error.clone());
                    return Err(error);
                }
            };
            let meter = Arc::clone(&active.meter);
            (active, meter)
        };
        self.emit_only(types::RecordingState::Stopping);

        {
            let mut state = self.lock_state()?;
            stop_emitter(&mut state);
        }
        meter.store(0.0);
        if let Ok(emit_slot) = self.emit_level.lock() {
            (emit_slot)(0.0);
        }

        let result = finalize_active(active, RecordingEndReason::Manual);

        self.emit_only(types::RecordingState::Idle);

        let mut state = self.lock_state()?;
        match result {
            Ok(completed) => {
                state.recording_state = types::RecordingState::Idle;
                let info = completed.info.clone();
                state.latest = Some(completed);
                state.last_error = None;
                Ok(info)
            }
            Err(error) if error.code == RecordingErrorCode::EmptyRecording => {
                state.recording_state = types::RecordingState::Idle;
                state.last_error = Some(error.clone());
                Err(error)
            }
            Err(_error) => {
                state.recording_state = types::RecordingState::Idle;
                let surfaced = RecordingError {
                    domain: "recording",
                    code: RecordingErrorCode::StopFailed,
                    message: "Recording failed".to_string(),
                };
                state.last_error = Some(surfaced.clone());
                Err(surfaced)
            }
        }
    }

    pub fn stop_for_shutdown(&self) -> Result<ShutdownRecordingResult, RecordingError> {
        // Finalize any finished recording before checking state
        self.try_finalize_if_finished()?;

        let (active, meter) = {
            let mut state = self.lock_state()?;

            stop_watchdog(&mut state);

            let Some(active) = state.active.take() else {
                return Ok(ShutdownRecordingResult::Idle);
            };

            state.recording_state = types::RecordingState::Stopping;
            let meter = Arc::clone(&active.meter);
            (active, meter)
        };

        {
            let mut state = self.lock_state()?;
            stop_emitter(&mut state);
        }
        meter.store(0.0);
        if let Ok(emit_slot) = self.emit_level.lock() {
            (emit_slot)(0.0);
        }

        let result = finalize_active(active, RecordingEndReason::Shutdown);

        {
            let mut state = self.lock_state()?;
            state.recording_state = types::RecordingState::Idle;
        }

        match result {
            Ok(completed) => {
                let mut state = self.lock_state()?;
                state.latest = Some(completed);
                state.last_error = None;
                Ok(ShutdownRecordingResult::Finalized)
            }
            Err(error) if error.code == RecordingErrorCode::EmptyRecording => {
                let mut state = self.lock_state()?;
                state.last_error = None;
                Ok(ShutdownRecordingResult::DiscardedEmpty)
            }
            Err(error) => {
                let mut state = self.lock_state()?;
                state.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub fn get_recording_status(&self) -> Result<RecordingStatus, RecordingError> {
        let state = self.lock_state()?;

        Ok(self.status_from_state(&state))
    }

    pub fn get_latest_recording_info(&self) -> Result<Option<RecordingInfo>, RecordingError> {
        let state = self.lock_state()?;

        Ok(state.latest.as_ref().map(|latest| latest.info.clone()))
    }

    pub fn get_latest_recording_wav_bytes(&self) -> Result<Option<Vec<u8>>, RecordingError> {
        let state = self.lock_state()?;

        Ok(state.latest.as_ref().map(|latest| latest.wav_bytes.clone()))
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, ManagerState>, RecordingError> {
        self.state.lock().map_err(|_| RecordingError {
            domain: "recording",
            code: RecordingErrorCode::Internal,
            message: "Recording state could not be locked.".to_string(),
        })
    }

    #[allow(dead_code)]
    fn state_arc(&self) -> Arc<Mutex<ManagerState>> {
        Arc::clone(&self.state)
    }

    /// Returns `true` if a recording was finalized, `false` otherwise.
    fn try_finalize_if_finished(&self) -> Result<bool, RecordingError> {
        let mut state = self.lock_state()?;
        if state.recording_state != types::RecordingState::Recording {
            return Ok(false);
        }

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
            return Ok(false);
        }

        if let Some(active) = state.active.take() {
            stop_emitter(&mut state);
            active.meter.store(0.0);
            match finalize_active(active, RecordingEndReason::Manual) {
                Ok(completed) => {
                    if completed.info.ended_reason == RecordingEndReason::DeviceDisconnected {
                        state.last_error = Some(RecordingError {
                            domain: "recording",
                            code: RecordingErrorCode::DeviceDisconnected,
                            message: "The input device disconnected while recording.".to_string(),
                        });
                    } else if completed.info.ended_reason == RecordingEndReason::WatchdogTimeout {
                        state.last_error = Some(RecordingError {
                            domain: "recording",
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
                Err(error) => {
                    // Clean up state even on finalize failure
                    stop_watchdog(&mut state);
                    state.recording_state = types::RecordingState::Idle;
                    self.emit_only(types::RecordingState::Idle);
                    return Err(error);
                }
            }
        }

        stop_watchdog(&mut state);
        state.recording_state = types::RecordingState::Idle;
        self.emit_only(types::RecordingState::Idle);

        Ok(true)
    }

    #[allow(dead_code)]
    pub fn poll_finalize(&self) -> Result<(), RecordingError> {
        self.try_finalize_if_finished()?;
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
                    output_channels: types::OUTPUT_CHANNELS,
                    duration_ms: 0,
                    sample_count: 0,
                    started_at_ms: None,
                    max_duration_seconds: MAX_RECORDING_DURATION_SECONDS,
                    latest_recording,
                    last_error: Some(RecordingError {
                        domain: "recording",
                        code: RecordingErrorCode::Internal,
                        message: "Recording buffer could not be locked.".to_string(),
                    }),
                    trace_id: None,
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
            trace_id: None,
        }
    }

    #[cfg(test)]
    fn watchdog_handle_is_clear(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.watchdog_handle.is_none())
            .unwrap_or(false)
    }
}

fn start_level_emitter(
    meter: Arc<LevelMeter>,
    emit_level: SharedLevelEmitter,
    state: &mut ManagerState,
) -> Result<(), RecordingError> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_signal = Arc::clone(&stop);
    let meter_for_thread = Arc::clone(&meter);

    let join = tokio::task::spawn_blocking(move || {
        level_emitter_loop(meter_for_thread, stop_signal, emit_level);
    });

    state.emitter_handle = Some(LevelEmitterHandle {
        stop,
        join: Some(join),
    });

    Ok(())
}

fn stop_emitter(state: &mut ManagerState) {
    if let Some(mut handle) = state.emitter_handle.take() {
        handle.stop_and_join();
    }
}

fn start_watchdog_thread(
    state: &mut ManagerState,
    state_arc: Arc<Mutex<ManagerState>>,
    timeout: Duration,
) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_signal = Arc::clone(&stop);

    let join = tokio::task::spawn_blocking(move || {
        watchdog_loop(state_arc, stop_signal, timeout);
    });

    state.watchdog_handle = Some(WatchdogHandle {
        stop,
        join: Some(join),
    });
}

fn stop_watchdog(state: &mut ManagerState) {
    if let Some(mut handle) = state.watchdog_handle.take() {
        handle.stop_and_join();
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
        domain: "recording",
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

fn watchdog_loop(state: Arc<Mutex<ManagerState>>, stop: Arc<AtomicBool>, timeout: Duration) {
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

fn no_op_emit(_level: f32) {}

fn no_op_state_emit(_state: types::RecordingState) {}

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
            RecordingEndReason, ShutdownRecordingResult, MAX_RECORDING_DURATION_SECONDS,
            WAV_HEADER_LEN,
        },
        wav::read_u32_le,
        RecordingManager,
    };
    use crate::audio::LevelMeter;
    use tokio::runtime::Runtime;

    fn create_test_runtime() -> Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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

        manager.poll_finalize().expect("poll_finalize succeeds");
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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

        manager.poll_finalize().expect("poll_finalize succeeds");
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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
        // try_finalize_if_finished now catches the poisoned buffer error
        assert_eq!(stop_error.code, RecordingErrorCode::Internal);
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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

        manager.poll_finalize().expect("poll_finalize succeeds");
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
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
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

    #[test]
    fn concurrent_start_stop_does_not_deadlock() {
        let rt = create_test_runtime();

        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = Arc::new(test_manager(
            Arc::clone(&buffer),
            Arc::new(LevelMeter::new()),
        ));

        rt.block_on(async {
            let mut handles = Vec::new();

            for i in 0..10 {
                let m = Arc::clone(&manager);
                let b = Arc::clone(&buffer);
                handles.push(tokio::spawn(async move {
                    if i % 2 == 0 {
                        tokio::task::spawn_blocking(move || {
                            let _ = m.start_recording();
                            b.lock()
                                .unwrap_or_else(|poison| poison.into_inner())
                                .append_interleaved(&[0.5_f32], &LevelMeter::new());
                            let _ = m.stop_recording();
                        })
                        .await
                        .unwrap();
                    } else {
                        tokio::task::spawn_blocking(move || {
                            let _ = m.get_recording_status();
                        })
                        .await
                        .unwrap();
                    }
                }));
            }

            for h in handles {
                h.await.expect("task panicked");
            }
        });

        let status = manager.get_recording_status().unwrap();
        assert!(!status.is_recording);
        assert!(manager.watchdog_handle_is_clear());
    }

    #[test]
    fn shutdown_during_recording_is_safe() {
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = Arc::new(test_manager(
            Arc::clone(&buffer),
            Arc::new(LevelMeter::new()),
        ));

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.5_f32], &LevelMeter::new());

        let m = Arc::clone(&manager);
        let shutdown = std::thread::spawn(move || m.stop_for_shutdown().ok());

        let _shutdown_result = shutdown.join().expect("shutdown thread panicked");
        let final_status = manager.get_recording_status().unwrap();
        assert!(!final_status.is_recording);
    }

    #[test]
    fn status_query_during_recording_does_not_panic() {
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = Arc::new(test_manager(
            Arc::clone(&buffer),
            Arc::new(LevelMeter::new()),
        ));

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.5_f32], &LevelMeter::new());

        let mut handles = Vec::new();
        for _ in 0..8 {
            let m = Arc::clone(&manager);
            handles.push(std::thread::spawn(move || {
                for _ in 0..5 {
                    let _ = m.get_recording_status();
                    let _ = m.get_latest_recording_info();
                    std::thread::sleep(Duration::from_millis(1));
                }
            }));
        }
        for h in handles {
            h.join().expect("reader thread panicked");
        }
        let _ = manager.stop_recording();
        let status = manager.get_recording_status().unwrap();
        assert!(!status.is_recording);
    }

    #[test]
    fn watchdog_stop_race_does_not_corrupt_state() {
        let _rt = create_test_runtime();
        let _guard = _rt.enter();
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(MAX_RECORDING_DURATION_SECONDS),
            1000,
        )));
        let manager = Arc::new(RecordingManager::new_with_options(
            Box::new(FakeBackend {
                buffer: Arc::clone(&buffer),
                meter: Arc::new(LevelMeter::new()),
            }),
            Box::new(super::no_op_emit),
            Duration::from_millis(30),
            Duration::from_millis(10),
        ));

        manager.start_recording().expect("start succeeds");
        buffer
            .lock()
            .unwrap()
            .append_interleaved(&[0.5_f32, 0.25], &LevelMeter::new());

        // Wait for the watchdog to fire and mark the buffer as finished.
        // macOS CI runners are slower, so allow extra time.
        let wait_ms = if cfg!(target_os = "macos") { 200 } else { 60 };
        std::thread::sleep(Duration::from_millis(wait_ms));

        // Spawn a concurrent status reader to add state/buffer contention
        // during the race window.
        let m = Arc::clone(&manager);
        let reader = std::thread::spawn(move || {
            // Short delay to increase the chance of overlapping with stop_recording
            std::thread::sleep(Duration::from_millis(5));
            let _ = m.get_recording_status();
        });

        // stop_recording should detect the watchdog-completed buffer via
        // try_finalize_if_finished and return the finalized info.
        let result = manager.stop_recording();

        reader.join().expect("reader thread panicked");

        // Verify state is consistent: no panic, no poison, final state is Idle.
        let info = result.expect("stop should succeed after watchdog timeout");
        assert_eq!(
            info.ended_reason,
            RecordingEndReason::WatchdogTimeout,
            "watchdog should have set WatchdogTimeout reason"
        );
        assert!(
            info.max_duration_reached,
            "max_duration_reached should be true for watchdog timeout"
        );

        let status = manager
            .get_recording_status()
            .expect("status query after race should succeed");
        assert!(!status.is_recording, "final state should be idle");
        assert!(
            manager.watchdog_handle_is_clear(),
            "watchdog handle should have been cleared"
        );
        assert_eq!(
            status
                .last_error
                .as_ref()
                .expect("last_error should contain WatchdogTimeout")
                .code,
            RecordingErrorCode::WatchdogTimeout
        );

        // Verify the manager can start a new recording (state is clean).
        buffer
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .reset_for_test();
        let next = manager
            .start_recording()
            .expect("should be able to start a new recording after race");
        assert!(next.is_recording);
    }
}
