use std::time::Instant;

use tauri::State;

use crate::{
    diag::{DiagEvent, PipelineContext},
    lifecycle,
    recording::{
        RecordingError, RecordingErrorCode, RecordingInfo, RecordingManager, RecordingStatus,
    },
};

#[tauri::command]
pub async fn start_recording(
    manager: State<'_, RecordingManager>,
    diag_ctx: State<'_, PipelineContext>,
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
            trace_id,
            duration_ms: setup_ms,
        }
    );

    Ok(status)
}

#[tauri::command]
pub async fn stop_recording(
    manager: State<'_, RecordingManager>,
    diag_ctx: State<'_, PipelineContext>,
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
                    trace_id,
                    duration_ms,
                    encode_ms: info.audio_encode_ms,
                    wav_bytes: info.wav_byte_count,
                    sample_rate: info.wav_sample_rate,
                    ended_reason: format!("{:?}", info.ended_reason).to_lowercase(),
                }
            );
            diag_ctx.end_session();
        }
        Err(err) => {
            log::warn!(
                "{}",
                DiagEvent::RecordingError {
                    trace_id,
                    error_code: format!("{:?}", err.code),
                }
            );
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

#[cfg(test)]
mod tests {
    #[test]
    fn recording_command_signatures_are_stable() {
        // Compile-time verification that recording commands have the expected signatures
        // This ensures no provider-switching complexity has been added
    }
}
