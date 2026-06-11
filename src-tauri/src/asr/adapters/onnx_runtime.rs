use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use ort::session::Session;

use super::model_cache::ModelPaths;
use crate::asr::error::{SessionError, SessionErrorCode};

#[derive(Debug, Clone)]
pub struct CachedModel {
    pub paths: ModelPaths,
    pub encoder_modified: u64,
    pub decoder_modified: u64,
    pub tokenizer_modified: Option<u64>,
}

#[derive(Debug)]
pub struct SessionCache {
    sessions: RwLock<HashMap<String, CachedModel>>,
    model_dir: PathBuf,
}

impl SessionCache {
    pub fn new(model_dir: PathBuf) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            model_dir,
        }
    }

    pub fn get_or_load(&self, model_name: &str) -> Result<CachedModel, SessionError> {
        {
            let cache = self.sessions.read().map_err(|e| {
                SessionError::new(
                    SessionErrorCode::Internal,
                    format!("session cache lock poisoned: {}", e),
                )
            })?;
            if let Some(cached) = cache.get(model_name) {
                if Self::validate_cached(cached) {
                    return Ok(cached.clone());
                }
            }
        }

        let model_dir = self.model_dir.join(model_name);
        let encoder_path = model_dir.join("encoder_model.onnx");
        let decoder_path = model_dir.join("decoder_model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !encoder_path.exists() {
            return Err(SessionError::new(
                SessionErrorCode::ModelUnavailable,
                format!(
                    "encoder model not found: {}",
                    encoder_path.display()
                ),
            ));
        }
        if !decoder_path.exists() {
            return Err(SessionError::new(
                SessionErrorCode::ModelUnavailable,
                format!(
                    "decoder model not found: {}",
                    decoder_path.display()
                ),
            ));
        }

        let cached = CachedModel {
            paths: ModelPaths {
                encoder_path: encoder_path.clone(),
                decoder_path: decoder_path.clone(),
                model_name: model_name.to_string(),
            },
            encoder_modified: Self::file_modified(&encoder_path),
            decoder_modified: Self::file_modified(&decoder_path),
            tokenizer_modified: if tokenizer_path.exists() {
                Some(Self::file_modified(&tokenizer_path))
            } else {
                None
            },
        };

        {
            let mut cache = self.sessions.write().map_err(|e| {
                SessionError::new(
                    SessionErrorCode::Internal,
                    format!("session cache lock poisoned: {}", e),
                )
            })?;
            cache.insert(model_name.to_string(), cached.clone());
        }

        Ok(cached)
    }

    pub fn load_encoder_session(
        cached: &CachedModel,
    ) -> Result<Session, SessionError> {
        Session::builder()
            .map_err(|e| {
                SessionError::new(
                    SessionErrorCode::Internal,
                    format!("session builder failed: {}", e),
                )
            })?
            .commit_from_file(&cached.paths.encoder_path)
            .map_err(|e| {
                SessionError::new(
                    SessionErrorCode::Internal,
                    format!("encoder model load failed: {}", e),
                )
            })
    }

    pub fn load_decoder_session(
        cached: &CachedModel,
    ) -> Result<Session, SessionError> {
        Session::builder()
            .map_err(|e| {
                SessionError::new(
                    SessionErrorCode::Internal,
                    format!("session builder failed: {}", e),
                )
            })?
            .commit_from_file(&cached.paths.decoder_path)
            .map_err(|e| {
                SessionError::new(
                    SessionErrorCode::Internal,
                    format!("decoder model load failed: {}", e),
                )
            })
    }

    pub fn load_tokenizer(
        cached: &CachedModel,
    ) -> Result<tokenizers::Tokenizer, SessionError> {
        let tokenizer_path = cached
            .paths
            .model_dir_path()
            .join("tokenizer.json");

        if !tokenizer_path.exists() {
            return Err(SessionError::new(
                SessionErrorCode::ModelUnavailable,
                format!(
                    "tokenizer not found: {}",
                    tokenizer_path.display()
                ),
            ));
        }

        tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            SessionError::new(
                SessionErrorCode::Internal,
                format!("tokenizer load failed: {}", e),
            )
        })
    }

    pub fn clear(&self) {
        if let Ok(mut cache) = self.sessions.write() {
            cache.clear();
        }
    }

    pub fn has_model(&self, model_name: &str) -> bool {
        let model_dir = self.model_dir.join(model_name);
        model_dir.join("encoder_model.onnx").exists()
            && model_dir.join("decoder_model.onnx").exists()
    }

    pub fn model_dir(&self) -> &PathBuf {
        &self.model_dir
    }

    fn validate_cached(cached: &CachedModel) -> bool {
        cached.paths.encoder_path.exists()
            && cached.paths.decoder_path.exists()
            && Self::file_modified(&cached.paths.encoder_path) == cached.encoder_modified
            && Self::file_modified(&cached.paths.decoder_path) == cached.decoder_modified
    }

    fn file_modified(path: &Path) -> u64 {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_initially_empty() {
        let cache = SessionCache::new(PathBuf::from("/nonexistent"));
        let guard = cache.sessions.read().unwrap();
        assert!(guard.is_empty());
    }

    #[test]
    fn get_or_load_missing_model_returns_error() {
        let cache = SessionCache::new(PathBuf::from("/nonexistent"));
        let result = cache.get_or_load("whisper-tiny");
        assert!(result.is_err());
    }

    #[test]
    fn has_model_on_nonexistent_path_returns_false() {
        let cache = SessionCache::new(PathBuf::from("/nonexistent"));
        assert!(!cache.has_model("whisper-tiny"));
    }

    #[test]
    fn clear_does_not_panic_on_empty() {
        let cache = SessionCache::new(PathBuf::from("/nonexistent"));
        cache.clear();
    }

    #[test]
    fn model_dir_returns_configured_path() {
        let cache = SessionCache::new(PathBuf::from("/tmp/models"));
        assert_eq!(cache.model_dir(), &PathBuf::from("/tmp/models"));
    }
}
