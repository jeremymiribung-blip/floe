use std::collections::HashMap;

use super::types::ModelSpec;

pub struct ModelManager {
    models: HashMap<&'static str, Vec<ModelSpec>>,
    overrides: HashMap<String, String>,
}

impl ModelManager {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            overrides: HashMap::new(),
        }
    }

    pub fn register(&mut self, provider_id: &'static str, models: Vec<ModelSpec>) {
        self.models.insert(provider_id, models);
    }

    pub fn models_for(&self, provider_id: &str) -> &[ModelSpec] {
        self.models
            .get(provider_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn default_model(&self, provider_id: &str) -> Option<&ModelSpec> {
        let models = self.models.get(provider_id)?;
        models.first()
    }

    pub fn find_model(&self, provider_id: &str, model_id: &str) -> Option<&ModelSpec> {
        self.models
            .get(provider_id)?
            .iter()
            .find(|m| m.id == model_id)
    }

    pub fn set_override(&mut self, provider_id: &str, model_id: &str) {
        self.overrides
            .insert(provider_id.to_string(), model_id.to_string());
    }

    pub fn clear_override(&mut self, provider_id: &str) {
        self.overrides.remove(provider_id);
    }

    pub fn effective_model<'a>(&'a self, provider_id: &str) -> Option<&'a ModelSpec> {
        if let Some(override_id) = self.overrides.get(provider_id) {
            let result = self.find_model(provider_id, override_id);
            if result.is_some() {
                return result;
            }
        }
        self.default_model(provider_id)
    }

    pub fn all_model_ids(&self, provider_id: &str) -> Vec<String> {
        self.models_for(provider_id)
            .iter()
            .map(|m| m.id.to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_models() -> Vec<ModelSpec> {
        vec![
            ModelSpec {
                id: "tiny",
                name: "Tiny",
                requires_gpu: false,
                max_duration_secs: 120,
                supported_languages: None,
                parameters: Some("39M".into()),
            },
            ModelSpec {
                id: "base",
                name: "Base",
                requires_gpu: false,
                max_duration_secs: 120,
                supported_languages: None,
                parameters: Some("74M".into()),
            },
            ModelSpec {
                id: "large-v3",
                name: "Large v3",
                requires_gpu: true,
                max_duration_secs: 300,
                supported_languages: None,
                parameters: Some("1550M".into()),
            },
        ]
    }

    #[test]
    fn register_and_default() {
        let mut mm = ModelManager::new();
        mm.register("whisper", test_models());
        let default = mm.default_model("whisper").unwrap();
        assert_eq!(default.id, "tiny");
    }

    #[test]
    fn find_exact_model() {
        let mut mm = ModelManager::new();
        mm.register("whisper", test_models());
        let found = mm.find_model("whisper", "base").unwrap();
        assert_eq!(found.parameters, Some("74M"));
    }

    #[test]
    fn missing_provider_returns_none() {
        let mm = ModelManager::new();
        assert!(mm.default_model("nonexistent").is_none());
    }

    #[test]
    fn override_takes_priority() {
        let mut mm = ModelManager::new();
        mm.register("whisper", test_models());
        mm.set_override("whisper", "large-v3");
        let effective = mm.effective_model("whisper").unwrap();
        assert_eq!(effective.id, "large-v3");
    }

    #[test]
    fn invalid_override_falls_back_to_default() {
        let mut mm = ModelManager::new();
        mm.register("whisper", test_models());
        mm.set_override("whisper", "nonexistent");
        let effective = mm.effective_model("whisper").unwrap();
        assert_eq!(effective.id, "tiny");
    }

    #[test]
    fn clear_override_restores_default() {
        let mut mm = ModelManager::new();
        mm.register("whisper", test_models());
        mm.set_override("whisper", "large-v3");
        mm.clear_override("whisper");
        let effective = mm.effective_model("whisper").unwrap();
        assert_eq!(effective.id, "tiny");
    }

    #[test]
    fn all_model_ids_returns_strings() {
        let mut mm = ModelManager::new();
        mm.register("whisper", test_models());
        let ids = mm.all_model_ids("whisper");
        assert_eq!(ids, vec!["tiny", "base", "large-v3"]);
    }

    #[test]
    fn models_for_returns_empty_slice_for_unknown() {
        let mm = ModelManager::new();
        assert!(mm.models_for("unknown").is_empty());
    }
}
