use std::time::{Duration, Instant};

use reqwest::header::HeaderMap;

use super::types::GroqRateLimitMetadata;

pub const MAX_ATTEMPTS: usize = 3;
pub const INITIAL_RETRY_BACKOFF: Duration = Duration::from_millis(250);
pub const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);

pub fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
}

pub fn rate_limit_metadata(headers: &HeaderMap) -> Option<GroqRateLimitMetadata> {
    let metadata = GroqRateLimitMetadata {
        remaining_requests: header_value(headers, "x-ratelimit-remaining-requests"),
        remaining_tokens: header_value(headers, "x-ratelimit-remaining-tokens"),
        reset_requests: header_value(headers, "x-ratelimit-reset-requests"),
        reset_tokens: header_value(headers, "x-ratelimit-reset-tokens"),
        retry_after_seconds: retry_after(headers).map(|duration| duration.as_secs()),
    };

    if metadata == GroqRateLimitMetadata::default() {
        None
    } else {
        Some(metadata)
    }
}

pub fn header_value(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

pub fn retry_delay(base: Duration, attempt: usize) -> Duration {
    let multiplier = 1_u32.checked_shl((attempt - 1) as u32).unwrap_or(u32::MAX);
    base.saturating_mul(multiplier)
}

pub fn retry_count_for_attempt(attempt: usize) -> u32 {
    attempt.saturating_sub(1).try_into().unwrap_or(u32::MAX)
}
