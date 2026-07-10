use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::audio::LevelMeter;

use super::{
    error::{recording_error, RecordingError, RecordingErrorCode},
    sample::AudioSample,
    types::{
        RecordingEndReason, RecordingInfo, RecordingStatus, OUTPUT_CHANNELS,
        TARGET_WAV_SAMPLE_RATE, WAV_BITS_PER_SAMPLE, WAV_FORMAT,
    },
    wav::encode_recording_wav,
};

pub type SharedBuffer = Arc<Mutex<RecordingBuffer>>;

pub struct RecordingBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub input_channels: u16,
    max_samples: usize,
    /// Hard byte ceiling as defense-in-depth (independent of duration).
    /// Prevents unbounded memory growth if duration gates are bypassed.
    max_bytes: usize,
    started_at_ms: u64,
    ended_at_ms: Option<u64>,
    end_reason: Option<RecordingEndReason>,
}

const MAX_BUFFER_BYTES: usize = 25 * 1024 * 1024; // 25 MiB hard cap (~25M f32 samples worst case)

impl RecordingBuffer {
    pub fn new(
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
            max_bytes: MAX_BUFFER_BYTES,
            started_at_ms,
            ended_at_ms: None,
            end_reason: None,
        }
    }

    pub fn append_interleaved<T: AudioSample>(&mut self, input: &[T], level_meter: &LevelMeter) {
        if self.is_finished() || input.is_empty() || self.input_channels == 0 {
            return;
        }

        let channels = self.input_channels as usize;
        let available_samples = self.max_samples.saturating_sub(self.samples.len());
        let available_frames = input.len() / channels;
        let frames_to_take = available_frames.min(available_samples);

        // Hard byte ceiling defense (in addition to sample count)
        let current_bytes = self.samples.len() * std::mem::size_of::<f32>();
        let available_bytes = self.max_bytes.saturating_sub(current_bytes);
        let max_samples_by_bytes = available_bytes / std::mem::size_of::<f32>();
        let frames_to_take = frames_to_take.min(max_samples_by_bytes / channels.max(1));

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
        } else if self.samples.len() * std::mem::size_of::<f32>() >= self.max_bytes {
            self.finish(RecordingEndReason::MaxDuration);
        }
    }

    pub fn mark_device_disconnected(&mut self) {
        self.finish(RecordingEndReason::DeviceDisconnected);
    }

    pub fn mark_watchdog_timeout(&mut self) {
        self.finish(RecordingEndReason::WatchdogTimeout);
    }

    #[cfg(test)]
    pub fn reset_for_test(&mut self) {
        self.end_reason = None;
        self.ended_at_ms = None;
    }

    pub fn is_finished(&self) -> bool {
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

    pub fn status(
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
            max_duration_seconds: super::MAX_RECORDING_DURATION_SECONDS,
            latest_recording,
            last_error,
            trace_id: None,
        }
    }

    #[cfg(test)]
    pub(super) fn into_completed(
        mut self,
        default_reason: RecordingEndReason,
    ) -> Result<super::CompletedRecording, RecordingError> {
        self.snapshot_completed(default_reason)
    }

    pub(super) fn snapshot_completed(
        &mut self,
        default_reason: RecordingEndReason,
    ) -> Result<super::CompletedRecording, RecordingError> {
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
        let ended_at_ms = self.ended_at_ms.unwrap_or_else(super::now_ms);
        // Capture the time when recording stopped (after finish() returns)
        let recording_stopped_at = Instant::now();

        let info = RecordingInfo {
            sample_rate: self.sample_rate,
            input_channels: self.input_channels,
            output_channels: OUTPUT_CHANNELS,
            wav_format: WAV_FORMAT,
            wav_sample_rate: TARGET_WAV_SAMPLE_RATE,
            wav_channels: OUTPUT_CHANNELS,
            duration_ms: self.duration_ms(),
            sample_count: self.samples.len() as u64,
            wav_byte_count: 0,
            wav_bits_per_sample: WAV_BITS_PER_SAMPLE,
            recording_stop_to_encode_start_ms: 0, // Will be updated after encoding starts
            audio_encode_ms: 0,
            started_at_ms: self.started_at_ms,
            ended_at_ms,
            max_duration_reached: ended_reason == RecordingEndReason::MaxDuration
                || ended_reason == RecordingEndReason::WatchdogTimeout,
            ended_reason,
        };
        let encode_started = Instant::now();
        // Measure the time between recording stop and encoding start
        let recording_stop_to_encode_start_ms = recording_stopped_at.elapsed().as_millis() as u64;
        let wav_bytes = encode_recording_wav(&self.samples, self.sample_rate)?;
        let info = RecordingInfo {
            wav_byte_count: wav_bytes.len() as u64,
            audio_encode_ms: super::elapsed_ms(encode_started),
            recording_stop_to_encode_start_ms,
            ..info
        };

        Ok(super::CompletedRecording { info, wav_bytes })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{RecordingBuffer, RecordingEndReason};
    use crate::audio::LevelMeter;

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
        assert_eq!(completed.info.wav_format, "wav");
        assert_eq!(
            completed.info.wav_sample_rate,
            super::TARGET_WAV_SAMPLE_RATE
        );
        assert_eq!(completed.info.wav_channels, 1);
        assert_eq!(completed.info.sample_count, 3);
        assert_eq!(completed.info.duration_ms, 3);
        assert_eq!(completed.info.wav_byte_count, 140);
        assert_eq!(completed.info.wav_bits_per_sample, 16);
        assert_eq!(completed.info.recording_stop_to_encode_start_ms, 0);
        assert!(completed.info.audio_encode_ms < 1_000);
        assert_eq!(completed.info.started_at_ms, 10_000);
        assert_eq!(completed.info.ended_at_ms, 10_003);
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
    fn device_disconnect_finalizes_status_without_microphone() {
        let mut buffer = RecordingBuffer::new(48_000, 1, Duration::from_secs(120), 1000);
        let _meter = LevelMeter::new();

        buffer.append_interleaved(&[0.25_f32, 0.25], &LevelMeter::new());
        buffer.mark_device_disconnected();

        let completed = buffer
            .into_completed(RecordingEndReason::DeviceDisconnected)
            .expect("buffer should complete with device disconnected");
        assert_eq!(
            completed.info.ended_reason,
            RecordingEndReason::DeviceDisconnected
        );
    }
}
