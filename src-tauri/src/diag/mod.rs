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
pub use storage::{default_diag_path, default_session_path, finalize_crashed_session};
pub use tracer::{PipelineTrace, PipelineTracer};

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Thread-safe store for the most recent dictation session snapshot,
/// optionally backed by a JSON file on disk for crash survival.
///
/// The dictation commands (`stop_recording`, `transcribe_latest_recording`,
/// `cleanup_transcript`, `copy_text_to_clipboard`, `paste_clipboard`) populate
/// this store as the pipeline progresses so that `get_diagnostics_report`
/// can assemble a complete report at any time — even after a failure.
///
/// When a `persist_path` is configured, every mutation is automatically
/// persisted to disk. On the next app launch the snapshot is reloaded,
/// and any previously incomplete (crashed) session is detected.
#[derive(Debug, Clone)]
pub struct LastSessionStore {
    inner: Arc<Mutex<Option<SessionSnapshot>>>,
    persist_path: Arc<Mutex<Option<PathBuf>>>,
}

impl Default for LastSessionStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            persist_path: Arc::new(Mutex::new(None)),
        }
    }
}

#[allow(dead_code)]
impl LastSessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the path used for automatic on-disk persistence.
    /// If a session.json already exists at this path, it is loaded into memory.
    pub fn set_persist_path(&self, path: PathBuf) {
        if let Ok(mut guard) = self.persist_path.lock() {
            *guard = Some(path.clone());
        }
        // Immediately try to load any existing persisted session.
        self.load_from_disk();
    }

    /// Get the current persist path, if configured.
    pub fn persist_path(&self) -> Option<PathBuf> {
        self.persist_path.lock().ok().and_then(|g| g.clone())
    }

    /// Persist the current in-memory snapshot to disk (if a path is set).
    fn persist(&self) {
        let path = match self.persist_path() {
            Some(p) => p,
            None => return,
        };

        let snapshot = match self.inner.lock().ok().and_then(|g| g.clone()) {
            Some(s) => s,
            None => {
                // No snapshot — delete any stale file on disk.
                let _ = storage::delete_persisted_session(&path);
                return;
            }
        };

        let mut persisted = storage::PersistedSession::new(snapshot);

        // Preserve existing clean_shutdown state so crash detection works
        // across multiple in-process mutations. The clean_shutdown flag is only
        // flipped from false → true in `mark_clean_shutdown()`.
        if let Some(existing) = storage::read_persisted_session(&path) {
            if existing.clean_shutdown {
                persisted.clean_shutdown = true;
            }
        }

        if let Err(e) = storage::write_persisted_session(&path, &persisted) {
            log::warn!("session_persist_failed error=\"{e}\"");
        }
    }

    /// Load a persisted session from disk into memory.
    /// Does nothing if no file exists or if the file cannot be parsed.
    /// Returns `true` if a session was loaded, `false` otherwise.
    fn load_from_disk(&self) -> bool {
        let path = match self.persist_path() {
            Some(p) => p,
            None => return false,
        };

        let persisted = match storage::read_persisted_session(&path) {
            Some(p) => p,
            None => return false,
        };

        if let Ok(mut guard) = self.inner.lock() {
            if guard.is_none() {
                log::info!(
                    "session_loaded_from_disk clean_shutdown={}",
                    persisted.clean_shutdown
                );
                *guard = Some(persisted.snapshot);
                return true;
            }
        }
        false
    }

    /// Mark the persisted session as a clean shutdown.
    /// This is called during graceful app exit to indicate no crash occurred.
    pub fn mark_clean_shutdown(&self) {
        let path = match self.persist_path() {
            Some(p) => p,
            None => return,
        };

        // Use the current in-memory snapshot, or fall back to what's on disk.
        let snapshot = match self.get() {
            Some(s) => s,
            None => {
                match storage::read_persisted_session(&path) {
                    Some(p) => p.snapshot,
                    None => return,
                }
            }
        };

        let finalized = storage::PersistedSession::new(snapshot).with_clean_shutdown();
        if let Err(e) = storage::write_persisted_session(&path, &finalized) {
            log::warn!("session_clean_shutdown_mark_failed error=\"{e}\"");
        }
    }

    /// Check whether a previously-persisted session exists on disk.
    pub fn has_persisted_session(&self) -> bool {
        self.persist_path()
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    pub fn set(&self, snapshot: SessionSnapshot) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some(snapshot);
        }
        self.persist();
    }

    pub fn update<F: FnOnce(&mut SessionSnapshot)>(&self, mutate: F) {
        if let Ok(mut guard) = self.inner.lock() {
            let snapshot = guard.get_or_insert_with(SessionSnapshot::default);
            mutate(snapshot);
        }
        self.persist();
    }

    pub fn get(&self) -> Option<SessionSnapshot> {
        self.inner.lock().ok().and_then(|g| g.clone())
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
        // Delete the on-disk file as well.
        if let Some(path) = self.persist_path() {
            let _ = storage::delete_persisted_session(&path);
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
                // Drop guard before persist to avoid deadlock
            }
        }
        self.persist();
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
        self.persist();
    }
}
