use std::future::Future;
use std::time::Duration;
use tracing::{debug, warn, error};

/// Classification of errors for retry strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryableError {
    /// 429 Rate Limit - retry with longer delays
    RateLimit,
    /// 5xx Server Error - retry with medium delays
    ServerError,
    /// Network timeout - retry with shorter delays
    Timeout,
    /// Other errors - don't retry
    Other,
}

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not including initial attempt)
    pub max_attempts: u32,
    /// Delays for rate limit errors (milliseconds) - typically [500, 1000, 2000]
    pub rate_limit_delays_ms: Vec<u64>,
    /// Delays for server errors (milliseconds) - typically [300, 600, 1200]
    pub server_error_delays_ms: Vec<u64>,
    /// Delays for timeout errors (milliseconds) - typically [500, 1000]
    pub timeout_delays_ms: Vec<u64>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            rate_limit_delays_ms: vec![500, 1000, 2000],
            server_error_delays_ms: vec![300, 600, 1200],
            timeout_delays_ms: vec![500, 1000],
        }
    }
}

impl RetryConfig {
    /// Get the delay for a specific retry attempt and error type
    fn get_delay(&self, attempt: u32, error_type: RetryableError) -> Option<Duration> {
        let delays = match error_type {
            RetryableError::RateLimit => &self.rate_limit_delays_ms,
            RetryableError::ServerError => &self.server_error_delays_ms,
            RetryableError::Timeout => &self.timeout_delays_ms,
            RetryableError::Other => return None, // Don't retry
        };

        // attempt is 0-indexed, delays array is also 0-indexed
        delays
            .get(attempt as usize)
            .map(|&delay_ms| Duration::from_millis(delay_ms))
    }
}

/// Retry an async operation with exponential backoff
///
/// # Arguments
/// * `operation` - The async operation to retry (should be a closure that returns a Future)
/// * `config` - Retry configuration
/// * `classify_error` - Function to classify errors for retry strategy
///
/// # Returns
/// * `Ok(T)` - Operation succeeded (either on first attempt or after retries)
/// * `Err(E)` - Operation failed after all retries exhausted
///
/// # Example
/// ```ignore
/// let result = retry_with_backoff(
///     || async { my_api_call().await },
///     &RetryConfig::default(),
///     |e| if e.is_rate_limit() { RetryableError::RateLimit } else { RetryableError::Other }
/// ).await;
/// ```
pub async fn retry_with_backoff<F, Fut, T, E>(
    mut operation: F,
    config: &RetryConfig,
    classify_error: impl Fn(&E) -> RetryableError,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0u32;

    loop {
        // Try the operation
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("✅ Operation succeeded after {} retry attempts", attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                let error_type = classify_error(&e);

                // Check if we should retry
                if error_type == RetryableError::Other {
                    error!("❌ Operation failed with non-retryable error: {}", e);
                    return Err(e);
                }

                // Check if we've exhausted retries
                if attempt >= config.max_attempts {
                    error!(
                        "❌ Operation failed after {} attempts (max retries exhausted): {}",
                        attempt + 1,
                        e
                    );
                    return Err(e);
                }

                // Get delay for this retry
                let delay = match config.get_delay(attempt, error_type) {
                    Some(d) => d,
                    None => {
                        error!("❌ No delay configured for attempt {}, failing", attempt);
                        return Err(e);
                    }
                };

                warn!(
                    "⚠️  Operation failed (attempt {}/{}): {} - Retrying in {}ms (error type: {:?})",
                    attempt + 1,
                    config.max_attempts + 1,
                    e,
                    delay.as_millis(),
                    error_type
                );

                // Wait before retry
                tokio::time::sleep(delay).await;

                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestError {
        kind: &'static str,
    }

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestError: {}", self.kind)
        }
    }

    #[tokio::test]
    async fn test_immediate_success() {
        let result = retry_with_backoff(
            || async { Ok::<_, TestError>(42) },
            &RetryConfig::default(),
            |_| RetryableError::Other,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_non_retryable_error() {
        let mut attempts = 0;
        let result = retry_with_backoff(
            || async {
                attempts += 1;
                Err::<i32, _>(TestError { kind: "fatal" })
            },
            &RetryConfig::default(),
            |_| RetryableError::Other,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(attempts, 1); // Should not retry
    }

    #[tokio::test]
    async fn test_retry_until_success() {
        let mut attempts = 0;
        let result = retry_with_backoff(
            || async {
                attempts += 1;
                if attempts < 3 {
                    Err(TestError { kind: "rate_limit" })
                } else {
                    Ok(42)
                }
            },
            &RetryConfig {
                max_attempts: 3,
                rate_limit_delays_ms: vec![10, 20, 30], // Short delays for testing
                server_error_delays_ms: vec![10, 20, 30],
                timeout_delays_ms: vec![10, 20],
            },
            |_| RetryableError::RateLimit,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts, 3);
    }

    #[tokio::test]
    async fn test_exhausted_retries() {
        let mut attempts = 0;
        let result = retry_with_backoff(
            || async {
                attempts += 1;
                Err::<i32, _>(TestError { kind: "rate_limit" })
            },
            &RetryConfig {
                max_attempts: 2,
                rate_limit_delays_ms: vec![10, 20], // Short delays for testing
                server_error_delays_ms: vec![10, 20],
                timeout_delays_ms: vec![10],
            },
            |_| RetryableError::RateLimit,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(attempts, 3); // Initial + 2 retries
    }
}
