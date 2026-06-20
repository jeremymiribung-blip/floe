use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::cleanup::cleanup_transcript_with;
use crate::commands::clipboard::{copy_text_to_clipboard_with, paste_clipboard_with};
use crate::providers::cleanup::CleanupProvider;
use crate::recording::{
    RecordingBuffer, RecordingEndReason, RecordingErrorCode, RecordingManager,
    ShutdownRecordingResult, WAV_HEADER_LEN,
};
use crate::settings::SettingsManager;
use crate::test_helpers::cleanup::FakeCleanupProvider;
use crate::test_helpers::clipboard::{FakeClipboard, FakePasteSimulator};
use crate::test_helpers::recording::FakeBackend;
use crate::test_helpers::settings::MemorySecretStore;

/// A fake transcription function: given WAV bytes, returns text or an error.
/// Replaces the old AsrBackend abstraction that no longer exists.
type TranscribeFn = Box<dyn Fn(Vec<u8>) -> Result<String, String> + Send + Sync>;
/// Uses fakes for hardware (microphone), network (cleanup API),
/// clipboard, paste, and secret storage.
struct PipeHarness {
    recording: RecordingManager,
    transcribe_fn: TranscribeFn,
    settings: SettingsManager,
    cleanup_provider: Box<dyn CleanupProvider>,
    clipboard: FakeClipboard,
    pub(crate) paste: FakePasteSimulator,
    buffer: Arc<Mutex<RecordingBuffer>>,
    meter: Arc<crate::audio::LevelMeter>,
}

impl PipeHarness {
    fn new(
        transcribe_fn: TranscribeFn,
        cleanup_provider: Box<dyn CleanupProvider>,
        api_key: Option<&str>,
    ) -> Self {
        let meter = Arc::new(crate::audio::LevelMeter::new());
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(
            48_000,
            1,
            Duration::from_secs(120),
            1000,
        )));

        let backend = FakeBackend::new(Arc::clone(&buffer), Arc::clone(&meter));
        let recording = RecordingManager::new_with_options(
            Box::new(backend),
            Box::new(no_op_emit),
            Duration::from_secs(120),
            Duration::from_millis(50),
        );

        let secret_store = match api_key {
            Some(key) => MemorySecretStore::with_key(key),
            None => MemorySecretStore::new(),
        };
        let settings = SettingsManager::with_secret_store(
            Box::new(secret_store),
            std::env::temp_dir().join(format!(
                "floe-pipeline-test-{}.json",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            )),
        );

        Self {
            recording,
            transcribe_fn,
            settings,
            cleanup_provider,
            clipboard: FakeClipboard::new(),
            paste: FakePasteSimulator::new(),
            buffer,
            meter,
        }
    }

    /// Injects sample audio data into the recording buffer.
    fn inject_samples(&self, samples: &[f32]) {
        let mut buf = self.buffer.lock().unwrap();
        buf.append_interleaved(samples, &self.meter);
    }

    /// Simulates the full pipeline: record -> STT -> cleanup -> clipboard -> paste.
    async fn run_full_pipeline(&self, _stt_provider_id: &str) -> PipelineResult {
        let recording_started_ok = self.recording.start_recording().is_ok();
        let mut result = PipelineResult {
            recording_started_ok,
            ..PipelineResult::default()
        };

        if result.recording_started_ok {
            // Stage 2: Transcribe
            let wav = self.recording.get_latest_recording_wav_bytes().unwrap();
            let wav_bytes = wav.unwrap_or_default();

            match (self.transcribe_fn)(wav_bytes) {
                Ok(text) => {
                    result.transcript = Some(text.clone());
                    result.transcribe_ok = true;

                    // Stage 3: Cleanup
                    let cleaned =
                        cleanup_transcript_with(&self.settings, text, &*self.cleanup_provider)
                            .await;
                    result.cleanup_text = Some(cleaned.text.clone());
                    result.cleanup_fallback_used = cleaned.fallback_used;
                    result.cleanup_warning = cleaned.warning;

                    // Stage 4: Clipboard
                    let clipboard_result =
                        copy_text_to_clipboard_with(&self.clipboard, &cleaned.text, || {});
                    result.clipboard_ok = clipboard_result.is_ok();
                    result.clipboard_text = self.clipboard.text();

                    // Stage 5: Paste
                    let paste_result = paste_clipboard_with(&self.paste, || {});
                    result.paste_ok = paste_result.is_ok();
                    result.paste_shortcut_count = self.paste.shortcut_count();
                }
                Err(_) => {
                    result.transcribe_ok = false;
                }
            }
        }

        result
    }
}

#[derive(Debug, Default, PartialEq)]
struct PipelineResult {
    recording_started_ok: bool,
    transcript: Option<String>,
    transcribe_ok: bool,
    cleanup_text: Option<String>,
    cleanup_fallback_used: bool,
    cleanup_warning: Option<String>,
    clipboard_ok: bool,
    clipboard_text: Option<String>,
    paste_ok: bool,
    paste_shortcut_count: usize,
}

fn no_op_emit(_level: f32) {}

fn transcribe_ok(text: &str) -> TranscribeFn {
    let t = text.to_string();
    Box::new(move |_wav| Ok(t.clone()))
}

fn transcribe_err() -> TranscribeFn {
    Box::new(|_wav| Err("stt_error".to_string()))
}

fn make_cleanup(response_text: &str) -> Box<FakeCleanupProvider> {
    Box::new(FakeCleanupProvider::ok(response_text))
}

// ── Happy path ────────────────────────────────────────────────────────

#[tokio::test]
async fn full_pipeline_happy_path() {
    let h = PipeHarness::new(
        transcribe_ok("hello world"),
        make_cleanup("hello world (cleaned)"),
        Some("gsk_fake_test_key_12345678"),
    );

    h.recording.start_recording().unwrap();
    h.inject_samples(&[0.5_f32; 480]); // 10ms at 48kHz
    let info = h.recording.stop_recording().unwrap();

    assert!(info.sample_count > 0, "should capture samples");
    assert_eq!(info.ended_reason, RecordingEndReason::Manual);

    let wav = h
        .recording
        .get_latest_recording_wav_bytes()
        .unwrap()
        .expect("wav should exist");
    assert_eq!(&wav[0..4], b"RIFF", "valid WAV header");
    assert!(wav.len() > WAV_HEADER_LEN, "WAV has audio data");

    let result = h.run_full_pipeline("groq").await;

    assert!(result.recording_started_ok);
    assert_eq!(result.transcript.as_deref(), Some("hello world"));
    assert_eq!(
        result.cleanup_text.as_deref(),
        Some("hello world (cleaned)")
    );
    assert!(!result.cleanup_fallback_used);
    assert!(result.clipboard_ok);
    assert_eq!(
        result.clipboard_text.as_deref(),
        Some("hello world (cleaned)")
    );
    assert!(result.paste_ok);
    assert_eq!(result.paste_shortcut_count, 1);
}

// ── Empty recording error path ────────────────────────────────────────

#[tokio::test]
async fn empty_recording_is_rejected() {
    let h = PipeHarness::new(transcribe_ok("unused"), make_cleanup("unused"), None);

    h.recording.start_recording().unwrap();
    let err = h.recording.stop_recording().expect_err("empty should fail");

    assert_eq!(err.code, RecordingErrorCode::EmptyRecording);
}

// ── STT failure path ──────────────────────────────────────────────────

#[tokio::test]
async fn stt_failure_prevents_cleanup_and_clipboard() {
    let h = PipeHarness::new(
        transcribe_err(),
        make_cleanup("unused"),
        Some("gsk_fake_test_key_12345678"),
    );

    h.recording.start_recording().unwrap();
    h.inject_samples(&[0.5_f32; 480]);
    h.recording.stop_recording().unwrap();

    let result = h.run_full_pipeline("groq").await;

    assert!(!result.transcribe_ok);
    assert!(result.transcript.is_none());
    assert!(result.cleanup_text.is_none());
    assert!(!result.clipboard_ok);
    assert!(!result.paste_ok);
}

// ── Cleanup fallback ──────────────────────────────────────────────────

#[tokio::test]
async fn cleanup_fallback_preserves_raw_transcript() {
    let h = PipeHarness::new(
        transcribe_ok("raw transcript text"),
        Box::new(FakeCleanupProvider::failing("serverError")),
        Some("gsk_fake_test_key_12345678"),
    );

    h.recording.start_recording().unwrap();
    h.inject_samples(&[0.5_f32; 480]);
    h.recording.stop_recording().unwrap();

    let result = h.run_full_pipeline("groq").await;

    assert!(result.transcribe_ok);
    assert!(result.cleanup_fallback_used);
    assert_eq!(result.cleanup_text.as_deref(), Some("raw transcript text"));
    assert_eq!(result.cleanup_warning.as_deref(), Some("Cleanup failed"));
    assert!(result.clipboard_ok);
    assert_eq!(
        result.clipboard_text.as_deref(),
        Some("raw transcript text")
    );
    assert!(result.paste_ok);
}

// ── Missing API key does not call cleanup provider ────────────────────

#[tokio::test]
async fn missing_api_key_falls_back_without_calling_cleanup_provider() {
    use std::sync::atomic::{AtomicBool, Ordering};

    static CALLED: AtomicBool = AtomicBool::new(false);

    struct TrackedCleanupProvider;
    #[async_trait::async_trait]
    impl crate::providers::cleanup::CleanupProvider for TrackedCleanupProvider {
        async fn cleanup(
            &self,
            _api_key: &str,
            _transcript: &str,
        ) -> Result<
            crate::providers::cleanup::CleanupSuccess,
            crate::providers::cleanup::CleanupError,
        > {
            CALLED.store(true, Ordering::SeqCst);
            Ok(crate::providers::cleanup::CleanupSuccess {
                text: "should not be reached".into(),
                model: "unused".into(),
                retry_count: 0,
                validation_ms: 0,
                rate_limit: None,
            })
        }
    }

    let h = PipeHarness::new(
        transcribe_ok("raw transcript"),
        Box::new(TrackedCleanupProvider),
        None, // no API key set
    );

    h.recording.start_recording().unwrap();
    h.inject_samples(&[0.5_f32; 480]);
    h.recording.stop_recording().unwrap();

    let result = h.run_full_pipeline("groq").await;

    assert!(result.transcribe_ok);
    assert!(result.cleanup_fallback_used);
    assert_eq!(result.cleanup_text.as_deref(), Some("raw transcript"));
    assert!(
        !CALLED.load(Ordering::SeqCst),
        "provider must not be called without key"
    );
}

// ── Paste failure still leaves text in clipboard ──────────────────────

#[tokio::test]
async fn paste_failure_leaves_text_in_clipboard() {
    let mut h = PipeHarness::new(
        transcribe_ok("hello"),
        make_cleanup("hello cleaned"),
        Some("gsk_key"),
    );

    h.recording.start_recording().unwrap();
    h.inject_samples(&[0.5_f32; 480]);
    let _info = h.recording.stop_recording().unwrap();

    let cleaned =
        cleanup_transcript_with(&h.settings, "hello".to_string(), &*h.cleanup_provider).await;

    copy_text_to_clipboard_with(&h.clipboard, &cleaned.text, || {}).unwrap();
    assert_eq!(h.clipboard.text().as_deref(), Some("hello cleaned"));

    // Now make paste fail
    h.paste.fail_paste = true;
    let _paste_result = paste_clipboard_with(&h.paste, || {});
    // Clipboard still holds the text
    assert_eq!(h.clipboard.text().as_deref(), Some("hello cleaned"));
}

// ── Concurrent start is rejected ──────────────────────────────────────

#[tokio::test]
async fn concurrent_start_rejected() {
    let h = PipeHarness::new(transcribe_ok("unused"), make_cleanup("unused"), None);

    h.recording.start_recording().unwrap();
    let err = h
        .recording
        .start_recording()
        .expect_err("second start should fail");

    assert_eq!(err.code, RecordingErrorCode::AlreadyRecording);
}

// ── Stop without start is rejected ────────────────────────────────────

#[tokio::test]
async fn stop_without_start_rejected() {
    let h = PipeHarness::new(transcribe_ok("unused"), make_cleanup("unused"), None);

    let err = h
        .recording
        .stop_recording()
        .expect_err("stop without start should fail");

    assert_eq!(err.code, RecordingErrorCode::NotRecording);
}

// ── Multiple record cycles work ──────────────────────────────────────

#[tokio::test]
async fn repeated_record_cycles_work() {
    let h = PipeHarness::new(
        transcribe_ok("cycle"),
        make_cleanup("cycle cleaned"),
        Some("gsk_key"),
    );

    for i in 0..3 {
        h.recording.start_recording().unwrap();
        h.inject_samples(&[0.5_f32; 160]);
        let info = h
            .recording
            .stop_recording()
            .unwrap_or_else(|_| panic!("cycle {} stop should succeed", i));

        assert_eq!(info.ended_reason, RecordingEndReason::Manual);

        let wav = h
            .recording
            .get_latest_recording_wav_bytes()
            .unwrap()
            .expect("wav should exist after each cycle");
        assert_eq!(&wav[0..4], b"RIFF");
    }
}

// ── Device disconnected mid-recording ────────────────────────────────

#[tokio::test]
async fn device_disconnect_finalizes_recording() {
    let h = PipeHarness::new(transcribe_ok("unused"), make_cleanup("unused"), None);

    h.recording.start_recording().unwrap();
    h.inject_samples(&[0.5_f32; 480]);
    {
        let mut buf = h.buffer.lock().unwrap();
        buf.mark_device_disconnected();
    }

    h.recording.poll_finalize().unwrap();
    let status = h.recording.get_recording_status().unwrap();
    assert!(!status.is_recording);
    let latest = status.latest_recording.expect("should have a recording");
    assert_eq!(latest.ended_reason, RecordingEndReason::DeviceDisconnected);
    assert_eq!(
        status.last_error.unwrap().code,
        RecordingErrorCode::DeviceDisconnected
    );
}

// ── WAV encoding produces correct format ──────────────────────────────

#[tokio::test]
async fn wav_output_is_16khz_mono_16bit() {
    let h = PipeHarness::new(
        transcribe_ok("test"),
        make_cleanup("test cleaned"),
        Some("gsk_key"),
    );

    h.recording.start_recording().unwrap();
    h.inject_samples(&[0.25_f32; 960]);
    let info = h.recording.stop_recording().unwrap();

    // Recording stores native sample rate (48 kHz)
    assert_eq!(info.sample_rate, 48_000);
    // WAV is resampled to 16 kHz mono 16-bit
    assert_eq!(info.wav_sample_rate, 16_000);
    assert_eq!(info.wav_channels, 1);
    assert_eq!(info.wav_bits_per_sample, 16);
    assert_eq!(info.wav_format, "wav");

    let wav = h
        .recording
        .get_latest_recording_wav_bytes()
        .unwrap()
        .expect("wav should exist");
    assert_eq!(&wav[0..4], b"RIFF");
    assert!(wav.len() > WAV_HEADER_LEN);
}

// ── Shutdown during recording ─────────────────────────────────────────

#[tokio::test]
async fn shutdown_during_recording_finalizes_data() {
    let h = PipeHarness::new(transcribe_ok("unused"), make_cleanup("unused"), None);

    h.recording.start_recording().unwrap();
    h.inject_samples(&[0.5_f32; 480]);

    match h.recording.stop_for_shutdown().unwrap() {
        ShutdownRecordingResult::Finalized => {}
        other => panic!("expected Finalized, got {:?}", other),
    }

    let status = h.recording.get_recording_status().unwrap();
    assert!(!status.is_recording);
    let latest = status.latest_recording.expect("shutdown recording exists");
    assert_eq!(latest.ended_reason, RecordingEndReason::Shutdown);
}

// ── Shutdown while idle is idempotent ────────────────────────────────

#[tokio::test]
async fn shutdown_when_idle_is_idempotent() {
    let h = PipeHarness::new(transcribe_ok("unused"), make_cleanup("unused"), None);

    assert_eq!(
        h.recording.stop_for_shutdown().unwrap(),
        ShutdownRecordingResult::Idle
    );
    assert_eq!(
        h.recording.stop_for_shutdown().unwrap(),
        ShutdownRecordingResult::Idle
    );
}

// ── Concurrent status queries do not panic ────────────────────────────

#[tokio::test]
async fn concurrent_status_queries_during_recording_do_not_panic() {
    let h = std::sync::Arc::new(PipeHarness::new(
        transcribe_ok("unused"),
        make_cleanup("unused"),
        None,
    ));

    h.recording.start_recording().unwrap();
    h.inject_samples(&[0.5_f32; 480]);

    let mut handles = Vec::new();
    for _ in 0..8 {
        let h_clone = std::sync::Arc::clone(&h);
        handles.push(std::thread::spawn(move || {
            for _ in 0..5 {
                let _ = h_clone.recording.get_recording_status();
                let _ = h_clone.recording.get_latest_recording_info();
                std::thread::sleep(Duration::from_millis(1));
            }
        }));
    }
    for handle in handles {
        handle.join().expect("reader thread panicked");
    }
    let _ = h.recording.stop_recording();
    let status = h.recording.get_recording_status().unwrap();
    assert!(!status.is_recording);
}

// ── Async concurrency stress test ─────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn async_concurrent_start_stop_stress_test() {
    let harness = std::sync::Arc::new(PipeHarness::new(
        transcribe_ok("stress"),
        make_cleanup("stress cleaned"),
        Some("gsk_stress_test_key_98765"),
    ));

    let start = std::time::Instant::now();

    let mut handles = Vec::new();
    for _ in 0..10 {
        let h = std::sync::Arc::clone(&harness);
        handles.push(tokio::spawn(async move {
            for _ in 0..5 {
                // Inject samples so stop_recording has data to encode
                h.inject_samples(&[0.5_f32; 48]);

                // Attempt start (may fail with AlreadyRecording in a race)
                let _ = h.recording.start_recording();

                // Inject more samples for the recording if start succeeded
                h.inject_samples(&[0.5_f32; 48]);

                // Attempt stop (may fail with NotRecording in a race)
                let _ = h.recording.stop_recording();
            }
        }));
    }

    for handle in handles {
        handle.await.expect("tokio task panicked");
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "test took {:?}, expected < 10s",
        elapsed,
    );

    // Check final state is Idle
    let status = harness
        .recording
        .get_recording_status()
        .expect("status should be readable");
    assert!(
        !status.is_recording,
        "recording should be idle after all tasks complete"
    );

    // last_error should not be Internal or StopFailed;
    // AlreadyRecording / NotRecording are expected race outcomes, as is None.
    if let Some(err) = &status.last_error {
        assert!(
            err.code != RecordingErrorCode::Internal && err.code != RecordingErrorCode::StopFailed,
            "unexpected error after stress test: {:?}",
            err,
        );
    }
}
