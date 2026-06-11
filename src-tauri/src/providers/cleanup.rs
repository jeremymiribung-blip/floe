use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_requests: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_tokens: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_requests: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_tokens: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CleanupSuccess {
    pub text: String,
    pub model: String,
    pub retry_count: u32,
    pub validation_ms: u64,
    pub rate_limit: Option<Box<RateLimitMetadata>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CleanupError {
    pub message: String,
    pub model: String,
    pub retry_count: u32,
    pub validation_ms: u64,
    pub rate_limit: Option<Box<RateLimitMetadata>>,
    pub error_code: Option<String>,
}

/// Common interface for transcript cleanup providers.
#[async_trait]
pub trait CleanupProvider: Send + Sync {
    async fn cleanup(
        &self,
        api_key: &str,
        transcript: &str,
    ) -> Result<CleanupSuccess, CleanupError>;
}
