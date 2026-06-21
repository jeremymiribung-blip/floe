use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};

use super::storage::{self, LogRotation};

/// Floe's structured logger.
/// Writes ISO8601-timestamped key=value lines to both stderr and a rotating file.
pub struct FloeLogger {
    stderr: bool,
    file: Mutex<Option<File>>,
    path: PathBuf,
    rotation: LogRotation,
    max_level: LevelFilter,
}

impl FloeLogger {
    pub fn new(stderr: bool, path: PathBuf, max_level: LevelFilter, rotation: LogRotation) -> Self {
        Self {
            stderr,
            file: Mutex::new(None),
            path,
            rotation,
            max_level,
        }
    }

    fn ensure_file_open(&self) -> io::Result<()> {
        let mut file_guard = self
            .file
            .lock()
            .map_err(|_| io::Error::other("logger mutex poisoned"))?;

        if file_guard.is_some() {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        *file_guard = Some(file);
        Ok(())
    }

    fn rotate_if_needed(&self) {
        if let Err(e) = storage::rotate_if_needed(&self.path, &self.rotation) {
            eprintln!("[floe] log rotation failed: {e}");
        }
    }

    fn write_line(&self, line: &str) {
        // Rotate before writing
        self.rotate_if_needed();

        if self.stderr {
            eprintln!("{line}");
        }

        if let Err(e) = self.ensure_file_open() {
            eprintln!("[floe] failed to open log file: {e}");
            return;
        }

        if let Ok(mut file_guard) = self.file.lock() {
            if let Some(file) = file_guard.as_mut() {
                let _ = writeln!(file, "{line}");
                let _ = file.flush();
            }
        }
    }

    pub fn flush(&self) {
        if let Ok(mut file_guard) = self.file.lock() {
            if let Some(file) = file_guard.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

impl Log for FloeLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.max_level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let line = format_log_line(record);
        self.write_line(&line);
    }

    fn flush(&self) {
        self.flush();
    }
}

/// Initialize the global logger.
/// Panics if called more than once.
pub fn init(
    max_level: LevelFilter,
    path: &Path,
    max_bytes: u64,
    max_files: u32,
) -> Result<(), SetLoggerError> {
    // On Windows the binary is always a GUI-subsystem app (via
    // the windows_subsystem attribute in main.rs) so there is
    // no console to write to — disable stderr output to prevent
    // any possibility of conhost.exe interaction.
    let stderr = cfg!(not(target_os = "windows"));
    let logger = FloeLogger::new(
        stderr,
        path.to_path_buf(),
        max_level,
        LogRotation {
            max_bytes,
            max_files,
        },
    );
    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(max_level);
    Ok(())
}

fn format_log_line(record: &Record) -> String {
    let ts = timestamp();
    let level = match record.level() {
        Level::Error => "ERROR",
        Level::Warn => "WARN",
        Level::Info => "INFO",
        Level::Debug => "DEBUG",
        Level::Trace => "TRACE",
    };
    let module = record
        .target()
        .rsplit(':')
        .next()
        .unwrap_or(record.target());

    let args = record.args();
    format!("{ts} {level} [{module}] {args}")
}

fn timestamp() -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use log::Record;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn logger_writes_structured_via_write_line() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.log");
        let logger = FloeLogger::new(
            false,
            path.to_path_buf(),
            LevelFilter::Info,
            LogRotation {
                max_bytes: 10_000,
                max_files: 2,
            },
        );

        logger.write_line(
            "2026-01-01T00:00:00.000Z INFO [test] event=test_event trace_id=abc123 key=value",
        );
        logger.flush();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("event=test_event"));
        assert!(content.contains("trace_id=abc123"));
        assert!(content.contains("key=value"));
        assert!(content.contains("INFO"));
    }

    #[test]
    fn logger_write_line_honors_path_isolation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("isolated.log");
        let logger = FloeLogger::new(
            false,
            path.to_path_buf(),
            LevelFilter::Info,
            LogRotation {
                max_bytes: 10_000,
                max_files: 2,
            },
        );

        logger.write_line("only this should appear");
        logger.flush();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content.trim(), "only this should appear");
    }

    #[test]
    fn logger_timestamp_has_iso8601_format() {
        let ts = timestamp();
        assert!(ts.len() >= 20);
        assert!(ts.ends_with('Z'));
        assert!(&ts[4..5] == "-");
        assert!(&ts[7..8] == "-");
        assert!(&ts[10..11] == "T");
    }

    #[test]
    fn format_log_line_has_expected_structure() {
        let record = Record::builder()
            .args(format_args!("event=test key=val"))
            .level(Level::Warn)
            .target("test_module")
            .build();

        let line = format_log_line(&record);
        assert!(line.contains("WARN"));
        assert!(line.contains("[test_module]"));
        assert!(line.contains("event=test"));
        assert!(line.contains("key=val"));
    }
}
