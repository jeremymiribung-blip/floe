use serde::Serialize;

use super::error::RecordingError;

pub const MAX_RECORDING_DURATION_SECONDS: u64 = 120;
pub const DEFAULT_WATCHDOG_GRACE_SECONDS: u64 = 5;
pub const TARGET_WAV_SAMPLE_RATE: u32 = 16_000;
pub const OUTPUT_CHANNELS: u16 = 1;
pub const WAV_FORMAT: &str = "wav";
pub const WAV_HEADER_LEN: usize = 44;
pub const WAV_AUDIO_FORMAT_PCM: u16 = 1;
pub const WAV_BITS_PER_SAMPLE: u16 = 16;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingInfo {
    pub sample_rate: u32,
    pub input_channels: u16,
    pub output_channels: u16,
    pub wav_format: &'static str,
    pub wav_sample_rate: u32,
    pub wav_channels: u16,
    pub duration_ms: u64,
    pub sample_count: u64,
    pub wav_byte_count: u64,
    pub wav_bits_per_sample: u16,
    pub recording_stop_to_encode_start_ms: u64,
    pub audio_encode_ms: u64,
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
    WatchdogTimeout,
    DeviceDisconnected,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownRecordingResult {
    Idle,
    Finalized,
    DiscardedEmpty,
}
