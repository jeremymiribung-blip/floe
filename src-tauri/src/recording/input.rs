use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio::LevelMeter;

use super::{
    buffer::{RecordingBuffer, SharedBuffer},
    error::{recording_error, RecordingError, RecordingErrorCode},
    sample::AudioSample,
};

pub trait RecordingInput: Send + Sync + 'static {
    fn start_recording(
        &self,
        max_duration: std::time::Duration,
    ) -> Result<StartedRecording, RecordingError>;
}

pub trait RecordingStream: Send + 'static {}

pub struct StartedRecording {
    pub stream: Box<dyn RecordingStream>,
    pub buffer: SharedBuffer,
    pub meter: Arc<LevelMeter>,
}

pub struct CpalInputBackend;

pub struct CpalRecordingStream {
    _stream: cpal::Stream,
}

impl RecordingStream for CpalRecordingStream {}

impl RecordingInput for CpalInputBackend {
    fn start_recording(
        &self,
        max_duration: std::time::Duration,
    ) -> Result<StartedRecording, RecordingError> {
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
        let buffer = Arc::new(std::sync::Mutex::new(RecordingBuffer::new(
            sample_rate,
            input_channels,
            max_duration,
            super::now_ms(),
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
                if let Ok(mut buffer) = data_buffer.try_lock() {
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
