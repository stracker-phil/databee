//! Lock access that survives poisoning. Panics unwind (ADR 0017), so a panic while a guard is
//! held poisons the lock, and an `unwrap()` on every later access would turn one failed command
//! into a broken app. The guarded data is caches and maps of pending requests, each updated in
//! a single call, so it is still consistent after such a panic.

use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub trait MutexExt<T> {
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub trait RwLockExt<T> {
    fn read_unpoisoned(&self) -> RwLockReadGuard<'_, T>;
    fn write_unpoisoned(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    fn read_unpoisoned(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write_unpoisoned(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn poison_mutex(m: &Arc<Mutex<Vec<i32>>>) {
        let m = Arc::clone(m);
        let joined = std::thread::spawn(move || {
            let mut guard = m.lock().unwrap();
            guard.push(2);
            panic!("poison");
        })
        .join();
        assert!(joined.is_err());
    }

    fn poison_rwlock(l: &Arc<RwLock<Vec<i32>>>) {
        let l = Arc::clone(l);
        let joined = std::thread::spawn(move || {
            let mut guard = l.write().unwrap();
            guard.push(2);
            panic!("poison");
        })
        .join();
        assert!(joined.is_err());
    }

    #[test]
    fn lock_unpoisoned_returns_data_after_a_panic() {
        let m = Arc::new(Mutex::new(vec![1]));
        poison_mutex(&m);
        assert!(m.is_poisoned());
        assert_eq!(*m.lock_unpoisoned(), vec![1, 2]);
    }

    #[test]
    fn read_unpoisoned_returns_data_after_a_panic() {
        let l = Arc::new(RwLock::new(vec![1]));
        poison_rwlock(&l);
        assert!(l.is_poisoned());
        assert_eq!(*l.read_unpoisoned(), vec![1, 2]);
    }

    #[test]
    fn write_unpoisoned_allows_mutation_after_a_panic() {
        let l = Arc::new(RwLock::new(vec![1]));
        poison_rwlock(&l);
        l.write_unpoisoned().push(3);
        assert_eq!(*l.read_unpoisoned(), vec![1, 2, 3]);
    }

    #[test]
    fn healthy_locks_behave_normally() {
        let m = Mutex::new(5);
        *m.lock_unpoisoned() += 1;
        assert_eq!(*m.lock_unpoisoned(), 6);
    }
}
