//! Full values of the long text cells the UI only received as previews, kept per session so
//! `fetch_cell` can return them without running the query again.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::ResultSet;
use crate::error::{AppError, AppResult};
use crate::lock::MutexExt;

/// Text kept across all result sets before the oldest ones are dropped.
const MAX_CACHED_BYTES: usize = 64 * 1024 * 1024;

pub struct CellCache {
    next_id: AtomicU64,
    max_bytes: usize,
    entries: Mutex<Entries>,
}

#[derive(Default)]
struct Entries {
    /// Result ids, oldest first.
    order: VecDeque<u64>,
    cells: HashMap<u64, HashMap<(usize, usize), String>>,
    bytes: usize,
}

impl Default for CellCache {
    fn default() -> Self {
        Self::with_capacity(MAX_CACHED_BYTES)
    }
}

impl CellCache {
    pub fn with_capacity(max_bytes: usize) -> Self {
        Self {
            next_id: AtomicU64::new(0),
            max_bytes,
            entries: Mutex::default(),
        }
    }

    /// Gives `result` a fresh id and moves its full texts into the cache.
    pub fn keep(&self, result: &mut ResultSet) {
        result.id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        for full in std::mem::take(&mut result.full_texts) {
            self.insert(result.id, full.row, full.col, full.text);
        }
    }

    pub fn insert(&self, result_id: u64, row: usize, col: usize, text: String) {
        let mut entries = self.entries.lock_unpoisoned();
        if !entries.cells.contains_key(&result_id) {
            entries.order.push_back(result_id);
        }
        entries.bytes += text.len();
        let replaced = entries
            .cells
            .entry(result_id)
            .or_default()
            .insert((row, col), text);
        entries.bytes -= replaced.map_or(0, |old| old.len());
        entries.evict_beyond(self.max_bytes, result_id);
    }

    pub fn get(&self, result_id: u64, row: usize, col: usize) -> AppResult<String> {
        self.entries
            .lock_unpoisoned()
            .cells
            .get(&result_id)
            .and_then(|cells| cells.get(&(row, col)))
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("Result expired; run the query again".into()))
    }
}

impl Entries {
    /// Drops whole result sets, oldest first, but never `keep`, the one being filled.
    fn evict_beyond(&mut self, max_bytes: usize, keep: u64) {
        while self.bytes > max_bytes
            && let Some(position) = self.order.iter().position(|id| *id != keep)
        {
            let id = self.order[position];
            self.order.remove(position);
            let freed: usize = self
                .cells
                .remove(&id)
                .map_or(0, |cells| cells.values().map(String::len).sum());
            self.bytes -= freed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::FullText;
    use crate::error::ErrorKind;

    fn expired(cache: &CellCache, result_id: u64, row: usize, col: usize) -> bool {
        cache
            .get(result_id, row, col)
            .is_err_and(|e| e.kind() == ErrorKind::InvalidInput)
    }

    #[test]
    fn inserted_cells_are_returned_by_result_row_and_column() {
        let cache = CellCache::default();
        cache.insert(1, 0, 2, "first".into());
        cache.insert(1, 3, 0, "second".into());
        cache.insert(2, 0, 2, "other result".into());

        assert_eq!(cache.get(1, 0, 2).unwrap(), "first");
        assert_eq!(cache.get(1, 3, 0).unwrap(), "second");
        assert_eq!(cache.get(2, 0, 2).unwrap(), "other result");
    }

    #[test]
    fn missing_cell_reports_an_expired_result() {
        let cache = CellCache::default();
        cache.insert(1, 0, 0, "x".into());

        for (result_id, row, col) in [(2, 0, 0), (1, 1, 0), (1, 0, 1)] {
            let err = cache.get(result_id, row, col).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput);
            assert_eq!(
                err.to_string(),
                "Result expired; run the query again",
                "{result_id}/{row}/{col}"
            );
        }
    }

    #[test]
    fn inserting_the_same_cell_again_replaces_its_text() {
        let cache = CellCache::default();
        cache.insert(1, 0, 0, "old".into());
        cache.insert(1, 0, 0, "new".into());

        assert_eq!(cache.get(1, 0, 0).unwrap(), "new");
    }

    #[test]
    fn keep_gives_each_result_a_fresh_id_and_moves_its_full_texts_into_the_cache() {
        let cache = CellCache::default();
        let mut first = ResultSet {
            full_texts: vec![
                FullText {
                    row: 0,
                    col: 1,
                    text: "a".into(),
                },
                FullText {
                    row: 4,
                    col: 0,
                    text: "b".into(),
                },
            ],
            ..ResultSet::default()
        };
        let mut second = ResultSet::default();

        cache.keep(&mut first);
        cache.keep(&mut second);

        assert_ne!(first.id, 0);
        assert_ne!(first.id, second.id);
        assert!(first.full_texts.is_empty());
        assert_eq!(cache.get(first.id, 0, 1).unwrap(), "a");
        assert_eq!(cache.get(first.id, 4, 0).unwrap(), "b");
        assert!(expired(&cache, second.id, 0, 0));
    }

    #[test]
    fn oldest_result_sets_are_evicted_whole_above_the_cap() {
        let cache = CellCache::with_capacity(12);
        cache.insert(1, 0, 0, "1234".into());
        cache.insert(1, 1, 0, "1234".into());
        cache.insert(2, 0, 0, "1234".into());
        assert_eq!(cache.get(1, 0, 0).unwrap(), "1234", "12 bytes fit the cap");
        cache.insert(3, 0, 0, "1234".into());

        assert!(expired(&cache, 1, 0, 0));
        assert!(expired(&cache, 1, 1, 0));
        assert_eq!(cache.get(2, 0, 0).unwrap(), "1234");
        assert_eq!(cache.get(3, 0, 0).unwrap(), "1234");
    }

    #[test]
    fn the_result_being_filled_is_never_evicted() {
        let cache = CellCache::with_capacity(4);
        cache.insert(1, 0, 0, "old".into());
        cache.insert(2, 0, 0, "0123456789".into());
        cache.insert(2, 1, 0, "0123456789".into());

        assert!(expired(&cache, 1, 0, 0));
        assert_eq!(cache.get(2, 0, 0).unwrap(), "0123456789");
        assert_eq!(cache.get(2, 1, 0).unwrap(), "0123456789");
    }

    #[test]
    fn replacing_a_cell_does_not_count_its_old_text_towards_the_cap() {
        let cache = CellCache::with_capacity(10);
        cache.insert(1, 0, 0, "12345678".into());
        cache.insert(1, 0, 0, "12345678".into());
        cache.insert(2, 0, 0, "12".into());

        assert_eq!(cache.get(1, 0, 0).unwrap(), "12345678");
        assert_eq!(cache.get(2, 0, 0).unwrap(), "12");
    }
}
