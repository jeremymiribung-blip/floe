use std::time::Instant;

use tauri::State;

use crate::{
    diag::{DiagEvent, LastSessionStore, PipelineContext},
    lifecycle,
    recording::{
        RecordingError, RecordingErrorCode, RecordingInfo, RecordingManager, RecordingStatus,
    },
};

#[tauri::command]
pub async fn start_recording(
    manager: State<'_, RecordingManager>,
    diag_ctx: State<'_, PipelineContext>,
    last_session: State<'_, LastSessionStore>,
) -> Result<RecordingStatus, RecordingError> {
    if !lifecycle::can_accept_commands() {
        return Err(RecordingError {
            domain: "recording",
            code: RecordingErrorCode::AppShuttingDown,
            message: "Application is shutting down".to_string(),
        });
    }

    let start = Instant::now();
    diag_ctx.end_session();
    let trace_id = diag_ctx.start_session();

    let mut status = manager.start_recording()?;
    status.trace_id = Some(trace_id.clone());

    let setup_ms = start.elapsed().as_millis() as u64;
    log::info!(
        "{}",
        DiagEvent::RecordingStarted {
            trace_id: trace_id.clone(),
            duration_ms: setup_ms,
        }
    );

    // Stash a fresh session snapshot so the diagnostics report has
    // per-stage data even if the user never releases the hotkey.
    // Use set() (not update() with ..snapshot.clone()) so stale fields
    // from a prior incomplete session are never carried forward.
    last_session.set(crate::diag::SessionSnapshot {
        trace_id: Some(trace_id.clone()),
        completed: false,
        recording_setup_ms: setup_ms,
        error_stage: None,
        sanitized_error_code: None,
        last_error: None,
        ..Default::default()
    });

    Ok(status)
}

#[tauri::command]
pub async fn stop_recording(
    manager: State<'_, RecordingManager>,
    diag_ctx: State<'_, PipelineContext>,
    last_session: State<'_, LastSessionStore>,
) -> Result<RecordingInfo, RecordingError> {
    if !lifecycle::can_accept_commands() {
        return Err(RecordingError {
            domain: "recording",
            code: RecordingErrorCode::AppShuttingDown,
            message: "Application is shutting down".to_string(),
        });
    }

    let start = Instant::now();
    let trace_id = diag_ctx.current_trace_id().unwrap_or_default();

    let result = manager.stop_recording();
    let duration_ms = start.elapsed().as_millis() as u64;

    match &result {
        Ok(info) => {
            log::info!(
                "{}",
                DiagEvent::RecordingStopped {
                    trace_id: trace_id.clone(),
                    duration_ms,
                    encode_ms: info.audio_encode_ms,
                    wav_bytes: info.wav_byte_count,
                    sample_rate: info.wav_sample_rate,
                    ended_reason: format!("{:?}", info.ended_reason).to_lowercase(),
                }
            );

            last_session.update(|snapshot| {
                *snapshot = crate::diag::SessionSnapshot {
                    trace_id: Some(trace_id.clone()),
                    completed: false,
                    audio: Some(crate::diag::AudioSnapshot {
                        format: info.wav_format.to_string(),
                        sample_rate: info.wav_sample_rate,
                        channels: info.wav_channels,
                        bits_per_sample: info.wav_bits_per_sample,
                        bytes: info.wav_byte_count,
                        duration_ms: info.duration_ms,
                        ended_reason: format!("{:?}", info.ended_reason).to_lowercase(),
                        max_duration_reached: info.max_duration_reached,
                    }),
                    audio_capture_ms: info.duration_ms,
                    buffering_to_encode_ms: info.recording_stop_to_encode_start_ms,
                    audio_encode_ms: info.audio_encode_ms,
                    recording_started_at_unix_ms: Some(info.started_at_ms),
                    recording_ended_at_unix_ms: Some(info.ended_at_ms),
                    ..snapshot.clone()
                };
            });
        }
        Err(err) => {
            log::warn!(
                "{}",
                DiagEvent::RecordingError {
                    trace_id,
                    error_code: format!("{:?}", err.code),
                }
            );
            last_session.update(|snapshot| {
                snapshot.error_stage = Some("recording".to_string());
                snapshot.sanitized_error_code = Some(format!("{:?}", err.code).to_lowercase());
                snapshot.last_error = Some(crate::diag::LastError {
                    stage: "recording".to_string(),
                    code: format!("{:?}", err.code).to_lowercase(),
                    message: String::new(),
                });
                snapshot.completed = false;
            });
        }
    }

    result
}

#[tauri::command]
pub fn get_recording_status(
    manager: State<'_, RecordingManager>,
    diag_ctx: State<'_, PipelineContext>,
) -> Result<RecordingStatus, RecordingError> {
    let mut status = manager.get_recording_status()?;
    if status.trace_id.is_none() && status.is_recording {
        status.trace_id = diag_ctx.current_trace_id();
    }
    Ok(status)
}

#[tauri::command]
pub fn get_latest_recording_info(
    manager: State<'_, RecordingManager>,
) -> Result<Option<RecordingInfo>, RecordingError> {
    manager.get_latest_recording_info()
}

#[tauri::command]
pub fn force_stop_recording(
    manager: State<'_, RecordingManager>,
) -> Result<(), RecordingError> {
    manager.force_stop_recording()
}
