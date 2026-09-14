//! PART 10 — bounded, thread-safe cache of compiled user regexes.
//!
//! `AssertionOperator::RegexMatch` evaluations used to call
//! `regex::Regex::new(expected)` on every single check execution. Two
//! problems with that:
//!
//!   1. `Regex::new` is not free; repeated compilation in the worker hot
//!      path wastes CPU for every check of every monitor that happens to
//!      have a regex assertion.
//!   2. The set of distinct patterns is attacker-influenced (a tenant can
//!      create many assertions). Any cache we add must therefore be
//!      strictly bounded — "unbounded repeated compilation" includes
//!      "unbounded cache growth".
//!
//! This module gives us a small LRU keyed by `(AssertionId, pattern)`:
//! the ID alone is not enough, because an assertion's pattern can be
//! edited in place under the same ID (see `Monitor::update_assertion`).
//! If the cached pattern differs from the requested one, we treat it as
//! a miss and recompile, replacing the stale entry.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use domain::shared::ids::AssertionId;
use regex::Regex;

/// Default hard cap on the number of distinct compiled patterns kept in
/// the process-wide cache. Each entry is small (a compiled regex plus the
/// pattern string), so this comfortably fits in memory while bounding the
/// worst case for a hostile tenant that spawns many distinct assertions.
pub const DEFAULT_MAX_ENTRIES: usize = 1024;

/// A compiled regex plus the exact pattern it was compiled from. The
/// pattern is stored alongside the compiled form so we can detect when an
/// assertion's pattern has been edited in place.
#[derive(Clone)]
struct Entry {
    pattern: String,
    regex: Arc<Regex>,
}

struct Inner {
    entries: HashMap<AssertionId, Entry>,
    /// Recency order for LRU eviction. Front = least recently used.
    order: VecDeque<AssertionId>,
}

/// Bounded, thread-safe LRU cache of compiled regexes.
pub struct RegexCache {
    inner: Mutex<Inner>,
    max_entries: usize,
    /// Test-only counter of actual `Regex::new` invocations. Exposed so
    /// tests can prove the cache is doing its job without inspecting
    /// internals.
    compile_count: AtomicU64,
}

impl RegexCache {
    /// Creates a new cache with the given entry cap.
    pub fn new(max_entries: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                order: VecDeque::new(),
            }),
            max_entries,
            compile_count: AtomicU64::new(0),
        }
    }

    /// Returns the compiled regex for `(assertion_id, pattern)`.
    ///
    /// * Cache hit with the same pattern — returns the cached `Arc<Regex>`
    ///   and touches the LRU order.
    /// * Cache hit with a different pattern — recompiles, replaces the
    ///   entry, and returns the fresh `Arc<Regex>`.
    /// * Cache miss — compiles; if the cache is at capacity the least
    ///   recently used entry is evicted first.
    ///
    /// Returns `None` only if the pattern itself is not a valid regex.
    /// By the time this is called from a check, the domain layer has
    /// already validated the pattern, so `None` here is a defensive
    /// fallback (e.g. data written before PART 10 landed).
    pub fn get_or_compile(&self, assertion_id: AssertionId, pattern: &str) -> Option<Arc<Regex>> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        // Fast path: cached entry with the exact same pattern.
        if let Some(entry) = inner.entries.get(&assertion_id)
            && entry.pattern == pattern
        {
            let regex = entry.regex.clone();
            touch(&mut inner, assertion_id);
            return Some(regex);
        }

        // Slow path: compile. We hold the lock across compile to avoid a
        // race where two threads both compile the same ID concurrently.
        // For bounded (<=512 byte, see `MAX_REGEX_PATTERN_LENGTH`) patterns
        // `Regex::new` is fast, so this is fine.
        self.compile_count.fetch_add(1, Ordering::Relaxed);
        let regex = Arc::new(Regex::new(pattern).ok()?);

        // Evict if we're at capacity and this would be a new entry.
        if !inner.entries.contains_key(&assertion_id)
            && inner.entries.len() >= self.max_entries
            && let Some(evicted) = inner.order.pop_front()
        {
            inner.entries.remove(&evicted);
        }

        inner.entries.insert(
            assertion_id,
            Entry {
                pattern: pattern.to_string(),
                regex: regex.clone(),
            },
        );
        touch(&mut inner, assertion_id);
        Some(regex)
    }

    /// Number of distinct entries currently in the cache. Test-only.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .len()
    }

    /// `true` when the cache holds no entries. Exists to satisfy the
    /// `clippy::len_without_is_empty` lint; `len()` is test-only, so
    /// `is_empty()` is test-only too.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total number of `Regex::new` calls this cache has made. Test-only.
    pub fn compile_count(&self) -> u64 {
        self.compile_count.load(Ordering::Relaxed)
    }
}

fn touch(inner: &mut Inner, id: AssertionId) {
    inner.order.retain(|x| x != &id);
    inner.order.push_back(id);
}

/// Process-wide default cache used by `evaluate_assertions`. Bounded at
/// `DEFAULT_MAX_ENTRIES`.
static GLOBAL: OnceLock<RegexCache> = OnceLock::new();

/// Returns the process-wide regex cache, initialising it on first use.
pub fn global() -> &'static RegexCache {
    GLOBAL.get_or_init(|| RegexCache::new(DEFAULT_MAX_ENTRIES))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_hit_returns_same_arc_and_does_not_recompile() {
        let cache = RegexCache::new(16);
        let id = AssertionId::new();

        let first = cache.get_or_compile(id, "^Hello").unwrap();
        let second = cache.get_or_compile(id, "^Hello").unwrap();

        assert!(
            Arc::ptr_eq(&first, &second),
            "must reuse the compiled regex"
        );
        assert_eq!(cache.compile_count(), 1, "must compile exactly once");
    }

    #[test]
    fn cache_invalidates_on_pattern_change() {
        let cache = RegexCache::new(16);
        let id = AssertionId::new();

        let first = cache.get_or_compile(id, "^Hello").unwrap();
        let second = cache.get_or_compile(id, "^Goodbye").unwrap();

        assert!(
            !Arc::ptr_eq(&first, &second),
            "a different pattern must produce a fresh regex"
        );
        assert_eq!(cache.compile_count(), 2);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn invalid_pattern_returns_none_without_caching() {
        let cache = RegexCache::new(16);
        let id = AssertionId::new();

        let result = cache.get_or_compile(id, "[unclosed");
        assert!(result.is_none());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn cache_is_bounded_and_evicts_oldest() {
        let cache = RegexCache::new(2);
        let a = AssertionId::new();
        let b = AssertionId::new();
        let c = AssertionId::new();

        cache.get_or_compile(a, "^a").unwrap();
        cache.get_or_compile(b, "^b").unwrap();
        // Inserting `c` must evict `a` (least recently used).
        cache.get_or_compile(c, "^c").unwrap();

        assert_eq!(cache.len(), 2);

        // `a` is gone; re-requesting it recompiles.
        let before = cache.compile_count();
        cache.get_or_compile(a, "^a").unwrap();
        assert_eq!(cache.compile_count(), before + 1);
    }

    #[test]
    fn touch_moves_entry_to_back_of_lru_order() {
        let cache = RegexCache::new(2);
        let a = AssertionId::new();
        let b = AssertionId::new();
        let c = AssertionId::new();

        cache.get_or_compile(a, "^a").unwrap();
        cache.get_or_compile(b, "^b").unwrap();
        // Touch `a` so it is no longer the LRU victim.
        cache.get_or_compile(a, "^a").unwrap();
        // Now inserting `c` should evict `b`, not `a`.
        cache.get_or_compile(c, "^c").unwrap();

        assert_eq!(cache.len(), 2);
        let before = cache.compile_count();
        // `a` must still be cached.
        cache.get_or_compile(a, "^a").unwrap();
        assert_eq!(cache.compile_count(), before, "`a` should still be cached");
    }
}
