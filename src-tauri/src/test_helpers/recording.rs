use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::audio::LevelMeter;
use crate::recording::{
    RecordingBuffer, RecordingError, RecordingInput, RecordingStream, StartedRecording,
};

pub struct FakeStream;

impl RecordingStream for FakeStream {}

pub struct FakeBackend {
    pub buffer: Arc<Mutex<RecordingBuffer>>,
    pub meter: Arc<LevelMeter>,
}

impl FakeBackend {
    pub fn new(buffer: Arc<Mutex<RecordingBuffer>>, meter: Arc<LevelMeter>) -> Self {
        Self { buffer, meter }
    }
}

impl RecordingInput for FakeBackend {
    fn start_recording(&self, _max_duration: Duration) -> Result<StartedRecording, RecordingError> {
        Ok(StartedRecording {
            stream: Box::new(FakeStream),
            buffer: Arc::clone(&self.buffer),
            meter: Arc::clone(&self.meter),
        })
    }
}
