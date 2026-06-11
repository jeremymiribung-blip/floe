use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AsrError {
    pub code: AsrErrorCode,
    pub message: String,
    pub retry_count: u32,
    pub diagnostics_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AsrErrorCode {
    NoProvider,
    ProviderUnhealthy,
    ProviderRejected,
    ModelNotFound,
    TranscriptionFailed,
    FallbackFailed,
    AudioTooLong,
    AudioTooLarge,
    AudioEmpty,
    SessionTimeout,
    Internal,
}

impl AsrError {
    pub fn new(code: AsrErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retry_count: 0,
            diagnostics_json: None,
        }
    }
}

impl fmt::Display for AsrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionError {
    pub code: SessionErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionErrorCode {
    ProviderNotFound,
    ModelUnavailable,
    SessionLimitReached,
    GpuMemoryExhausted,
    InvalidConfig,
    AlreadyFinalized,
    Internal,
}

impl SessionError {
    pub fn new(code: SessionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryError {
    pub code: RegistryErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryErrorCode {
    DuplicateProvider,
    ProviderNotFound,
    DefaultProviderNotRegistered,
}

impl RegistryError {
    pub fn new(code: RegistryErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionError {
    pub code: SelectionErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionErrorCode {
    NoSuitableProvider,
    PreferredProviderUnavailable,
    PreferredProviderUnhealthy,
    AllProvidersDisabled,
}

impl SelectionError {
    pub fn new(code: SelectionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asr_error_display_does_not_panic() {
        let err = AsrError::new(AsrErrorCode::AudioEmpty, "no audio recorded");
        let display = format!("{}", err);
        assert!(display.contains("AudioEmpty"));
        assert!(display.contains("no audio recorded"));
    }

    #[test]
    fn asr_error_serialization() {
        let err = AsrError::new(AsrErrorCode::TranscriptionFailed, "provider error");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("transcription_failed"));
        assert!(!json.contains("Bearer"));
    }

    #[test]
    fn session_error_constructs() {
        let err = SessionError::new(SessionErrorCode::SessionLimitReached, "too many sessions");
        assert_eq!(err.code, SessionErrorCode::SessionLimitReached);
    }

    #[test]
    fn registry_error_constructs() {
        let err = RegistryError::new(
            RegistryErrorCode::DuplicateProvider,
            "groq is already registered",
        );
        assert_eq!(err.code, RegistryErrorCode::DuplicateProvider);
    }

    #[test]
    fn selection_error_constructs() {
        let err = SelectionError::new(SelectionErrorCode::NoSuitableProvider, "no provider found");
        assert_eq!(err.code, SelectionErrorCode::NoSuitableProvider);
    }
}
