use std::path::PathBuf;
use std::sync::RwLock;

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ModelPaths {
    pub encoder_path: PathBuf,
    pub decoder_path: PathBuf,
    pub model_name: String,
}

#[derive(Debug)]
pub enum ModelError {
    NotFound(String),
    LoadFailed(String),
    IoError(String),
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "model not found: {}", msg),
            Self::LoadFailed(msg) => write!(f, "model load failed: {}", msg),
            Self::IoError(msg) => write!(f, "model i/o error: {}", msg),
        }
    }
}

impl std::error::Error for ModelError {}

const MODELS: &[(&str, &str, &str)] = &[
    ("whisper-tiny", "Whisper Tiny", "39M"),
    ("whisper-base", "Whisper Base", "74M"),
];

pub fn available_model_names() -> &'static [&'static str] {
    &["whisper-tiny", "whisper-base"]
}

pub fn model_specs() -> Vec<super::super::types::ModelSpec> {
    MODELS
        .iter()
        .map(|&(id, name, params)| super::super::types::ModelSpec {
            id,
            name,
            requires_gpu: false,
            max_duration_secs: 30,
            supported_languages: None,
            parameters: Some(params),
        })
        .collect()
}

#[derive(Debug)]
pub struct ModelCache {
    paths: RwLock<HashMap<String, ModelPaths>>,
    model_dir: PathBuf,
}

impl ModelCache {
    pub fn new(model_dir: PathBuf) -> Self {
        Self {
            paths: RwLock::new(HashMap::new()),
            model_dir,
        }
    }

    fn resolve_model_path(&self, model_name: &str) -> Option<ModelPaths> {
        let model_dir = self.model_dir.join(model_name);
        let encoder_path = model_dir.join("encoder_model.onnx");
        let decoder_path = model_dir.join("decoder_model.onnx");
        if encoder_path.exists() && decoder_path.exists() {
            Some(ModelPaths {
                encoder_path,
                decoder_path,
                model_name: model_name.to_string(),
            })
        } else {
            None
        }
    }

    pub fn get_or_load(&self, model_name: &str) -> Result<ModelPaths, ModelError> {
        {
            let cache = self.paths.read().map_err(|e| {
                ModelError::IoError(format!("cache lock poisoned: {}", e))
            })?;
            if let Some(paths) = cache.get(model_name) {
                return Ok(paths.clone());
            }
        }

        let paths = self
            .resolve_model_path(model_name)
            .ok_or_else(|| ModelError::NotFound(format!("{} not found in {:?}", model_name, self.model_dir)))?;

        {
            let mut cache = self.paths.write().map_err(|e| {
                ModelError::IoError(format!("cache lock poisoned: {}", e))
            })?;
            cache.insert(model_name.to_string(), paths.clone());
        }

        Ok(paths)
    }

    pub fn clear(&self) {
        if let Ok(mut cache) = self.paths.write() {
            cache.clear();
        }
    }

    pub fn has_model(&self, model_name: &str) -> bool {
        self.resolve_model_path(model_name).is_some()
    }

    pub fn model_dir(&self) -> &PathBuf {
        &self.model_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_models_are_listed() {
        let names = available_model_names();
        assert!(names.contains(&"whisper-tiny"));
        assert!(names.contains(&"whisper-base"));
    }

    #[test]
    fn model_specs_are_populated() {
        let specs = model_specs();
        assert_eq!(specs.len(), 2);
        assert!(specs.iter().any(|s| s.id == "whisper-tiny"));
    }

    #[test]
    fn cache_initially_empty() {
        let cache = ModelCache::new(PathBuf::from("/nonexistent"));
        let cache_guard = cache.paths.read().unwrap();
        assert!(cache_guard.is_empty());
    }

    #[test]
    fn get_or_load_missing_model_returns_error() {
        let cache = ModelCache::new(PathBuf::from("/nonexistent"));
        let result = cache.get_or_load("whisper-tiny");
        assert!(result.is_err());
        match result {
            Err(ModelError::NotFound(_)) => {}
            _ => panic!("expected NotFound error"),
        }
    }

    #[test]
    fn has_model_on_nonexistent_path_returns_false() {
        let cache = ModelCache::new(PathBuf::from("/nonexistent"));
        assert!(!cache.has_model("whisper-tiny"));
    }

    #[test]
    fn clear_does_not_panic_on_empty() {
        let cache = ModelCache::new(PathBuf::from("/nonexistent"));
        cache.clear();
    }

    #[test]
    fn model_dir_returns_configured_path() {
        let cache = ModelCache::new(PathBuf::from("/tmp/models"));
        assert_eq!(cache.model_dir(), &PathBuf::from("/tmp/models"));
    }
}
