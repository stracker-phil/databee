use std::collections::HashMap;
use std::sync::Mutex;

use super::context::WpContext;
use crate::lock::MutexExt;

/// Detected WordPress contexts of one session, by database name; attached to `Session.ext`
/// so it dies with the session. Databases that are not WordPress are never stored, so one
/// that becomes WordPress is detected on the next call.
#[derive(Default)]
pub struct WpCache(Mutex<HashMap<String, WpContext>>);

impl WpCache {
    pub fn get(&self, database: &str) -> Option<WpContext> {
        self.0.lock_unpoisoned().get(database).cloned()
    }

    pub fn insert(&self, database: &str, context: WpContext) {
        self.0
            .lock_unpoisoned()
            .insert(database.to_string(), context);
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

        cache.insert("shop", context("wp_"));

        assert_eq!(cache.get("shop"), Some(context("wp_")));
        assert_eq!(cache.get("other"), None);
    }

    #[test]
    fn remove_drops_only_that_database() {
        let cache = WpCache::default();
        cache.insert("shop", context("wp_"));
        cache.insert("blog", context("b_"));

        cache.remove("shop");

        assert_eq!(cache.get("shop"), None);
        assert_eq!(cache.get("blog"), Some(context("b_")));
    }

    #[test]
    fn clear_empties_the_cache() {
        let cache = WpCache::default();
        cache.insert("shop", context("wp_"));
        cache.insert("blog", context("b_"));

        cache.clear();

        assert_eq!(cache.get("shop"), None);
        assert_eq!(cache.get("blog"), None);
    }
}
