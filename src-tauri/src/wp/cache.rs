use std::collections::HashMap;
use std::sync::Mutex;

use super::context::WpContext;
use crate::lock::MutexExt;

/// Detected WordPress contexts of one session, by database name; attached to `Session.ext`
/// so it dies with the session. Databases that are not WordPress are never stored, so one
/// that becomes WordPress is detected on the next call. Each entry remembers the session's
/// write count of its database (`Session::writes`) and goes stale when that moves.
#[derive(Default)]
pub struct WpCache(Mutex<HashMap<String, Entry>>);

struct Entry {
    /// `Session::writes` of the database when it was detected.
    writes: u64,
    context: WpContext,
}

impl WpCache {
    pub fn get(&self, database: &str, writes: u64) -> Option<WpContext> {
        let cache = self.0.lock_unpoisoned();
        let entry = cache.get(database)?;
        (entry.writes == writes).then(|| entry.context.clone())
    }

    pub fn insert(&self, database: &str, context: WpContext, writes: u64) {
        self.0
            .lock_unpoisoned()
            .insert(database.to_string(), Entry { writes, context });
    }

    pub fn remove(&self, database: &str) {
        self.0.lock_unpoisoned().remove(database);
    }

    pub fn clear(&self) {
        self.0.lock_unpoisoned().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(prefix: &str) -> WpContext {
        WpContext {
            version: None,
            prefix: prefix.into(),
            sites: vec![],
            wc: None,
            site: Default::default(),
            tables: Default::default(),
        }
    }

    #[test]
    fn inserted_context_is_returned_for_its_database() {
        let cache = WpCache::default();

        cache.insert("shop", context("wp_"), 0);

        assert_eq!(cache.get("shop", 0), Some(context("wp_")));
        assert_eq!(cache.get("other", 0), None);
    }

    #[test]
    fn entry_is_stale_once_the_write_counter_moved() {
        let cache = WpCache::default();
        cache.insert("shop", context("wp_"), 2);

        assert_eq!(cache.get("shop", 2), Some(context("wp_")));
        assert_eq!(cache.get("shop", 3), None);
    }

    #[test]
    fn reinserting_after_a_write_makes_the_new_counter_current() {
        let cache = WpCache::default();
        cache.insert("shop", context("wp_"), 0);

        cache.insert("shop", context("w2_"), 1);

        assert_eq!(cache.get("shop", 1), Some(context("w2_")));
        assert_eq!(cache.get("shop", 0), None);
    }

    #[test]
    fn remove_drops_only_that_database() {
        let cache = WpCache::default();
        cache.insert("shop", context("wp_"), 0);
        cache.insert("blog", context("b_"), 0);

        cache.remove("shop");

        assert_eq!(cache.get("shop", 0), None);
        assert_eq!(cache.get("blog", 0), Some(context("b_")));
    }

    #[test]
    fn clear_empties_the_cache() {
        let cache = WpCache::default();
        cache.insert("shop", context("wp_"), 0);
        cache.insert("blog", context("b_"), 0);

        cache.clear();

        assert_eq!(cache.get("shop", 0), None);
        assert_eq!(cache.get("blog", 0), None);
    }
}
