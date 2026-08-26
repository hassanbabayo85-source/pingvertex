use application::ports::{PendingAuthCache, PendingAuthCacheResult};
use async_trait::async_trait;
use redis::AsyncCommands;

/// Redis-backed cache for the TOTP 2FA flow (pending secrets + pending
/// 2FA login tokens).
///
/// Deliberately mirrors `RedisStatusPageCache` byte-for-byte in how it
/// talks to Redis (`redis::Client::open` + `get_multiplexed_async_connection`)
/// — this is the SAME kind of client already used elsewhere in this
/// crate for the status-page cache, just pointed at a different key
/// namespace (`totp_auth:` instead of `status_page:`). No new connection
/// pool is introduced for this feature.
#[derive(Clone)]
pub struct RedisPendingAuthCache {
    client: redis::Client,
}

impl RedisPendingAuthCache {
    pub fn new(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self { client })
    }

    fn namespaced(key: &str) -> String {
        format!("totp_auth:{key}")
    }
}

#[async_trait]
impl PendingAuthCache for RedisPendingAuthCache {
    async fn get(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let value: Option<String> = conn.get(Self::namespaced(key)).await?;
        Ok(value)
    }

    async fn getdel(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        // Redis `GETDEL` returns the value and deletes the key atomically.
        // `redis` crate doesn't expose `getdel` as a typed `AsyncCommands`
        // method, so we use a tiny Lua script via `redis::Script` (which is
        // the same primitive used elsewhere in this crate for rate limiter
        // scripts).
        let script = redis::Script::new("return redis.call('GETDEL', KEYS[1])");
        let value: Option<String> = script
            .key(Self::namespaced(key))
            .invoke_async(&mut conn)
            .await?;
        Ok(value)
    }

    async fn set(&self, key: &str, value: &str, ttl_seconds: u64) -> PendingAuthCacheResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = conn
            .set_ex(Self::namespaced(key), value, ttl_seconds)
            .await?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> PendingAuthCacheResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = conn.del(Self::namespaced(key)).await?;
        Ok(())
    }
}
