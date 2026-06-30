use async_trait::async_trait;

use crate::providers::cleanup::{CleanupError, CleanupProvider, CleanupSuccess};

const MOCK_CLEANUP_MODEL: &str = "qwen/qwen3.6-27b";

pub struct FakeCleanupProvider {
    response_text: String,
    fail: bool,
    error_code: Option<String>,
    latency_ms: u64,
}

impl FakeCleanupProvider {
    pub fn ok(response_text: impl Into<String>) -> Self {
        Self {
            response_text: response_text.into(),
            fail: false,
            error_code: None,
            latency_ms: 0,
        }
    }

    pub fn failing(error_code: impl Into<String>) -> Self {
        Self {
            response_text: String::new(),
            fail: true,
            error_code: Some(error_code.into()),
            latency_ms: 0,
        }
    }


}

#[async_trait]
impl CleanupProvider for FakeCleanupProvider {
    async fn cleanup(
        &self,
        _api_key: &str,
        _transcript: &str,
    ) -> Result<CleanupSuccess, CleanupError> {
        if self.latency_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.latency_ms)).await;
        }
        if self.fail {
            return Err(CleanupError {
                message: "fake cleanup failed".to_string(),
                model: MOCK_CLEANUP_MODEL.to_string(),
                retry_count: 0,
                validation_ms: 0,
                rate_limit: None,
                error_code: self.error_code.clone(),
            });
        }
        Ok(CleanupSuccess {
            text: self.response_text.clone(),
            model: MOCK_CLEANUP_MODEL.to_string(),
            retry_count: 0,
            validation_ms: 0,
            rate_limit: None,
        })
    }
}
