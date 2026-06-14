use serde::Serialize;

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
    StopFailed,
    WatchdogTimeout,
    Internal,
}

pub fn recording_error(code: RecordingErrorCode, message: &'static str) -> RecordingError {
    RecordingError {
        code,
        message: message.to_string(),
    }
}

pub fn internal_error() -> RecordingError {
    recording_error(
        RecordingErrorCode::Internal,
        "Recording level emitter could not be started.",
    )
}

pub fn wav_encoding_error() -> RecordingError {
    recording_error(
        RecordingErrorCode::WavEncodingFailed,
        "Recording WAV bytes could not be encoded.",
    )
}
