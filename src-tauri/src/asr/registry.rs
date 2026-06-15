use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::Mutex;

use super::error::{RegistryError, RegistryErrorCode, SelectionError, SelectionErrorCode};
use super::traits::AsrProvider;
use super::types::{HealthStatus, SelectionCriteria};

pub struct ProviderRegistry {
    providers: HashMap<&'static str, Box<dyn AsrProvider>>,
    default_provider: Option<&'static str>,
    experimental: HashSet<&'static str>,
    disabled: HashSet<&'static str>,
    health_cache: Arc<Mutex<HashMap<&'static str, HealthStatus>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            default_provider: None,
            experimental: HashSet::new(),
            disabled: HashSet::new(),
            health_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register(&mut self, provider: Box<dyn AsrProvider>) -> Result<(), RegistryError> {
        let id = provider.id();
        if self.providers.contains_key(id) {
            return Err(RegistryError::new(
                RegistryErrorCode::DuplicateProvider,
                format!("Provider '{}' is already registered.", id),
            ));
        }
        if self.default_provider.is_none() {
            self.default_provider = Some(id);
        }
        self.providers.insert(id, provider);
        Ok(())
    }

    pub fn set_default(&mut self, id: &'static str) -> Result<(), RegistryError> {
        if !self.providers.contains_key(id) {
            return Err(RegistryError::new(
                RegistryErrorCode::ProviderNotFound,
                format!("Cannot set default: '{}' is not registered.", id),
            ));
        }
        self.default_provider = Some(id);
        Ok(())
    }

    pub fn mark_experimental(&mut self, id: &'static str) {
        self.experimental.insert(id);
    }

    pub fn mark_disabled(&mut self, id: &'static str) {
        self.disabled.insert(id);
    }

    pub fn enable(&mut self, id: &'static str) {
        self.disabled.remove(id);
    }

    pub fn get(&self, id: &str) -> Option<&dyn AsrProvider> {
        self.providers.get(id).map(AsRef::as_ref)
    }

    pub fn default(&self) -> Option<&dyn AsrProvider> {
        self.default_provider
            .and_then(|id| self.providers.get(id))
            .map(AsRef::as_ref)
    }

    pub fn default_id(&self) -> Option<&'static str> {
        self.default_provider
    }

    pub fn is_experimental(&self, id: &str) -> bool {
        self.experimental.contains(id)
    }

    pub fn is_disabled(&self, id: &str) -> bool {
        self.disabled.contains(id)
    }

    pub fn all_providers(&self) -> Vec<&dyn AsrProvider> {
        self.providers.values().map(AsRef::as_ref).collect()
    }

    pub fn provider_ids(&self) -> Vec<&'static str> {
        self.providers.keys().copied().collect()
    }

    pub fn select(&self, criteria: SelectionCriteria) -> Result<&dyn AsrProvider, SelectionError> {
        if let Some(ref preferred) = criteria.preferred {
            if !self.disabled.contains(preferred.as_str()) {
                if let Some(provider) = self.providers.get(preferred.as_str()) {
                    return Ok(provider.as_ref());
                }
                return Err(SelectionError::new(
                    SelectionErrorCode::PreferredProviderUnavailable,
                    format!("Preferred provider '{}' is not registered.", preferred),
                ));
            }
        }

        // Check preferred default first
        if let Some(default_id) = self.default_provider {
            if !self.disabled.contains(default_id) {
                if let Some(provider) = self.providers.get(default_id) {
                    let caps = provider.capabilities();
                    let matches = (!criteria.requires_fallback_compatible
                        || caps.fallback_compatible)
                        && (!criteria.requires_streaming
                            || matches!(caps.streaming, super::types::StreamingSupport::Full));
                    if matches {
                        return Ok(provider.as_ref());
                    }
                }
            }
        }

        for provider in self.providers.values() {
            let id = provider.id();
            if self.disabled.contains(id) {
                continue;
            }
            let caps = provider.capabilities();
            if criteria.requires_fallback_compatible && !caps.fallback_compatible {
                continue;
            }
            if criteria.requires_streaming
                && !matches!(caps.streaming, super::types::StreamingSupport::Full)
            {
                continue;
            }
            return Ok(provider.as_ref());
        }

        Err(SelectionError::new(
            SelectionErrorCode::NoSuitableProvider,
            "No ASR provider matches the selection criteria.",
        ))
    }

    /// Returns Groq as the default fallback provider if available and not disabled.
    /// Falls back to any other fallback-compatible provider if Groq is unavailable.
    /// Excludes the primary provider to avoid self-fallback loops.
    pub fn fallback_provider(&self) -> Option<&dyn AsrProvider> {
        self.fallback_provider_excluding(None)
    }

    /// Returns a fallback provider excluding the given primary provider id.
    /// If `excluding` is `None`, no provider is excluded.
    pub fn fallback_provider_excluding(&self, excluding: Option<&str>) -> Option<&dyn AsrProvider> {
        // Try Groq first as the preferred fallback
        if let Some(provider) = self.providers.get("groq") {
            if !self.disabled.contains("groq") && excluding != Some("groq") {
                return Some(provider.as_ref());
            }
        }

        // Fall back to any compatible provider that isn't the excluded one
        for provider in self.providers.values() {
            let id = provider.id();
            if self.disabled.contains(id) || excluding == Some(id) {
                continue;
            }
            if provider.capabilities().fallback_compatible {
                return Some(provider.as_ref());
            }
        }
        None
    }

    pub async fn update_health(&self, id: &'static str, status: HealthStatus) {
        let mut cache = self.health_cache.lock().await;
        cache.insert(id, status);
    }

    pub async fn get_health(&self, id: &str) -> Option<HealthStatus> {
        let cache = self.health_cache.lock().await;
        cache.get(id).cloned()
    }

    pub fn health_cache_arc(&self) -> Arc<Mutex<HashMap<&'static str, HealthStatus>>> {
        Arc::clone(&self.health_cache)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::error::SessionError;
    use crate::asr::traits::{AsrProvider, AsrSession};
    use crate::asr::types::*;
    use async_trait::async_trait;

    #[derive(Debug)]
    struct MockProvider {
        id: &'static str,
        fallback: bool,
    }

    #[async_trait]
    impl AsrProvider for MockProvider {
        fn id(&self) -> &'static str {
            self.id
        }
        fn name(&self) -> &'static str {
            self.id
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                fallback_compatible: self.fallback,
                ..Default::default()
            }
        }
        fn default_model(&self) -> &'static str {
            "model"
        }
        fn available_models(&self) -> &[ModelSpec] {
            &[]
        }
        async fn create_session(
            &self,
            _: SessionConfig,
        ) -> Result<Box<dyn AsrSession>, SessionError> {
            Err(SessionError::new(
                crate::asr::error::SessionErrorCode::Internal,
                "mock",
            ))
        }
        async fn health_check(&self) -> Result<HealthStatus, ()> {
            Ok(HealthStatus::Healthy)
        }
    }

    #[test]
    fn register_and_get() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "groq",
            fallback: true,
        }))
        .unwrap();
        assert!(reg.get("groq").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn prevents_duplicate_registration() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "groq",
            fallback: true,
        }))
        .unwrap();
        let err = reg
            .register(Box::new(MockProvider {
                id: "groq",
                fallback: true,
            }))
            .unwrap_err();
        assert_eq!(err.code, RegistryErrorCode::DuplicateProvider);
    }

    #[test]
    fn first_registered_becomes_default() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "a",
            fallback: false,
        }))
        .unwrap();
        reg.register(Box::new(MockProvider {
            id: "b",
            fallback: true,
        }))
        .unwrap();
        assert_eq!(reg.default_id(), Some("a"));
    }

    #[test]
    fn set_default_validates_registration() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "groq",
            fallback: true,
        }))
        .unwrap();
        assert!(reg.set_default("groq").is_ok());
        assert!(reg.set_default("missing").is_err());
    }

    #[test]
    fn select_preferred_provider() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "a",
            fallback: true,
        }))
        .unwrap();
        reg.register(Box::new(MockProvider {
            id: "b",
            fallback: false,
        }))
        .unwrap();
        let selected = reg
            .select(SelectionCriteria {
                preferred: Some("b".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(selected.id(), "b");
    }

    #[test]
    fn select_fallback_compatible() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "a",
            fallback: false,
        }))
        .unwrap();
        reg.register(Box::new(MockProvider {
            id: "b",
            fallback: true,
        }))
        .unwrap();
        let selected = reg
            .select(SelectionCriteria {
                requires_fallback_compatible: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(selected.id(), "b");
    }

    #[test]
    fn disabled_providers_are_skipped() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "a",
            fallback: true,
        }))
        .unwrap();
        reg.mark_disabled("a");
        reg.register(Box::new(MockProvider {
            id: "b",
            fallback: true,
        }))
        .unwrap();
        let selected = reg.select(SelectionCriteria::default()).unwrap();
        assert_eq!(selected.id(), "b");
    }

    #[test]
    fn select_returns_error_when_no_suitable_provider() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "a",
            fallback: false,
        }))
        .unwrap();
        reg.mark_disabled("a");
        let err = reg.select(SelectionCriteria::default()).unwrap_err();
        assert_eq!(err.code, SelectionErrorCode::NoSuitableProvider);
    }

    #[test]
    fn fallback_provider_returns_fallback_compatible() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "a",
            fallback: false,
        }))
        .unwrap();
        reg.register(Box::new(MockProvider {
            id: "b",
            fallback: true,
        }))
        .unwrap();
        let fb = reg.fallback_provider().unwrap();
        assert_eq!(fb.id(), "b");
    }

    #[test]
    fn fallback_provider_skips_disabled() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "a",
            fallback: true,
        }))
        .unwrap();
        reg.mark_disabled("a");
        assert!(reg.fallback_provider().is_none());
    }

    #[test]
    fn fallback_provider_prefers_groq_over_others() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "other_fallback",
            fallback: true,
        }))
        .unwrap();
        reg.register(Box::new(MockProvider {
            id: "groq",
            fallback: true,
        }))
        .unwrap();
        let fb = reg.fallback_provider().unwrap();
        assert_eq!(fb.id(), "groq");
    }

    #[test]
    fn fallback_provider_returns_groq_even_if_not_first() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "first_non_fallback",
            fallback: false,
        }))
        .unwrap();
        reg.register(Box::new(MockProvider {
            id: "groq",
            fallback: true,
        }))
        .unwrap();
        reg.register(Box::new(MockProvider {
            id: "another_fallback",
            fallback: true,
        }))
        .unwrap();
        let fb = reg.fallback_provider().unwrap();
        assert_eq!(fb.id(), "groq");
    }

    #[test]
    fn fallback_provider_falls_back_to_other_when_groq_disabled() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "groq",
            fallback: true,
        }))
        .unwrap();
        reg.register(Box::new(MockProvider {
            id: "other_fallback",
            fallback: true,
        }))
        .unwrap();
        reg.mark_disabled("groq");
        let fb = reg.fallback_provider().unwrap();
        assert_eq!(fb.id(), "other_fallback");
    }

    #[test]
    fn fallback_provider_returns_none_when_groq_missing_and_no_others() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "non_fallback",
            fallback: false,
        }))
        .unwrap();
        assert!(reg.fallback_provider().is_none());
    }

    #[test]
    fn experimental_flag() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "new",
            fallback: false,
        }))
        .unwrap();
        reg.mark_experimental("new");
        assert!(reg.is_experimental("new"));
        assert!(!reg.is_experimental("nonexistent"));
    }

    #[test]
    fn enable_reactivates_disabled() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "a",
            fallback: true,
        }))
        .unwrap();
        reg.mark_disabled("a");
        assert!(reg.is_disabled("a"));
        reg.enable("a");
        assert!(!reg.is_disabled("a"));
    }

    #[tokio::test]
    async fn health_cache_round_trip() {
        let mut reg = ProviderRegistry::new();
        reg.register(Box::new(MockProvider {
            id: "groq",
            fallback: true,
        }))
        .unwrap();
        reg.update_health("groq", HealthStatus::Degraded("slow".into()))
            .await;
        let status = reg.get_health("groq").await.unwrap();
        assert!(matches!(status, HealthStatus::Degraded(_)));
    }
}
