//! Retry utilities with exponential backoff
//!
//! This module provides utilities for retrying operations with exponential backoff,
//! particularly useful for network requests to external APIs.

use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Configuration for retry behavior
#[derive(Clone, Debug)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Maximum delay between retries
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(500),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(10),
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration with custom values
    pub fn new(max_attempts: u32, initial_delay: Duration) -> Self {
        Self {
            max_attempts,
            initial_delay,
            ..Default::default()
        }
    }

    /// Calculate the delay for a given attempt number
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay_ms = self.initial_delay.as_millis() as f64
            * self.backoff_multiplier.powi(attempt as i32 - 1);
        let delay = Duration::from_millis(delay_ms as u64);
        delay.min(self.max_delay)
    }
}

/// Retry a function with exponential backoff
pub async fn retry_with_backoff<F, T, E, Fut>(
    config: &RetryConfig,
    mut f: F,
    is_retryable: impl Fn(&E) -> bool,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut last_error = None;

    for attempt in 1..=config.max_attempts {
        match f().await {
            Ok(result) => {
                if attempt > 1 {
                    debug!("Operation succeeded on attempt {}", attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                last_error = Some(e);
                
                if attempt < config.max_attempts && is_retryable(last_error.as_ref().unwrap()) {
                    let delay = config.delay_for_attempt(attempt);
                    warn!(
                        "Operation failed on attempt {}/{}, retrying after {:?}",
                        attempt, config.max_attempts, delay
                    );
                    sleep(delay).await;
                } else {
                    break;
                }
            }
        }
    }

    Err(last_error.unwrap())
}

/// Check if an HTTP error is retryable
pub fn is_http_retryable(status: u16) -> bool {
    // Retry on server errors (5xx) and certain client errors
    matches!(status, 408 | 429 | 500..=599)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_delay_calculation() {
        let config = RetryConfig::new(5, Duration::from_millis(100));
        
        // First attempt: 100ms
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(100));
        // Second attempt: 200ms
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(200));
        // Third attempt: 400ms
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(400));
        // Fourth attempt: 800ms
        assert_eq!(config.delay_for_attempt(4), Duration::from_millis(800));
    }

    #[test]
    fn test_retry_config_max_delay() {
        let config = RetryConfig {
            max_attempts: 10,
            initial_delay: Duration::from_millis(100),
            backoff_multiplier: 10.0,
            max_delay: Duration::from_secs(1),
        };
        
        // Should be capped at max_delay
        assert_eq!(config.delay_for_attempt(5), Duration::from_secs(1));
    }

    #[test]
    fn test_is_http_retryable() {
        assert!(is_http_retryable(500));
        assert!(is_http_retryable(502));
        assert!(is_http_retryable(503));
        assert!(is_http_retryable(429)); // Too Many Requests
        assert!(is_http_retryable(408)); // Request Timeout
        assert!(!is_http_retryable(400)); // Bad Request
        assert!(!is_http_retryable(401)); // Unauthorized
        assert!(!is_http_retryable(404)); // Not Found
    }
}