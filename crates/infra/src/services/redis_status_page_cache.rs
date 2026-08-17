use application::ports::{CacheResult, StatusPageCache};
use async_trait::async_trait;
use redis::AsyncCommands;

/// Redis-backed cache for the public status page endpoint.
///
/// This exists specifically to satisfy the task's CACHING requirement:
/// status pages are unauthenticated, publicly linked, and can receive
/// heavy/spiky traffic, so every request must not be allowed to reach the
/// database directly.
#[derive(Clone)]
pub struct RedisStatusPageCache {
    client: redis::Client,
}

impl RedisStatusPageCache {
    pub fn new(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self { client })
    }

    fn namespaced(key: &str) -> String {
        format!("status_page:{key}")
    }
}

#[async_trait]
impl StatusPageCache for RedisStatusPageCache {
    async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let value: Option<String> = conn.get(Self::namespaced(key)).await?;
        Ok(value)
    }

    async fn set(&self, key: &str, value: &str, ttl_seconds: u64) -> CacheResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = conn
            .set_ex(Self::namespaced(key), value, ttl_seconds)
            .await?;
        Ok(())
    }

    async fn invalidate(&self, key: &str) -> CacheResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = conn.del(Self::namespaced(key)).await?;
        Ok(())
    }
}
