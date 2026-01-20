//! Rate limiter for GitHub API requests.
//!
//! This module provides a global rate limiter that enforces backpressure
//! to avoid hitting GitHub API rate limits. It tracks remaining quota
//! and implements exponential backoff when limits are approached.

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Global rate limiter state for GitHub API
static GITHUB_RATE_LIMIT: Mutex<RateLimitState> = Mutex::new(RateLimitState::new());

/// Minimum requests to keep in reserve before applying backpressure
const RESERVE_LIMIT: u64 = 10;

/// State tracking for rate limiting
struct RateLimitState {
    /// Remaining requests in current window
    remaining: u64,
    /// Timestamp when the rate limit resets (Unix seconds)
    reset_at: u64,
    /// Last time we made a request (for adaptive delays)
    last_request_at: u64,
    /// Number of consecutive 429 responses
    consecutive_429s: u32,
}

impl RateLimitState {
    const fn new() -> Self {
        Self {
            remaining: 60, // GitHub's unauthenticated limit
            reset_at: 0,
            last_request_at: 0,
            consecutive_429s: 0,
        }
    }
}

/// Result of checking rate limit
pub enum RateLimitResult {
    /// OK to proceed with request
    Proceed,
    /// Should wait before making request
    Wait(Duration),
    /// Rate limited - should not make request
    Limited(Duration),
}

/// Check if we can make a GitHub API request.
/// Returns guidance on whether to proceed, wait, or skip.
pub fn check_github_rate_limit() -> RateLimitResult {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut state = match GITHUB_RATE_LIMIT.lock() {
        Ok(s) => s,
        Err(poisoned) => poisoned.into_inner(),
    };

    // If we've passed the reset time, assume limits are refreshed
    if now > state.reset_at {
        state.remaining = 60;
        state.consecutive_429s = 0;
    }

    // If we have plenty of quota, proceed
    if state.remaining > RESERVE_LIMIT {
        return RateLimitResult::Proceed;
    }

    // If we're low on quota but not depleted, add adaptive delay
    if state.remaining > 0 {
        // Delay based on how close we are to the limit
        let delay_ms = match state.remaining {
            1..=3 => 2000,
            4..=6 => 1000,
            7..=10 => 500,
            _ => 100,
        };
        return RateLimitResult::Wait(Duration::from_millis(delay_ms));
    }

    // Quota depleted - calculate wait time until reset
    if state.reset_at > now {
        return RateLimitResult::Limited(Duration::from_secs(state.reset_at - now));
    }

    // Reset time passed but we haven't confirmed new quota yet
    RateLimitResult::Wait(Duration::from_millis(1000))
}

/// Update rate limit state from response headers.
/// Call this after each GitHub API response.
pub fn update_github_rate_limit(remaining: Option<u64>, reset_at: Option<u64>, was_429: bool) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut state = match GITHUB_RATE_LIMIT.lock() {
        Ok(s) => s,
        Err(poisoned) => poisoned.into_inner(),
    };

    if let Some(r) = remaining {
        state.remaining = r;
    }
    if let Some(reset) = reset_at {
        state.reset_at = reset;
    }

    state.last_request_at = now;

    if was_429 {
        state.consecutive_429s = state.consecutive_429s.saturating_add(1);
        // On 429, conservatively assume we're out of quota
        state.remaining = 0;
    } else {
        state.consecutive_429s = 0;
    }
}

/// Parse rate limit headers from a reqwest response.
/// Returns (remaining, reset_timestamp).
pub fn parse_rate_limit_headers(
    headers: &reqwest::header::HeaderMap,
) -> (Option<u64>, Option<u64>) {
    let remaining = headers
        .get("X-RateLimit-Remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());

    let reset = headers
        .get("X-RateLimit-Reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());

    (remaining, reset)
}

/// Calculate exponential backoff delay for retries.
pub fn exponential_backoff(attempt: u32, base_ms: u64, max_ms: u64) -> Duration {
    let delay = base_ms.saturating_mul(1u64 << attempt.min(6));
    Duration::from_millis(delay.min(max_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff() {
        assert_eq!(
            exponential_backoff(0, 100, 10000),
            Duration::from_millis(100)
        );
        assert_eq!(
            exponential_backoff(1, 100, 10000),
            Duration::from_millis(200)
        );
        assert_eq!(
            exponential_backoff(2, 100, 10000),
            Duration::from_millis(400)
        );
        assert_eq!(
            exponential_backoff(3, 100, 10000),
            Duration::from_millis(800)
        );
        // Test max cap
        assert_eq!(
            exponential_backoff(10, 100, 5000),
            Duration::from_millis(5000)
        );
    }

    #[test]
    fn test_rate_limit_state_new() {
        let state = RateLimitState::new();
        assert_eq!(state.remaining, 60);
        assert_eq!(state.reset_at, 0);
        assert_eq!(state.consecutive_429s, 0);
    }
}
