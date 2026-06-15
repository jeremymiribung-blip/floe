use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const DIAG_FILENAME: &str = "floe-diag.log";

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
}
