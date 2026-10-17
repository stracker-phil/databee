//! Live mode: a watched row keeps its last read document in the session (`Session.ext`), and
//! each poll reads it again and returns what changed. Reads go through `document::open`, so they
//! use the metadata pool, never the editor connection.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;
use ts_rs::TS;

use super::Document;
use super::diff::{DocDelta, FlatDocument, diff_documents};
use crate::error::{AppError, AppResult};
use crate::lock::MutexExt;

/// Watches per session; starting one more ends the oldest.
pub const MAX_WATCHES: usize = 8;

/// What one poll found: the delta, and the new document when anything changed.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct WatchPoll {
    pub delta: DocDelta,
    pub document: Option<Document>,
}

/// The watches of one session, oldest first.
#[derive(Default)]
pub struct Watches(Mutex<WatchList>);

#[derive(Default)]
struct WatchList {
    next: u64,
    items: VecDeque<Watch>,
}

struct Watch {
    id: String,
    address: String,
    last: FlatDocument,
}

impl Watches {
    /// Starts watching `address` from `document`; returns the watch id.
    pub fn start(&self, address: String, document: FlatDocument) -> String {
        let mut list = self.0.lock_unpoisoned();
        list.next += 1;
        let id = format!("w{}", list.next);
        list.items.push_back(Watch {
            id: id.clone(),
            address,
            last: document,
        });
        while list.items.len() > MAX_WATCHES {
            list.items.pop_front();
        }
        id
    }

    /// The watched address.
    pub fn address(&self, id: &str) -> AppResult<String> {
        self.0
            .lock_unpoisoned()
            .items
            .iter()
            .find(|w| w.id == id)
            .map(|w| w.address.clone())
            .ok_or_else(|| ended(id))
    }

    /// Keeps `document` as the last one and returns the changes since the previous; a watch that
    /// ended meanwhile stays ended.
    pub fn advance(&self, id: &str, document: FlatDocument) -> AppResult<DocDelta> {
        let mut list = self.0.lock_unpoisoned();
        let watch = list
            .items
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| ended(id))?;
        let delta = diff_documents(&watch.last, &document);
        watch.last = document;
        Ok(delta)
    }

    pub fn stop(&self, id: &str) {
        self.0.lock_unpoisoned().items.retain(|w| w.id != id);
    }
}

fn ended(id: &str) -> AppError {
    AppError::InvalidInput(format!("No watch {id}; it ended"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::diff::fixtures::{prov, status_doc, text};
    use crate::error::ErrorKind;

    fn address(n: usize) -> String {
        format!("databee://local/wp/orders/{n}")
    }

    #[test]
    fn starting_a_ninth_watch_ends_the_first() {
        let watches = Watches::default();
        let ids: Vec<String> = (0..=MAX_WATCHES)
            .map(|n| watches.start(address(n), status_doc("a")))
            .collect();

        let address_err = watches.address(&ids[0]).unwrap_err();
        let advance_err = watches.advance(&ids[0], status_doc("b")).unwrap_err();

        assert_eq!(address_err.kind(), ErrorKind::InvalidInput);
        assert_eq!(advance_err.kind(), ErrorKind::InvalidInput);
        assert_eq!(watches.address(&ids[1]).unwrap(), address(1));
        assert_eq!(
            watches.address(&ids[MAX_WATCHES]).unwrap(),
            address(MAX_WATCHES)
        );
    }

    #[test]
    fn advance_returns_the_diff_and_keeps_the_new_document() {
        let watches = Watches::default();
        let id = watches.start(address(1), status_doc("pending"));

        let first = watches.advance(&id, status_doc("paid")).unwrap();
        let again = watches.advance(&id, status_doc("paid")).unwrap();
        let back = watches.advance(&id, status_doc("pending")).unwrap();

        assert_eq!(first.changed.len(), 1);
        assert_eq!(first.changed[0].source, prov("orders", "status", "id", "1"));
        assert_eq!(first.changed[0].old, text("pending"));
        assert_eq!(first.changed[0].new, text("paid"));
        assert!(again.is_empty());
        assert_eq!(back.changed[0].old, text("paid"));
        assert_eq!(back.changed[0].new, text("pending"));
    }

    #[test]
    fn a_stopped_watch_is_gone_and_the_others_stay() {
        let watches = Watches::default();
        let first = watches.start(address(1), status_doc("a"));
        let second = watches.start(address(2), status_doc("a"));

        watches.stop(&first);

        assert_eq!(
            watches.address(&first).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
        assert_eq!(watches.address(&second).unwrap(), address(2));
    }
}
