use std::fmt;

/// Structured diagnostic events emitted at key pipeline stages.
/// Each variant enforces compile-time privacy — no transcripts, audio, keys, or clipboard content.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum DiagEvent {
    RecordingStarted {
        trace_id: String,
        duration_ms: u64,
    },
    RecordingStopped {
        trace_id: String,
        duration_ms: u64,
        encode_ms: u64,
        wav_bytes: u64,
        sample_rate: u32,
        ended_reason: String,
    },
    RecordingError {
        trace_id: String,
        error_code: String,
    },
    SttAttempt {
        trace_id: String,
        attempt: u32,
        audio_duration_ms: u64,
    },
    SttRetry {
        trace_id: String,
        attempt: u32,
        delay_ms: u64,
        error_code: String,
    },
    SttCompleted {
        trace_id: String,
        duration_ms: u64,
        model: String,
        retry_count: u32,
        audio_duration_ms: u64,
        realtime_factor: f64,
    },
    SttFailed {
        trace_id: String,
        duration_ms: u64,
        attempt: u32,
        retry_count: u32,
        error_code: String,
    },
    CleanupAttempt {
        trace_id: String,
        attempt: u32,
        transcript_len: u32,
    },
    CleanupRetry {
        trace_id: String,
        attempt: u32,
        delay_ms: u64,
        error_code: String,
    },
    CleanupCompleted {
        trace_id: String,
        duration_ms: u64,
        model: String,
        retry_count: u32,
        validation_ms: u64,
    },
    CleanupFailed {
        trace_id: String,
        duration_ms: u64,
        attempt: u32,
        retry_count: u32,
        error_code: String,
    },
    CleanupFallback {
        trace_id: String,
        error_code: String,
    },
    ClipboardWritten {
        trace_id: String,
        duration_ms: u64,
    },
    ClipboardFailed {
        trace_id: String,
        duration_ms: u64,
        error_code: String,
    },
    PasteDone {
        trace_id: String,
        duration_ms: u64,
    },
    PasteFailed {
        trace_id: String,
        duration_ms: u64,
        error_code: String,
    },
    PipelineCompleted {
        trace_id: String,
        total_ms: u64,
        stages: String,
    },
    PipelineFailed {
        trace_id: String,
        total_ms: u64,
        error_stage: String,
        error_code: String,
    },
}

impl fmt::Display for DiagEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecordingStarted {
                trace_id,
                duration_ms,
            } => {
                write!(
                    f,
                    "event=recording_started trace_id={trace_id} setup_ms={duration_ms}"
                )
            }
            Self::RecordingStopped {
                trace_id,
                duration_ms,
                encode_ms,
                wav_bytes,
                sample_rate,
                ended_reason,
            } => {
                write!(f, "event=recording_stopped trace_id={trace_id} duration_ms={duration_ms} encode_ms={encode_ms} wav_bytes={wav_bytes} sample_rate={sample_rate} ended_reason={ended_reason}")
            }
            Self::RecordingError {
                trace_id,
                error_code,
            } => {
                write!(
                    f,
                    "event=recording_error trace_id={trace_id} error_code={error_code}"
                )
            }
            Self::SttAttempt {
                trace_id,
                attempt,
                audio_duration_ms,
            } => {
                write!(f, "event=stt_attempt trace_id={trace_id} attempt={attempt} audio_duration_ms={audio_duration_ms}")
            }
            Self::SttRetry {
                trace_id,
                attempt,
                delay_ms,
                error_code,
            } => {
                write!(f, "event=stt_retry trace_id={trace_id} attempt={attempt} delay_ms={delay_ms} error_code={error_code}")
            }
            Self::SttCompleted {
                trace_id,
                duration_ms,
                model,
                retry_count,
                audio_duration_ms,
                realtime_factor,
            } => {
                write!(f, "event=stt_completed trace_id={trace_id} duration_ms={duration_ms} model={model} retry_count={retry_count} audio_duration_ms={audio_duration_ms} realtime_factor={realtime_factor:.3}")
            }
            Self::SttFailed {
                trace_id,
                duration_ms,
                attempt,
                retry_count,
                error_code,
            } => {
                write!(f, "event=stt_failed trace_id={trace_id} duration_ms={duration_ms} attempt={attempt} retry_count={retry_count} error_code={error_code}")
            }
            Self::CleanupAttempt {
                trace_id,
                attempt,
                transcript_len,
            } => {
                write!(f, "event=cleanup_attempt trace_id={trace_id} attempt={attempt} transcript_len={transcript_len}")
            }
            Self::CleanupRetry {
                trace_id,
                attempt,
                delay_ms,
                error_code,
            } => {
                write!(f, "event=cleanup_retry trace_id={trace_id} attempt={attempt} delay_ms={delay_ms} error_code={error_code}")
            }
            Self::CleanupCompleted {
                trace_id,
                duration_ms,
                model,
                retry_count,
                validation_ms,
            } => {
                write!(f, "event=cleanup_completed trace_id={trace_id} duration_ms={duration_ms} model={model} retry_count={retry_count} validation_ms={validation_ms}")
            }
            Self::CleanupFailed {
                trace_id,
                duration_ms,
                attempt,
                retry_count,
                error_code,
            } => {
                write!(f, "event=cleanup_failed trace_id={trace_id} duration_ms={duration_ms} attempt={attempt} retry_count={retry_count} error_code={error_code}")
            }
            Self::CleanupFallback {
                trace_id,
                error_code,
            } => {
                write!(
                    f,
                    "event=cleanup_fallback trace_id={trace_id} error_code={error_code}"
                )
            }
            Self::ClipboardWritten {
                trace_id,
                duration_ms,
            } => {
                write!(
                    f,
                    "event=clipboard_written trace_id={trace_id} duration_ms={duration_ms}"
                )
            }
            Self::ClipboardFailed {
                trace_id,
                duration_ms,
                error_code,
            } => {
                write!(f, "event=clipboard_failed trace_id={trace_id} duration_ms={duration_ms} error_code={error_code}")
            }
            Self::PasteDone {
                trace_id,
                duration_ms,
            } => {
                write!(
                    f,
                    "event=paste_done trace_id={trace_id} duration_ms={duration_ms}"
                )
            }
            Self::PasteFailed {
                trace_id,
                duration_ms,
                error_code,
            } => {
                write!(f, "event=paste_failed trace_id={trace_id} duration_ms={duration_ms} error_code={error_code}")
            }
            Self::PipelineCompleted {
                trace_id,
                total_ms,
                stages,
            } => {
                write!(f, "event=pipeline_completed trace_id={trace_id} total_ms={total_ms} stages={stages}")
            }
            Self::PipelineFailed {
                trace_id,
                total_ms,
                error_stage,
                error_code,
            } => {
                write!(f, "event=pipeline_failed trace_id={trace_id} total_ms={total_ms} error_stage={error_stage} error_code={error_code}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_started_format() {
        let e = DiagEvent::RecordingStarted {
            trace_id: "abc123".into(),
            duration_ms: 42,
        };
        let s = e.to_string();
        assert!(s.contains("event=recording_started"));
        assert!(s.contains("trace_id=abc123"));
        assert!(s.contains("setup_ms=42"));
    }

    #[test]
    fn stt_completed_format() {
        let e = DiagEvent::SttCompleted {
            trace_id: "xyz789".into(),
            duration_ms: 1500,
            model: "whisper".into(),
            retry_count: 1,
            audio_duration_ms: 5000,
            realtime_factor: 0.3,
        };
        let s = e.to_string();
        assert!(s.contains("event=stt_completed"));
        assert!(s.contains("trace_id=xyz789"));
        assert!(s.contains("duration_ms=1500"));
        assert!(s.contains("model=whisper"));
        assert!(s.contains("retry_count=1"));
        assert!(s.contains("realtime_factor=0.300"));
    }

    #[test]
    fn no_private_data_in_any_variant() {
        let variants: Vec<DiagEvent> = vec![
            DiagEvent::RecordingStarted {
                trace_id: "t".into(),
                duration_ms: 0,
            },
            DiagEvent::RecordingStopped {
                trace_id: "t".into(),
                duration_ms: 0,
                encode_ms: 0,
                wav_bytes: 0,
                sample_rate: 16000,
                ended_reason: "manual".into(),
            },
            DiagEvent::RecordingError {
                trace_id: "t".into(),
                error_code: "err".into(),
            },
            DiagEvent::SttAttempt {
                trace_id: "t".into(),
                attempt: 1,
                audio_duration_ms: 1000,
            },
            DiagEvent::SttRetry {
                trace_id: "t".into(),
                attempt: 2,
                delay_ms: 250,
                error_code: "timeout".into(),
            },
            DiagEvent::SttCompleted {
                trace_id: "t".into(),
                duration_ms: 500,
                model: "m".into(),
                retry_count: 0,
                audio_duration_ms: 1000,
                realtime_factor: 0.5,
            },
            DiagEvent::SttFailed {
                trace_id: "t".into(),
                duration_ms: 30000,
                attempt: 3,
                retry_count: 2,
                error_code: "timeout".into(),
            },
            DiagEvent::CleanupAttempt {
                trace_id: "t".into(),
                attempt: 1,
                transcript_len: 200,
            },
            DiagEvent::CleanupRetry {
                trace_id: "t".into(),
                attempt: 2,
                delay_ms: 500,
                error_code: "rate_limit".into(),
            },
            DiagEvent::CleanupCompleted {
                trace_id: "t".into(),
                duration_ms: 800,
                model: "llm".into(),
                retry_count: 0,
                validation_ms: 10,
            },
            DiagEvent::CleanupFailed {
                trace_id: "t".into(),
                duration_ms: 15000,
                attempt: 3,
                retry_count: 2,
                error_code: "server_error".into(),
            },
            DiagEvent::CleanupFallback {
                trace_id: "t".into(),
                error_code: "server_error".into(),
            },
            DiagEvent::ClipboardWritten {
                trace_id: "t".into(),
                duration_ms: 5,
            },
            DiagEvent::ClipboardFailed {
                trace_id: "t".into(),
                duration_ms: 50,
                error_code: "clipboard_unavailable".into(),
            },
            DiagEvent::PasteDone {
                trace_id: "t".into(),
                duration_ms: 100,
            },
            DiagEvent::PasteFailed {
                trace_id: "t".into(),
                duration_ms: 200,
                error_code: "paste_unavailable".into(),
            },
            DiagEvent::PipelineCompleted {
                trace_id: "t".into(),
                total_ms: 5000,
                stages: "recording,stt,cleanup,clipboard,paste".into(),
            },
            DiagEvent::PipelineFailed {
                trace_id: "t".into(),
                total_ms: 3000,
                error_stage: "stt".into(),
                error_code: "timeout".into(),
            },
        ];
        for v in &variants {
            let s = v.to_string();
            // Check that no variant includes a field that could hold the full transcript text.
            // The key name "transcript_len" is metadata (character count), not transcript content.
            assert!(!s.contains(" text="), "variant leaks text: {s}");
            assert!(!s.contains("audio_bytes"), "variant leaks audio: {s}");
            assert!(!s.contains("api_key"), "variant leaks key: {s}");
            assert!(
                !s.contains("clipboard_text"),
                "variant leaks clipboard: {s}"
            );
            assert!(!s.contains("bearer"), "variant leaks bearer: {s}");
            assert!(!s.contains("authorization"), "variant leaks auth: {s}");
        }
    }
}
