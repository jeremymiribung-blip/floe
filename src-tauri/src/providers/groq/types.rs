use crate::providers::cleanup::RateLimitMetadata;
use serde::Serialize;

pub const GROQ_STT_MODEL: &str = "whisper-large-v3-turbo";
pub const GROQ_CLEANUP_MODEL: &str = "llama-3.3-70b-versatile";

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

impl GroqTranscriptionError {
    #[allow(dead_code)]
    pub fn new(code: GroqTranscriptionErrorCode, message: &'static str) -> Self {
        Self {
            domain: "stt",
            code,
            message: message.to_string(),
            model: GROQ_STT_MODEL.to_string(),
            retry_count: 0,
            rate_limit: None,
        }
    }
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

impl GroqCleanupError {
    #[allow(dead_code)]
    pub fn new(code: GroqCleanupErrorCode, message: &'static str) -> Self {
        Self {
            domain: "cleanup",
            code,
            message: message.to_string(),
            model: GROQ_CLEANUP_MODEL.to_string(),
            retry_count: 0,
            validation_ms: 0,
            rate_limit: None,
        }
    }
}
