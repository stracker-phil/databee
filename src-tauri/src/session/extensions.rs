use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::lock::RwLockExt;

/// A map from type to one shared value of that type, so a module can attach its own data to a
/// session without the core naming the module.
#[derive(Default)]
pub struct Extensions(RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>);

impl Extensions {
    pub fn get<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        let value = self.0.read_unpoisoned().get(&TypeId::of::<T>())?.clone();
        value.downcast().ok()
    }

    /// The value of type `T`, created with `init` on first use.
    pub fn get_or_insert_with<T: Any + Send + Sync>(&self, init: impl FnOnce() -> T) -> Arc<T> {
        if let Some(value) = self.get() {
            return value;
        }
        let mut map = self.0.write_unpoisoned();
        let value = map
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Arc::new(init()))
            .clone();
        value
            .downcast()
            .expect("the entry for a TypeId holds that type")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Sessions;
    use crate::session::testing::session;

    #[test]
    fn get_or_insert_with_runs_init_once_and_returns_the_same_value() {
        let ext = Extensions::default();
        let mut calls = 0;

        let first = ext.get_or_insert_with(|| {
            calls += 1;
            String::from("a")
        });
        let second = ext.get_or_insert_with(|| {
            calls += 1;
            String::from("b")
        });

        assert_eq!(calls, 1);
        assert_eq!(*second, "a");
        assert!(Arc::ptr_eq(&first, &second));
        assert!(Arc::ptr_eq(&first, &ext.get::<String>().unwrap()));
    }

    #[test]
    fn extensions_of_different_types_are_stored_independently() {
        let ext = Extensions::default();

        ext.get_or_insert_with(|| 7u32);
        ext.get_or_insert_with(|| String::from("text"));

        assert_eq!(*ext.get::<u32>().unwrap(), 7);
        assert_eq!(*ext.get::<String>().unwrap(), "text");
    }

    #[test]
    fn get_of_an_absent_type_is_none() {
        let ext = Extensions::default();
        ext.get_or_insert_with(|| 7u32);

        assert!(ext.get::<String>().is_none());
    }

    #[test]
    fn extensions_are_gone_when_the_session_is_removed_and_reopened() {
        struct Marker(u32);

        let sessions = Sessions::default();
        let first = sessions.insert(session("a"));
        first.ext.get_or_insert_with(|| Marker(7));
        assert_eq!(first.ext.get::<Marker>().unwrap().0, 7);

        sessions.remove("a");
        let reopened = sessions.insert(session("b"));

        assert!(reopened.ext.get::<Marker>().is_none());
    }
}
