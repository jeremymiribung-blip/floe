pub mod context;
pub mod event;
pub mod logger;
pub mod report;
pub mod storage;
pub mod tracer;

pub use context::PipelineContext;
pub use event::DiagEvent;
pub use logger::init;
#[allow(unused_imports)]
pub use report::{
    contains_secret_marker, rate_limit_to_map, redact_string_for_report, AudioSnapshot,
    DetailedEvent, DiagnosticsReport, HotkeySnapshot, LastError, LastSession, PlatformInfo,
    ProviderState, RateLimitSnapshot, RecoveryAction, ReportInputs, SessionSnapshot,
    SettingsSnapshot, StageRecord, StageStatus, SttProviderSnapshot, REPORT_SCHEMA_VERSION,
};
pub use storage::default_diag_path;
pub use tracer::{PipelineTrace, PipelineTracer};

use std::sync::{Arc, Mutex};

/// Thread-safe store for the most recent dictation session snapshot.
///
/// The dictation commands (`stop_recording`, `transcribe_latest_recording`,
/// `cleanup_transcript`, `copy_text_to_clipboard`, `paste_clipboard`) populate
/// this store as the pipeline progresses so that `get_diagnostics_report`
/// can assemble a complete report at any time — even after a failure.
#[derive(Debug, Clone, Default)]
pub struct LastSessionStore {
    inner: Arc<Mutex<Option<SessionSnapshot>>>,
}

#[allow(dead_code)]
impl LastSessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, snapshot: SessionSnapshot) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some(snapshot);
        }
    }

    pub fn update<F: FnOnce(&mut SessionSnapshot)>(&self, mutate: F) {
        if let Ok(mut guard) = self.inner.lock() {
            let snapshot = guard.get_or_insert_with(SessionSnapshot::default);
            mutate(snapshot);
        }
    }

    pub fn get(&self) -> Option<SessionSnapshot> {
        self.inner.lock().ok().and_then(|g| g.clone())
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
    }

    /// Append a DetailedEvent from the frontend into the current session's
    /// detailed_timeline so the diagnostics report can reconstruct frontend
    /// lifecycle events (pipeline start, stage transitions, retries, etc.).
    pub fn push_frontend_event(&self, trace_id: &str, event: DetailedEvent) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(ref mut snapshot) = guard.as_mut() {
                if snapshot.trace_id.as_deref() == Some(trace_id) {
                    snapshot.detailed_timeline.push(event);
                }
            }
        }
    }

    /// Set the frontend-measured total pipeline duration onto the current session.
    pub fn set_frontend_total_ms(&self, trace_id: &str, total_ms: u64) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(ref mut snapshot) = guard.as_mut() {
                if snapshot.trace_id.as_deref() == Some(trace_id) {
                    snapshot.pipeline_total_ms = Some(total_ms);
                }
            }
        }
    }
}
