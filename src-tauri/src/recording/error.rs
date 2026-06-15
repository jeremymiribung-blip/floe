use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecordingError {
    pub domain: &'static str,
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
    StopFailed,
    WatchdogTimeout,
    AppShuttingDown,
    Internal,
}

pub fn recording_error(code: RecordingErrorCode, message: &'static str) -> RecordingError {
    RecordingError {
        domain: "recording",
        code,
        message: message.to_string(),
    }
}

#[allow(dead_code)]
pub fn internal_error() -> RecordingError {
    recording_error(
        RecordingErrorCode::Internal,
        "Recording level emitter could not be started.",
    )
}

#[allow(dead_code)]
pub fn wav_encoding_error() -> RecordingError {
    recording_error(
        RecordingErrorCode::WavEncodingFailed,
        "Recording WAV bytes could not be encoded.",
    )
}
