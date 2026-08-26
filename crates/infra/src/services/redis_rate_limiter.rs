use application::ports::{RateLimiter, RateLimiterResult};
use async_trait::async_trait;
use redis::Script;

/// Redis-backed distributed fixed-window rate limiter.
///
/// The Lua script makes the increment + first-request TTL assignment atomic.
/// A request is allowed only when the counter remains <= max_requests.
///
/// Redis failures are propagated to the caller so security-sensitive
/// endpoints can fail closed.
#[derive(Clone)]
pub struct RedisRateLimiter {
    client: redis::Client,
}

impl RedisRateLimiter {
    pub fn new(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self { client })
    }

    fn namespaced_key(namespace: &str, key: &str) -> String {
        format!("rate_limit:{namespace}:{key}")
    }
}

#[async_trait]
impl RateLimiter for RedisRateLimiter {
    async fn check_and_record(
        &self,
        namespace: &str,
        key: &str,
        max_requests: u32,
        window_seconds: u64,
    ) -> RateLimiterResult<bool> {
        if max_requests == 0 || window_seconds == 0 {
            return Ok(false);
        }

        let redis_key = Self::namespaced_key(namespace, key);

        // Atomic:
        //   1. increment counter
        //   2. assign TTL only on first request
        //   3. reject once counter exceeds the configured limit
        //
        // No separate INCR/EXPIRE round-trip exists, so concurrent API
        // instances cannot race between the two operations.
        let script = Script::new(
            r#"
            local count = redis.call("INCR", KEYS[1])

            if count == 1 then
                redis.call("EXPIRE", KEYS[1], ARGV[1])
            end

            if count <= tonumber(ARGV[2]) then
                return 1
            end

            return 0
            "#,
        );

        let mut conn = self.client.get_multiplexed_async_connection().await?;

        let allowed: i64 = script
            .key(redis_key)
            .arg(window_seconds)
            .arg(max_requests)
            .invoke_async(&mut conn)
            .await?;

        Ok(allowed == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_namespaced_key() {
        assert_eq!(
            RedisRateLimiter::namespaced_key("forgot_password", "user@example.com"),
            "rate_limit:forgot_password:user@example.com"
        );
    }
}
