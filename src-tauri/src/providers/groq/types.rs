use crate::providers::cleanup::RateLimitMetadata;
use serde::Serialize;

pub const GROQ_STT_MODEL: &str = "whisper-large-v3-turbo";
pub const GROQ_CLEANUP_MODEL: &str = "qwen/qwen3.6-27b";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GroqTranscription {
    pub text: String,
    pub model: String,
    pub retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<Box<RateLimitMetadata>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GroqTranscriptionError {
    pub domain: &'static str,
    pub code: GroqTranscriptionErrorCode,
    pub message: String,
    pub model: String,
    pub retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<Box<RateLimitMetadata>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GroqTranscriptionErrorCode {
    MissingApiKey,
    InvalidApiKey,
    RateLimit,
    Timeout,
    ApiUnreachable,
    MalformedResponse,
    UnsupportedAudio,
    InvalidRequest,
    EmptyAudio,
    ServerError,
}


#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqCleanup {
    pub text: String,
    pub model: String,
    pub retry_count: u32,
    pub validation_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<Box<RateLimitMetadata>>,
}

impl PartialEq<&str> for GroqCleanup {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqCleanupError {
    pub domain: &'static str,
    pub code: GroqCleanupErrorCode,
    pub message: String,
    pub model: String,
    pub retry_count: u32,
    pub validation_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<Box<RateLimitMetadata>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GroqCleanupErrorCode {
    MissingApiKey,
    InvalidApiKey,
    RateLimit,
    Timeout,
    ApiUnreachable,
    MalformedResponse,
    InvalidRequest,
    EmptyTranscript,
    ValidationFailed,
    ServerError,
}


