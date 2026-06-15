/// Rate-limiting helpers.
///
/// Uses Redis sliding-window counters when available.
/// Falls back to a no-op when Redis is not configured (development only).
use crate::error::ApiError;

/// Check a rate limit for a given key (e.g. `"auth:{ip}"`) and increment.
///
/// Returns `ApiError::TooManyRequests` if the limit is exceeded.
/// `window_secs` and `max_requests` define the sliding window.
pub async fn check_and_increment(
    redis: Option<&mut redis::aio::ConnectionManager>,
    key: &str,
    window_secs: u64,
    max_requests: u64,
) -> Result<(), ApiError> {
    let Some(conn) = redis else {
        // No Redis — skip rate limiting in dev.
        return Ok(());
    };

    use redis::AsyncCommands;
    let count: u64 = conn
        .incr(key, 1u64)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    if count == 1 {
        // First request in this window — set expiry.
        let _: () = conn
            .expire(key, window_secs as i64)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    }

    if count > max_requests {
        return Err(ApiError::TooManyRequests);
    }

    Ok(())
}
