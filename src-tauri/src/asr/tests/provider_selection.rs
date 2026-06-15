//! Tests for generic provider selection in the ASR architecture.

use crate::asr::registry::ProviderRegistry;
use crate::asr::traits::AsrProvider;
use crate::asr::types::{Deployment, HealthStatus, ProviderCapabilities, SelectionCriteria};
use async_trait::async_trait;

/// Mock provider for testing
#[derive(Debug)]
pub struct MockProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub fallback_compatible: bool,
    pub streaming: bool,
    pub healthy: bool,
}

#[async_trait]
impl AsrProvider for MockProvider {
    fn id(&self) -> &'static str {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            backend_type: super::super::types::BackendType::Cloud,
            deployment: Deployment::Cloud,
            streaming: if self.streaming {
                super::super::types::StreamingSupport::Full
            } else {
                super::super::types::StreamingSupport::None
            },
            partials: false,
            timestamps: false,
            gpu_required: false,
            fallback_compatible: self.fallback_compatible,
            max_audio_seconds: 120,
            supported_sample_rates: vec![16_000],
            min_audio_bytes: 1,
            max_audio_bytes: 25_000_000,
        }
    }
    fn default_model(&self) -> &'static str {
        "test-model"
    }
    fn available_models(&self) -> &[crate::asr::types::ModelSpec] {
        &[]
    }
    async fn create_session(
        &self,
        _: crate::asr::types::SessionConfig,
    ) -> Result<Box<dyn crate::asr::traits::AsrSession>, crate::asr::error::SessionError> {
        Err(crate::asr::error::SessionError::new(
            crate::asr::error::SessionErrorCode::Internal,
            "mock session not implemented",
        ))
    }
    async fn health_check(&self) -> Result<HealthStatus, ()> {
        if self.healthy {
            Ok(HealthStatus::Healthy)
        } else {
            Ok(HealthStatus::Unhealthy("mock unhealthy".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_registered_becomes_default() {
        let mut registry = ProviderRegistry::new();
        let groq = Box::new(MockProvider {
            id: "groq",
            name: "Groq",
            fallback_compatible: true,
            streaming: true,
            healthy: true,
        });
        let other = Box::new(MockProvider {
            id: "other",
            name: "Other",
            fallback_compatible: false,
            streaming: false,
            healthy: true,
        });

        registry.register(groq).unwrap();
        registry.register(other).unwrap();

        assert_eq!(registry.default_id(), Some("groq"));
    }

    #[test]
    fn set_default_explicitly() {
        let mut registry = ProviderRegistry::new();
        let groq = Box::new(MockProvider {
            id: "groq",
            name: "Groq",
            fallback_compatible: true,
            streaming: true,
            healthy: true,
        });
        let other = Box::new(MockProvider {
            id: "other",
            name: "Other",
            fallback_compatible: false,
            streaming: false,
            healthy: true,
        });

        registry.register(groq).unwrap();
        registry.register(other).unwrap();
        registry.set_default("other").unwrap();

        assert_eq!(registry.default_id(), Some("other"));
    }

    #[test]
    fn select_preferred_provider() {
        let mut registry = ProviderRegistry::new();
        let groq = Box::new(MockProvider {
            id: "groq",
            name: "Groq",
            fallback_compatible: true,
            streaming: true,
            healthy: true,
        });
        let other = Box::new(MockProvider {
            id: "other",
            name: "Other",
            fallback_compatible: false,
            streaming: false,
            healthy: true,
        });

        registry.register(groq).unwrap();
        registry.register(other).unwrap();

        let criteria = SelectionCriteria {
            preferred: Some("other".to_string()),
            ..Default::default()
        };

        let selected = registry.select(criteria).unwrap();
        assert_eq!(selected.id(), "other");
    }

    #[test]
    fn select_default_when_no_preference() {
        let mut registry = ProviderRegistry::new();
        let groq = Box::new(MockProvider {
            id: "groq",
            name: "Groq",
            fallback_compatible: true,
            streaming: true,
            healthy: true,
        });
        let other = Box::new(MockProvider {
            id: "other",
            name: "Other",
            fallback_compatible: false,
            streaming: false,
            healthy: true,
        });

        registry.register(groq).unwrap();
        registry.register(other).unwrap();

        let criteria = SelectionCriteria::default();
        let selected = registry.select(criteria).unwrap();
        assert_eq!(selected.id(), "groq");
    }

    #[test]
    fn select_fallback_compatible_provider() {
        let mut registry = ProviderRegistry::new();
        let groq = Box::new(MockProvider {
            id: "groq",
            name: "Groq",
            fallback_compatible: true,
            streaming: true,
            healthy: true,
        });
        let other = Box::new(MockProvider {
            id: "other",
            name: "Other",
            fallback_compatible: false,
            streaming: false,
            healthy: true,
        });

        registry.register(groq).unwrap();
        registry.register(other).unwrap();

        let criteria = SelectionCriteria {
            requires_fallback_compatible: true,
            ..Default::default()
        };

        let selected = registry.select(criteria).unwrap();
        assert_eq!(selected.id(), "groq");
    }

    #[test]
    fn select_streaming_provider() {
        let mut registry = ProviderRegistry::new();
        let groq = Box::new(MockProvider {
            id: "groq",
            name: "Groq",
            fallback_compatible: true,
            streaming: true,
            healthy: true,
        });
        let other = Box::new(MockProvider {
            id: "other",
            name: "Other",
            fallback_compatible: false,
            streaming: false,
            healthy: true,
        });

        registry.register(groq).unwrap();
        registry.register(other).unwrap();

        let criteria = SelectionCriteria {
            requires_streaming: true,
            ..Default::default()
        };

        let selected = registry.select(criteria).unwrap();
        assert_eq!(selected.id(), "groq");
    }

    #[test]
    fn disabled_providers_are_skipped() {
        let mut registry = ProviderRegistry::new();
        let groq = Box::new(MockProvider {
            id: "groq",
            name: "Groq",
            fallback_compatible: true,
            streaming: true,
            healthy: true,
        });
        let other = Box::new(MockProvider {
            id: "other",
            name: "Other",
            fallback_compatible: false,
            streaming: false,
            healthy: true,
        });

        registry.register(groq).unwrap();
        registry.register(other).unwrap();
        registry.mark_disabled("groq");

        let criteria = SelectionCriteria::default();
        let selected = registry.select(criteria).unwrap();
        assert_eq!(selected.id(), "other");
    }

    #[test]
    fn no_suitable_provider_returns_error() {
        let mut registry = ProviderRegistry::new();
        let groq = Box::new(MockProvider {
            id: "groq",
            name: "Groq",
            fallback_compatible: true,
            streaming: true,
            healthy: true,
        });

        registry.register(groq).unwrap();
        registry.mark_disabled("groq");

        let criteria = SelectionCriteria::default();

        let result = registry.select(criteria);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code,
            crate::asr::error::SelectionErrorCode::NoSuitableProvider
        );
    }

    #[test]
    fn fallback_provider_returns_groq_by_default() {
        let mut registry = ProviderRegistry::new();
        let groq = Box::new(MockProvider {
            id: "groq",
            name: "Groq",
            fallback_compatible: true,
            streaming: true,
            healthy: true,
        });
        let other = Box::new(MockProvider {
            id: "other",
            name: "Other",
            fallback_compatible: true,
            streaming: false,
            healthy: true,
        });

        registry.register(groq).unwrap();
        registry.register(other).unwrap();

        let fallback = registry.fallback_provider().unwrap();
        assert_eq!(fallback.id(), "groq");
    }

    #[test]
    fn fallback_provider_skips_disabled_groq() {
        let mut registry = ProviderRegistry::new();
        let groq = Box::new(MockProvider {
            id: "groq",
            name: "Groq",
            fallback_compatible: true,
            streaming: true,
            healthy: true,
        });
        let other = Box::new(MockProvider {
            id: "other",
            name: "Other",
            fallback_compatible: true,
            streaming: false,
            healthy: true,
        });

        registry.register(groq).unwrap();
        registry.register(other).unwrap();
        registry.mark_disabled("groq");

        let fallback = registry.fallback_provider().unwrap();
        assert_eq!(fallback.id(), "other");
    }

    #[test]
    fn fallback_provider_returns_none_when_no_compatible() {
        let mut registry = ProviderRegistry::new();
        let other = Box::new(MockProvider {
            id: "other",
            name: "Other",
            fallback_compatible: false,
            streaming: false,
            healthy: true,
        });

        registry.register(other).unwrap();

        assert!(registry.fallback_provider().is_none());
    }

    #[test]
    fn enable_reactivates_disabled() {
        let mut registry = ProviderRegistry::new();
        let groq = Box::new(MockProvider {
            id: "groq",
            name: "Groq",
            fallback_compatible: true,
            streaming: true,
            healthy: true,
        });

        registry.register(groq).unwrap();
        registry.mark_disabled("groq");
        assert!(registry.is_disabled("groq"));

        registry.enable("groq");
        assert!(!registry.is_disabled("groq"));
    }

    #[test]
    fn experimental_flag() {
        let mut registry = ProviderRegistry::new();
        let other = Box::new(MockProvider {
            id: "other",
            name: "Other",
            fallback_compatible: false,
            streaming: false,
            healthy: true,
        });

        registry.register(other).unwrap();
        registry.mark_experimental("other");

        assert!(registry.is_experimental("other"));
        assert!(!registry.is_experimental("groq"));
    }
}
