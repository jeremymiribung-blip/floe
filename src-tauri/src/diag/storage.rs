use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::report::SessionSnapshot;

const DIAG_FILENAME: &str = "floe-diag.log";

/// Name of the file that persists the most recent session snapshot.
pub const SESSION_FILENAME: &str = "session.json";

/// Schema version for the persisted session JSON.
/// Bump on any breaking change to `PersistedSession`.
pub const SESSION_SCHEMA_VERSION: u32 = 1;

pub struct LogRotation {
    pub max_bytes: u64,
    pub max_files: u32,
}

impl Default for LogRotation {
    fn default() -> Self {
        Self {
            max_bytes: 2_000_000,
            max_files: 3,
        }
    }
}

pub fn default_diag_path(config_dir: &Path) -> PathBuf {
    config_dir.join(DIAG_FILENAME)
}

/// Return the default path for the persisted session snapshot file.
pub fn default_session_path(config_dir: &Path) -> PathBuf {
    config_dir.join(SESSION_FILENAME)
}

pub fn rotate_if_needed(log_path: &Path, rotation: &LogRotation) -> io::Result<()> {
    let metadata = match fs::metadata(log_path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    if metadata.len() < rotation.max_bytes {
        return Ok(());
    }

    rotate_files(log_path, rotation.max_files)
}

fn rotate_files(log_path: &Path, max_files: u32) -> io::Result<()> {
    for i in (1..max_files).rev() {
        let src = numbered_path(log_path, i);
        let dst = numbered_path(log_path, i + 1);
        if src.exists() {
            if dst.exists() {
                fs::remove_file(&dst)?;
            }
            fs::rename(&src, &dst)?;
        }
    }

    if log_path.exists() {
        let first = numbered_path(log_path, 1);
        if first.exists() {
            fs::remove_file(&first)?;
        }
        fs::rename(log_path, &first)?;
    }

    Ok(())
}

/// Wrapper around the on-disk session snapshot.
/// Includes metadata used for crash detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PersistedSession {
    pub schema_version: u32,
    pub snapshot: SessionSnapshot,
    pub clean_shutdown: bool,
    pub written_at: String,
}

impl PersistedSession {
    /// Create a new persisted session wrapping the given snapshot.
    pub fn new(snapshot: SessionSnapshot) -> Self {
        Self {
            schema_version: SESSION_SCHEMA_VERSION,
            snapshot,
            clean_shutdown: false,
            written_at: iso_now(),
        }
    }

    /// Mark this session as a clean shutdown and return an updated copy.
    pub fn with_clean_shutdown(mut self) -> Self {
        self.clean_shutdown = true;
        self.written_at = iso_now();
        self
    }
}

/// Read the persisted session file at `path` and return the `PersistedSession`
/// if the file exists and is valid. Returns `None` if the file does not exist
/// or cannot be parsed.
pub fn read_persisted_session(path: &Path) -> Option<PersistedSession> {
    if !path.exists() {
        return None;
    }
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str::<PersistedSession>(&data).ok()
}

/// Write (overwrite) the persisted session file at `path`.
/// Creates parent directories if needed.
pub fn write_persisted_session(path: &Path, persisted: &PersistedSession) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(persisted)?;
    fs::write(path, data)?;
    Ok(())
}

/// Delete the persisted session file at `path`.
/// Succeeds silently if the file does not exist.
pub fn delete_persisted_session(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Attempt to finalize the previous session after a crash.
///
/// If a session file exists with `clean_shutdown: false`, the app likely
/// crashed during or after the last session. This function re-writes that
/// session with a `clean_shutdown: true` flag so it is available for the
/// diagnostics UI, and returns a reference to the finalized snapshot.
///
/// Returns `None` when no session file exists or the session was already
/// cleanly shut down.
pub fn finalize_crashed_session(path: &Path) -> io::Result<Option<CrashedSessionInfo>> {
    let persisted = match read_persisted_session(path) {
        Some(p) => p,
        None => return Ok(None),
    };

    if persisted.clean_shutdown {
        // Previous session ended cleanly — nothing to do.
        return Ok(None);
    }

    let info = CrashedSessionInfo {
        trace_id: persisted.snapshot.trace_id.clone(),
    };

    // Finalize: write with clean_shutdown = true so the next startup doesn't
    // re-detect a crash. The snapshot content is preserved for the UI.
    let finalized = persisted.with_clean_shutdown();
    write_persisted_session(path, &finalized)?;

    log::info!(
        "crashed_session_finalized trace_id={:?}",
        info.trace_id
    );

    Ok(Some(info))
}

/// Information about a detected-and-finalized crashed session.
#[derive(Debug, Clone)]
pub struct CrashedSessionInfo {
    pub trace_id: Option<String>,
}

fn iso_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let millis = d.subsec_millis();
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;
    let days = secs / 86400;

    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let month_days = if is_leap(y) {
        &LEAP_MONTH_DAYS[..]
    } else {
        &NORMAL_MONTH_DAYS[..]
    };
    let mut m = 1;
    for &md in month_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        m += 1;
    }
    let d = remaining + 1;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y, m, d, hours, minutes, seconds, millis
    )
}

const NORMAL_MONTH_DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
const LEAP_MONTH_DAYS: [i64; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn numbered_path(base: &Path, n: u32) -> PathBuf {
    let mut path = base.to_path_buf();
    let ext = format!("{}.log", n);
    path.set_extension(&ext);
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rotation_smaller_than_max_does_nothing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.log");
        fs::write(&path, "small content").unwrap();
        rotate_if_needed(
            &path,
            &LogRotation {
                max_bytes: 10_000,
                max_files: 3,
            },
        )
        .unwrap();
        assert!(path.exists());
        assert!(!numbered_path(&path, 1).exists());
    }

    #[test]
    fn rotation_creates_rotated_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rotate.log");
        let content = "x".repeat(100);
        fs::write(&path, &content).unwrap();
        rotate_if_needed(
            &path,
            &LogRotation {
                max_bytes: 50,
                max_files: 3,
            },
        )
        .unwrap();
        assert!(!path.exists());
        assert!(numbered_path(&path, 1).exists());
        let rotated = fs::read_to_string(numbered_path(&path, 1)).unwrap();
        assert_eq!(rotated, content);
    }

    #[test]
    fn rotation_keeps_only_max_files() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("evict.log");
        let content = "x".repeat(100);

        // Write content repeatedly to trigger rotations
        for _ in 0..6 {
            fs::write(&path, &content).unwrap();
            rotate_if_needed(
                &path,
                &LogRotation {
                    max_bytes: 50,
                    max_files: 3,
                },
            )
            .unwrap();
        }

        // Only max_files rotated files should exist (1.log, 2.log, 3.log)
        assert!(numbered_path(&path, 1).exists());
        assert!(numbered_path(&path, 2).exists());
        assert!(numbered_path(&path, 3).exists());
        // No file beyond max_files
        assert!(!numbered_path(&path, 4).exists());
    }

    #[test]
    fn missing_file_is_ok() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.log");
        rotate_if_needed(&path, &LogRotation::default()).unwrap();
    }

    #[test]
    fn default_diag_path_uses_config_dir() {
        let dir = tempdir().unwrap();
        let path = default_diag_path(dir.path());
        assert_eq!(path.file_name().unwrap(), "floe-diag.log");
    }

    // ── Session persistence tests ──

    #[test]
    fn default_session_path_uses_config_dir() {
        let dir = tempdir().unwrap();
        let path = default_session_path(dir.path());
        assert_eq!(path.file_name().unwrap(), "session.json");
    }

    #[test]
    fn persisted_session_round_trip() {
        let dir = tempdir().unwrap();
        let path = default_session_path(dir.path());

        let snapshot = super::SessionSnapshot {
            trace_id: Some("abc123".into()),
            completed: true,
            transcription_ms: 500,
            ..Default::default()
        };

        let persisted = PersistedSession::new(snapshot.clone());
        assert!(!persisted.clean_shutdown);
        assert_eq!(persisted.schema_version, SESSION_SCHEMA_VERSION);
        assert_eq!(persisted.snapshot.trace_id.as_deref(), Some("abc123"));

        write_persisted_session(&path, &persisted).unwrap();
        assert!(path.exists());

        let loaded = read_persisted_session(&path).unwrap();
        assert_eq!(loaded.snapshot.trace_id, snapshot.trace_id);
        assert_eq!(loaded.snapshot.transcription_ms, 500);
        assert!(loaded.snapshot.completed);
        assert!(!loaded.clean_shutdown);
        assert_eq!(loaded.schema_version, SESSION_SCHEMA_VERSION);
    }

    #[test]
    fn missing_session_file_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        assert!(read_persisted_session(&path).is_none());
    }

    #[test]
    fn delete_persisted_session_works() {
        let dir = tempdir().unwrap();
        let path = default_session_path(dir.path());

        let persisted = PersistedSession::new(SessionSnapshot::default());
        write_persisted_session(&path, &persisted).unwrap();
        assert!(path.exists());

        delete_persisted_session(&path).unwrap();
        assert!(!path.exists());

        // Deleting non-existent file should succeed silently.
        delete_persisted_session(&path).unwrap();
    }

    #[test]
    fn finalize_crashed_session_detects_and_fixes_crash() {
        let dir = tempdir().unwrap();
        let path = default_session_path(dir.path());

        // Write a session with clean_shutdown = false (simulates crash)
        let snapshot = SessionSnapshot {
            trace_id: Some("crash001".into()),
            transcription_ms: 100,
            ..Default::default()
        };
        let persisted = PersistedSession::new(snapshot);
        write_persisted_session(&path, &persisted).unwrap();

        // Finalize should detect the crash
        let info = finalize_crashed_session(&path).unwrap();
        assert!(info.is_some());
        assert_eq!(info.unwrap().trace_id.as_deref(), Some("crash001"));

        // After finalization, the file should have clean_shutdown = true
        let reloaded = read_persisted_session(&path).unwrap();
        assert!(reloaded.clean_shutdown);
        assert_eq!(reloaded.snapshot.trace_id.as_deref(), Some("crash001"));
        assert_eq!(reloaded.snapshot.transcription_ms, 100);

        // Second call should return None (already clean)
        let second_check = finalize_crashed_session(&path).unwrap();
        assert!(second_check.is_none());
    }

    #[test]
    fn finalize_crashed_session_no_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("no_such_file.json");
        let result = finalize_crashed_session(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn with_clean_shadow_marks_clean() {
        let snapshot = SessionSnapshot::default();
        let persisted = PersistedSession::new(snapshot).with_clean_shutdown();
        assert!(persisted.clean_shutdown);
    }
}
